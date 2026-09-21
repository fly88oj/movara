// SPDX-License-Identifier: MIT OR Apache-2.0
//! v1.3 S1 sync tests, in-process (the receive_core pattern): pair
//! store, ledger base-advance, the pure merge planner's convergence
//! property, the liveness probe and the lease.

mod common;

use movara::sync::{self, Action, LedgerState, MemberInv, Pair, PlannedMember};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn ctx(tag: &str) -> (movara::ctx::Ctx, std::path::PathBuf) {
    let raw = std::env::temp_dir().join(format!("movara-s1-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&raw);
    fs::create_dir_all(&raw).unwrap();
    let tmp = movara::ctx::de_verbatim(&fs::canonicalize(&raw).unwrap_or(raw));
    let c = movara::ctx::Ctx {
        home: tmp.clone(),
        config_home: tmp.join(".config"),
        data_home: tmp.join(".local").join("share"),
    };
    (c, tmp)
}

fn inv(entries: &[(&str, &str)]) -> BTreeMap<String, MemberInv> {
    entries
        .iter()
        .map(|(k, h)| {
            (
                k.to_string(),
                MemberInv {
                    hash: h.to_string(),
                    mtime: 0,
                },
            )
        })
        .collect()
}

fn planner_action(plan: &[PlannedMember], member: &str) -> Action {
    plan.iter()
        .find(|p| p.member == member)
        .map(|p| p.action.clone())
        .unwrap_or_else(|| panic!("member {member} not in plan"))
}

/// commit with every row trusted (planner-level tests don't run applies)
fn commit(ledger: &mut LedgerState, plan: &[PlannedMember]) {
    let confirmed = sync::confirmed_all(plan);
    sync::commit_plan(ledger, plan, &confirmed, &BTreeMap::new());
}

// -------------------------------------------------------------- pair store

#[test]
fn pair_store_add_list_remove_and_dup_refusals() {
    let (c, tmp) = ctx("pair");
    let p = Pair {
        name: "laptop".into(),
        local: "/home/me/abc".into(),
        remote_host: "me@laptop".into(),
        remote_path: "/home/me/projects/abc".into(),
        with_files: false,
        agents: None,
    };
    sync::pair_add(&c, &p).unwrap();
    assert_eq!(sync::pair_list(&c).unwrap().len(), 1);
    // duplicate NAME refused
    let e = sync::pair_add(
        &c,
        &Pair {
            local: "/x".into(),
            remote_host: "h2".into(),
            remote_path: "/y".into(),
            ..p.clone()
        },
    );
    assert!(e.is_err());
    // duplicate ENDPOINT tuple under another name refused
    let e = sync::pair_add(
        &c,
        &Pair {
            name: "other".into(),
            ..p.clone()
        },
    );
    assert!(e.is_err());
    // same endpoints but different remote host is a NEW pair (allowed)
    sync::pair_add(
        &c,
        &Pair {
            name: "desktop".into(),
            remote_host: "me@desktop".into(),
            ..p
        },
    )
    .unwrap();
    assert_eq!(sync::pair_list(&c).unwrap().len(), 2);
    assert!(sync::pair_remove(&c, "laptop").unwrap());
    assert!(!sync::pair_remove(&c, "laptop").unwrap());
    assert_eq!(sync::pair_list(&c).unwrap().len(), 1);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn pair_id_is_endpoint_tuple() {
    let mk = |l: &str, h: &str, r: &str| Pair {
        name: "n".into(),
        local: l.into(),
        remote_host: h.into(),
        remote_path: r.into(),
        with_files: false,
        agents: None,
    };
    assert_eq!(
        sync::pair_id(&mk("/a", "h", "/b")),
        sync::pair_id(&mk("/a", "h", "/b"))
    );
    assert_ne!(
        sync::pair_id(&mk("/a", "h", "/b")),
        sync::pair_id(&mk("/a", "h2", "/b"))
    );
    // the NAME is not part of identity
    let mut renamed = mk("/a", "h", "/b");
    renamed.name = "zzz".into();
    assert_eq!(sync::pair_id(&mk("/a", "h", "/b")), sync::pair_id(&renamed));
}

// ------------------------------------------------------------- ledger hash

#[test]
fn norm_hash_bom_crlf_nfc_equivalence() {
    let plain = b"{\"cwd\":\"/p/abc\"}\n";
    let bom = [0xEF, 0xBB, 0xBF]
        .iter()
        .chain(plain.iter())
        .copied()
        .collect::<Vec<u8>>();
    let crlf = b"{\"cwd\":\"/p/abc\"}\r\n";
    // NFD vs NFC of é inside a path
    let nfc_path = "/p/proje\u{e9}ct";
    let nfd_path = "/p/proje\u{65}\u{301}ct";
    let nfc_json = format!("{{\"cwd\":\"{nfc_path}\"}}\n").into_bytes();
    let nfd_json = format!("{{\"cwd\":\"{nfd_path}\"}}\n").into_bytes();

    let h = sync::norm_hash(plain);
    assert_eq!(h, sync::norm_hash(&bom), "BOM must not change the hash");
    assert_eq!(h, sync::norm_hash(crlf), "CRLF must not change the hash");
    assert_eq!(
        sync::norm_hash(&nfc_json),
        sync::norm_hash(&nfd_json),
        "NFD must compose to NFC pre-hash"
    );
    // content differences still hash differently
    assert_ne!(h, sync::norm_hash(b"{\"cwd\":\"/p/cba\"}\n"));
}

// ------------------------------------------------------------- the planner

#[test]
fn planner_convergence_property() {
    // random-ish state on both sides; one plan; then a second plan
    // against the COMMITTED ledger must be a pure no-op fixpoint
    let mut ledger = LedgerState {
        schema: 1,
        pair_id: "pair-x".into(),
        ..Default::default()
    };
    // first sync: everything is absent-base or absent-on-one-side
    let a = inv(&[
        ("claude/home/.claude/projects/x/s1.jsonl", "h-a1"),
        ("claude/home/.claude/projects/x/s2.jsonl", "h-a2"), // A-only
        ("claude/home/.claude/projects/x/s3.jsonl", "h-shared"),
    ]);
    let b = inv(&[
        ("claude/home/.claude/projects/x/s1.jsonl", "h-b1"), // diverged vs A
        ("claude/home/.claude/projects/x/s4.jsonl", "h-b4"), // B-only
        ("claude/home/.claude/projects/x/s3.jsonl", "h-shared"),
    ]);
    let plan1 = sync::plan(&a, &b, &ledger);
    // s1: both present, absent base → ConflictWinnerA (deterministic)
    assert!(matches!(
        planner_action(&plan1, "claude/home/.claude/projects/x/s1.jsonl"),
        Action::ConflictWinnerA { content } if content == "h-a1"
    ));
    // s2 A-only → CopyToB; s4 B-only → CopyToA
    assert!(matches!(
        planner_action(&plan1, "claude/home/.claude/projects/x/s2.jsonl"),
        Action::CopyToB
    ));
    assert!(matches!(
        planner_action(&plan1, "claude/home/.claude/projects/x/s4.jsonl"),
        Action::CopyToA
    ));
    // s3 identical → Noop
    assert!(matches!(
        planner_action(&plan1, "claude/home/.claude/projects/x/s3.jsonl"),
        Action::Noop
    ));

    // commit: symmetric base-advance
    commit(&mut ledger, &plan1);
    // convergence: the next run's inventories show the applied state —
    // winner content on both sides for conflicts, copies landed
    let a2 = inv(&[
        ("claude/home/.claude/projects/x/s1.jsonl", "h-a1"),
        ("claude/home/.claude/projects/x/s2.jsonl", "h-a2"),
        ("claude/home/.claude/projects/x/s3.jsonl", "h-shared"),
        ("claude/home/.claude/projects/x/s4.jsonl", "h-b4"),
    ]);
    let b2 = a2.clone();
    let plan2 = sync::plan(&a2, &b2, &ledger);
    assert!(
        plan2.iter().all(|p| p.action == Action::Noop),
        "second plan must be a pure no-op fixpoint"
    );
}

#[test]
fn planner_fast_forward_and_deletion_skew() {
    let mut ledger = LedgerState {
        schema: 1,
        pair_id: "pair-y".into(),
        ..Default::default()
    };
    // establish base v1 everywhere
    let base_inv = inv(&[("m", "v1")]);
    let p0 = sync::plan(&base_inv, &base_inv, &ledger);
    commit(&mut ledger, &p0);
    // A advances to v2, B stays at base → fast-forward B
    let a = inv(&[("m", "v2")]);
    let b = inv(&[("m", "v1")]);
    let p1 = sync::plan(&a, &b, &ledger);
    assert!(matches!(
        planner_action(&p1, "m"),
        Action::FastForwardToB { content } if content == "v2"
    ));
    commit(&mut ledger, &p1);
    // base advanced: a NEW one-sided edit after a fast-forward does NOT
    // false-conflict (the base-advance regression pin)
    let a = inv(&[("m", "v3")]);
    let b = inv(&[("m", "v2")]);
    let p2 = sync::plan(&a, &b, &ledger);
    assert!(matches!(
        planner_action(&p2, "m"),
        Action::FastForwardToB { content } if content == "v3"
    ));
    commit(&mut ledger, &p2);
    // deletion skew: B deleted, A unchanged → SkipDeleted (no propagation)
    let a = inv(&[("m", "v3")]);
    let b: BTreeMap<String, MemberInv> = BTreeMap::new();
    let p3 = sync::plan(&a, &b, &ledger);
    assert!(matches!(planner_action(&p3, "m"), Action::SkipDeleted));
}

#[test]
fn planner_crash_fixpoint_absorbs_equality() {
    // applied-both-but-unledgered: hashes equal, base still old → Noop
    let mut ledger = LedgerState {
        schema: 1,
        pair_id: "pair-z".into(),
        ..Default::default()
    };
    ledger.members.insert(
        "m".into(),
        movara::sync::MemberRow {
            base: Some("v1".into()),
            ..Default::default()
        },
    );
    let both = inv(&[("m", "v2")]);
    let p = sync::plan(&both, &both, &ledger);
    assert!(matches!(planner_action(&p, "m"), Action::Noop));
    // and the commit adopts equality as the new base
    commit(&mut ledger, &p);
    assert_eq!(ledger.members["m"].base.as_deref(), Some("v2"));
}

// ------------------------------------------------------------ live probe

#[test]
fn probe_reports_hot_on_fresh_state() {
    let (c, tmp) = ctx("probe");
    // a claude session file touched RIGHT NOW referencing the project
    let dir = c.h(".claude/projects/enc");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("s.jsonl"),
        format!("{{\"cwd\":\"{}\"}}\n", "/nowhere/proj/abc"),
    )
    .unwrap();
    let list = movara::adapters::get_adapters(None).unwrap();
    let rep = sync::probe(&c, &list, "/nowhere/proj/abc", 90);
    assert!(rep.hot, "fresh member must make the probe hot");
    assert!(
        !rep.fresh_members.is_empty(),
        "the referencing session file must be listed"
    );
    // a different project path has no referencing members: the gate
    // rests on WAL/processes only (freshness 0 = nothing counts as fresh)
    let rep2 = sync::probe(&c, &list, "/other/project", 0);
    assert!(rep2.fresh_members.is_empty());
    assert!(rep2.processes.is_empty());
    assert!(!rep2.hot);
    let _ = fs::remove_dir_all(&tmp);
}

// ----------------------------------------------------------------- lease

#[test]
fn lease_acquire_renew_release_and_expiry_takeover() {
    let (c, tmp) = ctx("lease");
    let l = sync::Lease::acquire(&c, "pair-t", 60)
        .unwrap()
        .expect("first acquire");
    // held
    assert!(sync::Lease::acquire(&c, "pair-t", 60).unwrap().is_none());
    l.renew(60).unwrap();
    assert!(sync::Lease::acquire(&c, "pair-t", 60).unwrap().is_none());
    l.release();
    assert!(sync::Lease::acquire(&c, "pair-t", 60).unwrap().is_some());
    // expired lease is takeable: write a past timestamp directly
    fs::write(c.home.join(".movara/sync/pair-t/lock"), "1").unwrap();
    assert!(sync::Lease::acquire(&c, "pair-t", 60).unwrap().is_some());
    let _ = fs::remove_dir_all(&tmp);
}

// ---------------------------------------------- inventory (member identity)

#[test]
fn inventory_enumerates_referencing_members_and_excludes_artifacts() {
    let fx = common::Fixture::new("s1-inv");
    let ledger = movara::sync::Ledger {
        dir: fx.tmp.join("led"),
        state: LedgerState {
            schema: 1,
            pair_id: "p".into(),
            ..Default::default()
        },
    };
    let list = movara::adapters::get_adapters(None).unwrap();
    let inv = movara::sync::inventory(&fx.ctx, &list, &fx.old, "/srv/other", &ledger);
    assert!(
        !inv.is_empty(),
        "fixture state referencing the project must enumerate"
    );
    // key shape: agent/kind/rel (canonical portable form)
    assert!(inv.keys().any(|k| k.starts_with("claude/home/.claude/")));
    // artifact exclusion: drop a conflict copy — not enumerated
    let conflict = fx.ctx.h(".claude/projects/x.movara-conflict-h-1.jsonl");
    fs::write(&conflict, format!("{{\"cwd\":\"{}\"}}", fx.old)).unwrap();
    let inv2 = movara::sync::inventory(&fx.ctx, &list, &fx.old, "/srv/other", &ledger);
    assert!(inv2.keys().all(|k| !k.contains(".movara-conflict-")));
    // recorded sibling exemption: ledger-recorded conflict copies DO
    let mut state = ledger.state.clone();
    state.members.insert(
        "claude/home/.claude/projects/x.movara-conflict-h-1.jsonl".into(),
        movara::sync::MemberRow {
            sibling: Some("x.movara-conflict-h-1.jsonl".into()),
            ..Default::default()
        },
    );
    let ledger2 = movara::sync::Ledger {
        dir: ledger.dir.clone(),
        state,
    };
    let inv3 = movara::sync::inventory(&fx.ctx, &list, &fx.old, "/srv/other", &ledger2);
    assert!(
        inv3.keys().any(|k| k.contains(".movara-conflict-")),
        "ledger-recorded siblings enumerate"
    );
    let _ = fs::remove_dir_all(&fx.tmp);
}

// ------------------------------------------------------- end-to-end (S1)

/// two hosts (A initiator, B remote) with ASYMMETRIC project paths, one
/// codex session member on each. The loop mirrors what `movara sync`
/// drives over ssh — in-process here.
struct E2E {
    ca: movara::ctx::Ctx,
    cb: movara::ctx::Ctx,
    tmpa: PathBuf,
    tmpb: PathBuf,
    pa: String,
    pb: String,
    list: Vec<Box<dyn movara::adapters::Adapter>>,
    la: LedgerState,
    lb: LedgerState,
}

const MEMBER: &str = "codex/home/.codex/sessions/2026/r.jsonl";

/// every form a path can appear in inside a member's bytes (raw,
/// JSON-escaped, forward-slash, msys). Windows JSON files carry doubled
/// backslashes, so a plain `contains(raw)` assert is wrong there.
fn path_forms(p: &str) -> Vec<String> {
    let mut v = vec![p.to_string(), p.replace('\\', "\\\\"), p.replace('\\', "/")];
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        let fwd = p.replace('\\', "/");
        v.push(format!(
            "/{}{}",
            (b[0] as char).to_ascii_lowercase(),
            &fwd[2..]
        ));
    }
    v
}

fn names_path(hay: &str, p: &str) -> bool {
    path_forms(p).iter().any(|f| hay.contains(f))
}

fn e2e_new(tag: &str) -> E2E {
    let (ca, tmpa) = ctx(&format!("{tag}-a"));
    let (cb, tmpb) = ctx(&format!("{tag}-b"));
    let pa_dir = tmpa.join("proj").join("abc");
    let pb_dir = tmpb.join("work").join("abc");
    fs::create_dir_all(&pa_dir).unwrap();
    fs::create_dir_all(&pb_dir).unwrap();
    let names = vec!["codex".to_string()];
    E2E {
        ca,
        cb,
        tmpa,
        tmpb,
        pa: pa_dir.to_string_lossy().into_owned(),
        pb: pb_dir.to_string_lossy().into_owned(),
        list: movara::adapters::get_adapters(Some(&names)).unwrap(),
        la: LedgerState {
            schema: 1,
            pair_id: "pair-e2e".into(),
            ..Default::default()
        },
        lb: LedgerState {
            schema: 1,
            pair_id: "pair-e2e".into(),
            ..Default::default()
        },
    }
}

impl E2E {
    /// seed a codex session file referencing `cwd` with the given body
    fn seed(&self, on_a: bool, rel: &str, cwd: &str, body: &str) -> PathBuf {
        let c = if on_a { &self.ca } else { &self.cb };
        let p = c.h(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let meta = serde_json::json!({"type": "session_meta", "payload": {"id": "u1", "cwd": cwd}});
        let msg = serde_json::json!({
            "type": "response_item",
            "payload": {"type": "message", "content": body}
        });
        fs::write(&p, format!("{meta}\n{msg}\n")).unwrap();
        p
    }

    fn inv_a(&self) -> BTreeMap<String, MemberInv> {
        sync::inventory(
            &self.ca,
            &self.list,
            &self.pa,
            &self.pb,
            &sync::Ledger {
                dir: self.ca.home.join(".movara/sync/x"),
                state: self.la.clone(),
            },
        )
    }

    fn inv_b(&self) -> BTreeMap<String, MemberInv> {
        sync::inventory(
            &self.cb,
            &self.list,
            &self.pb,
            &self.pa,
            &sync::Ledger {
                dir: self.cb.home.join(".movara/sync/x"),
                state: self.lb.clone(),
            },
        )
    }

    /// one full sync round: plan, stage both directions from disk, apply
    /// both halves, commit both ledgers
    #[allow(clippy::type_complexity)]
    fn round(&mut self, run: &str) -> (Vec<PlannedMember>, sync::ApplyReport, sync::ApplyReport) {
        let plan = sync::plan(&self.inv_a(), &self.inv_b(), &self.la);
        let mut st_a = sync::Staging::new();
        let mut st_b = sync::Staging::new();
        for pm in &plan {
            match &pm.action {
                Action::FastForwardToA { .. } | Action::CopyToA => {
                    if let Some(r) = sync::read_member(&self.cb, &self.list, &pm.member) {
                        st_a.insert(pm.member.clone(), r);
                    }
                }
                Action::FastForwardToB { .. }
                | Action::CopyToB
                | Action::ConflictWinnerA { .. } => {
                    if let Some(r) = sync::read_member(&self.ca, &self.list, &pm.member) {
                        st_b.insert(pm.member.clone(), r);
                    }
                }
                _ => {}
            }
        }
        let mut bk_a = movara::backup::Backup::new(
            &self.tmpa.join("backups"),
            &movara::spec::ReplaceSpec::new(&self.pb, &self.pa).unwrap(),
            vec![],
            false,
        );
        let mut bk_b = movara::backup::Backup::new(
            &self.tmpb.join("backups"),
            &movara::spec::ReplaceSpec::new(&self.pa, &self.pb).unwrap(),
            vec![],
            false,
        );
        let ra = sync::apply_half(
            &self.ca,
            &self.list,
            &self.pa,
            &self.pb,
            &plan,
            true,
            &st_a,
            &mut self.la,
            &mut bk_a,
            run,
        )
        .unwrap();
        let rb = sync::apply_half(
            &self.cb,
            &self.list,
            &self.pb,
            &self.pa,
            &plan,
            false,
            &st_b,
            &mut self.lb,
            &mut bk_b,
            run,
        )
        .unwrap();
        let confirmed = sync::confirmed_members(&plan, &ra.applied, &rb.applied);
        sync::commit_plan(&mut self.la, &plan, &confirmed, &rb.siblings);
        sync::commit_plan(&mut self.lb, &plan, &confirmed, &rb.siblings);
        (plan, ra, rb)
    }

    fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.tmpa);
        let _ = fs::remove_dir_all(&self.tmpb);
    }
}

#[test]
fn e2e_asymmetric_paths_converge_without_ping_pong() {
    let mut e = e2e_new("conv");
    e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "shared body");
    e.seed(false, ".codex/sessions/2026/r.jsonl", &e.pb, "shared body");

    // the path-asymmetry pin: same logical content, different embedded
    // paths → the path-canonical hash makes it ONE no-op row (without
    // canonicalization these two files hash differently and the pair
    // would fast-forward ping-pong forever)
    let (p1, _, _) = e.round("r1");
    assert!(matches!(planner_action(&p1, MEMBER), Action::Noop));
    assert_eq!(e.la.members[MEMBER].base, e.lb.members[MEMBER].base);

    // A edits; B is at base → fast-forward B with a rebase
    let new_body = "shared body v2 from A";
    e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, new_body);
    let (p2, ra, rb) = e.round("r2");
    assert!(matches!(
        planner_action(&p2, MEMBER),
        Action::FastForwardToB { .. }
    ));
    assert_eq!(rb.replaced, 1, "B overwrote its base copy");
    assert_eq!(ra.replaced, 0, "A wrote nothing");
    // the received copy is REBASED: it names B's path, never A's
    let on_b = e.cb.h(".codex/sessions/2026/r.jsonl");
    let got = fs::read_to_string(&on_b).unwrap();
    assert!(
        names_path(&got, &e.pb),
        "winner content must reference B's path"
    );
    assert!(
        !names_path(&got, &e.pa),
        "A's path must not survive the rebase"
    );
    assert!(got.contains(new_body));

    // convergence: the next plan over fresh inventories is a pure no-op
    let (p3, _, _) = e.round("r3");
    assert!(
        p3.iter().all(|p| p.action == Action::Noop),
        "third round must be the fixpoint"
    );
    e.cleanup();
}

#[test]
fn e2e_conflict_keeps_loser_sibling_and_ledgers_agree() {
    let mut e = e2e_new("conf");
    e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "A says one");
    e.seed(false, ".codex/sessions/2026/r.jsonl", &e.pb, "B says two");

    // no base, both diverged → deterministic A-winner, B keeps a sibling
    let (p1, ra, rb) = e.round("r1");
    assert!(matches!(
        planner_action(&p1, MEMBER),
        Action::ConflictWinnerA { .. }
    ));
    assert_eq!(rb.conflicts_local_kept, 1);
    assert!(
        ra.applied.is_empty(),
        "A holds the winner and writes nothing"
    );
    assert_eq!(rb.applied, vec![MEMBER.to_string()]);

    let on_b = e.cb.h(".codex/sessions/2026/r.jsonl");
    let got = fs::read_to_string(&on_b).unwrap();
    assert!(got.contains("A says one"), "A's content won on B");
    assert!(names_path(&got, &e.pb), "rebased onto B's path");
    let sib =
        e.cb.h(".codex/sessions/2026")
            .join(sync::sibling_name(MEMBER, "r1"));
    let loser = fs::read_to_string(&sib).unwrap();
    assert!(loser.contains("B says two"), "B's loser bytes survive");
    assert!(names_path(&loser, &e.pb), "loser keeps B's own path");

    // round 2: the member is a no-op; the recorded sibling is a union
    // member (the survivor copy) that flows B -> A
    let (p2, _, _) = e.round("r2");
    assert!(matches!(planner_action(&p2, MEMBER), Action::Noop));
    let sib_key = format!(
        "codex/home/.codex/sessions/2026/{}",
        sync::sibling_name(MEMBER, "r1")
    );
    assert!(
        matches!(planner_action(&p2, &sib_key), Action::CopyToA),
        "the recorded sibling must flow to A as a union member"
    );
    // round 3: everything converged, both ledgers structurally identical
    let (p3, _, _) = e.round("r3");
    assert!(p3.iter().all(|p| p.action == Action::Noop));
    assert_eq!(e.la.members, e.lb.members);
    // the survivor copy on A is rebased to A's path
    let sib_on_a =
        e.ca.h(".codex/sessions/2026")
            .join(sync::sibling_name(MEMBER, "r1"));
    let loser_on_a = fs::read_to_string(&sib_on_a).unwrap();
    assert!(loser_on_a.contains("B says two"));
    assert!(
        names_path(&loser_on_a, &e.pa),
        "survivor copy rebased to A's path"
    );
    e.cleanup();
}

#[test]
fn e2e_prestate_guard_skips_member_changed_after_plan() {
    let mut e = e2e_new("guard");
    e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "v1 body");
    e.seed(false, ".codex/sessions/2026/r.jsonl", &e.pb, "v1 body");
    e.round("r0"); // establish the base (a no-op row adopting equality)

    // A advances to v2; plan says fast-forward B — but B's file changes
    // BETWEEN inventory and apply
    e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "v2 body");
    let plan = sync::plan(&e.inv_a(), &e.inv_b(), &e.la);
    assert!(matches!(
        planner_action(&plan, MEMBER),
        Action::FastForwardToB { .. }
    ));
    e.seed(
        false,
        ".codex/sessions/2026/r.jsonl",
        &e.pb.clone(),
        "v3 rogue edit",
    );

    // apply against the mutated pre-state: B must refuse the write
    let mut st_b = sync::Staging::new();
    st_b.insert(
        MEMBER.to_string(),
        sync::read_member(&e.ca, &e.list, MEMBER).unwrap(),
    );
    let mut bk_b = movara::backup::Backup::new(
        &e.tmpb.join("backups"),
        &movara::spec::ReplaceSpec::new(&e.pa, &e.pb).unwrap(),
        vec![],
        false,
    );
    let rb = sync::apply_half(
        &e.cb, &e.list, &e.pb, &e.pa, &plan, false, &st_b, &mut e.lb, &mut bk_b, "rg",
    )
    .unwrap();
    assert_eq!(rb.skipped, 1, "the mutated member is refused");
    assert!(rb.applied.is_empty());
    let on_b = fs::read_to_string(e.cb.h(".codex/sessions/2026/r.jsonl")).unwrap();
    assert!(on_b.contains("v3 rogue edit"), "B's bytes are untouched");

    // the unconfirmed row must NOT advance the base
    let confirmed = sync::confirmed_members(&plan, &[], &rb.applied);
    sync::commit_plan(&mut e.la, &plan, &confirmed, &rb.siblings);
    assert_eq!(e.la.members[MEMBER].base, e.lb.members[MEMBER].base);
    // next round over the real state: both diverged from base → conflict
    let (p2, _, _) = e.round("r2");
    assert!(matches!(
        planner_action(&p2, MEMBER),
        Action::ConflictWinnerA { .. }
    ));
    // …applying it lands the winner and keeps B's rogue edit as the
    // sibling; that survivor then flows to A, and the NEXT round is the
    // pure fixpoint
    let (p3, _, _) = e.round("r3");
    assert!(matches!(planner_action(&p3, MEMBER), Action::Noop));
    let sib_key = format!(
        "codex/home/.codex/sessions/2026/{}",
        sync::sibling_name(MEMBER, "r2")
    );
    assert!(matches!(planner_action(&p3, &sib_key), Action::CopyToA));
    let (p4, _, _) = e.round("r4");
    assert!(
        p4.iter().all(|p| p.action == Action::Noop),
        "fourth round must be the fixpoint"
    );
    e.cleanup();
}

// ------------------------------------------------- perform_sync (the CLI core)

/// the remote half in-process: exactly what `movara sync-agent` does on
/// the other host, minus the ssh hop and the base64 wire format
struct LocalTransport<'a> {
    cb: &'a movara::ctx::Ctx,
    list: &'a [Box<dyn movara::adapters::Adapter>],
}

impl movara::cli::SyncTransport for LocalTransport<'_> {
    fn probe(
        &self,
        project: &str,
        freshness: u64,
        _agents: Option<&str>,
    ) -> Result<sync::ProbeReport, anyhow::Error> {
        Ok(sync::probe(self.cb, self.list, project, freshness))
    }
    fn inventory(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        _agents: Option<&str>,
    ) -> Result<BTreeMap<String, MemberInv>, anyhow::Error> {
        let ledger = sync::Ledger::load(self.cb, pair_id)?;
        Ok(sync::inventory(self.cb, self.list, project, other, &ledger))
    }
    fn pack(
        &self,
        _project: &str,
        members: &[String],
        _agents: Option<&str>,
    ) -> Result<sync::Staging, anyhow::Error> {
        let mut s = sync::Staging::new();
        for m in members {
            if let Some(raw) = sync::read_member(self.cb, self.list, m) {
                s.insert(m.clone(), raw);
            }
        }
        Ok(s)
    }
    fn apply(
        &self,
        project: &str,
        other: &str,
        pair_id: &str,
        run_id: &str,
        side_a: bool,
        plan: &[PlannedMember],
        staging: &sync::Staging,
        _agents: Option<&str>,
    ) -> Result<sync::ApplyReport, anyhow::Error> {
        let mut ledger = sync::Ledger::load(self.cb, pair_id)?;
        let mut backup = movara::backup::Backup::new(
            &self.cb.home.join("backups"),
            &movara::spec::ReplaceSpec::new(other, project)?,
            vec![],
            false,
        );
        let rep = sync::apply_half(
            self.cb,
            self.list,
            project,
            other,
            plan,
            side_a,
            staging,
            &mut ledger.state,
            &mut backup,
            run_id,
        )?;
        ledger.save()?;
        backup.save()?;
        Ok(rep)
    }
    fn commit(&self, pair_id: &str, ledger: &LedgerState) -> Result<(), anyhow::Error> {
        sync::Ledger {
            dir: self.cb.home.join(".movara").join("sync").join(pair_id),
            state: ledger.clone(),
        }
        .save()
    }
}

/// backdate a file outside the probe's freshness window (std only,
/// File::set_times is stable on our 1.75 MSRV)
fn backdate(p: &std::path::Path) {
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.set_times(
        std::fs::FileTimes::new()
            .set_accessed(past)
            .set_modified(past),
    )
    .unwrap();
}

/// backdate every file under a tree (out of the probe's window)
fn backdate_all(dir: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_file() {
            backdate(&p);
        } else if p.is_dir() {
            backdate_all(&p);
        }
    }
}

#[test]
fn perform_sync_orchestrates_gates_and_commits() {
    let e = e2e_new("orch");
    let fa = e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "orch body");
    let fb = e.seed(
        false,
        ".codex/sessions/2026/r.jsonl",
        &e.pb,
        "orch body v2 on B",
    );
    backdate(&fa);
    backdate(&fb);
    let pair = Pair {
        name: "lab".into(),
        local: e.pa.clone(),
        remote_host: "b".into(),
        remote_path: e.pb.clone(),
        with_files: false,
        agents: Some(vec!["codex".into()]),
    };
    let tr = LocalTransport {
        cb: &e.cb,
        list: &e.list,
    };

    // dry-run plans only: no writes, no backup journal
    let dry = movara::cli::SyncOpts {
        freshness: 90,
        dry_run: true,
        backup_dir: Some(e.tmpa.join("bk")),
    };
    let out = movara::cli::perform_sync(&e.ca, &pair, &e.list, &tr, &dry).unwrap();
    assert_eq!(out.counts.conflicts, 1, "diverged first observation");
    assert!(out.backup_id.is_none());

    // live run: B receives A's winner (rebased), keeps its loser sibling,
    // and A's authoritative ledger lands on both hosts
    let opts = movara::cli::SyncOpts {
        freshness: 90,
        dry_run: false,
        backup_dir: Some(e.tmpa.join("bk")),
    };
    let out = movara::cli::perform_sync(&e.ca, &pair, &e.list, &tr, &opts).unwrap();
    assert!(out.backup_id.is_some(), "A's undo journal exists");
    assert_eq!(out.b.conflicts_local_kept, 1);
    let pid = sync::pair_id(&pair);
    let la = sync::Ledger::load(&e.ca, &pid).unwrap();
    let lb = sync::Ledger::load(&e.cb, &pid).unwrap();
    assert_eq!(
        la.state.members, lb.state.members,
        "A's authoritative ledger shipped to B"
    );

    // re-run: converged member is a no-op row; the recorded sibling is
    // a union member flowing to A
    backdate_all(&e.ca.h(".codex"));
    backdate_all(&e.cb.h(".codex"));
    let out = movara::cli::perform_sync(&e.ca, &pair, &e.list, &tr, &opts).unwrap();
    assert_eq!(out.counts.noop, 1);
    assert_eq!(out.counts.copy_to_a, 1);

    // third run acquires the lease again (the prior run released it) and
    // hits the full fixpoint
    backdate_all(&e.ca.h(".codex"));
    backdate_all(&e.cb.h(".codex"));
    let out = movara::cli::perform_sync(&e.ca, &pair, &e.list, &tr, &opts).unwrap();
    assert_eq!(out.counts.noop, 2, "member and survivor both converged");
    e.cleanup();
}

#[test]
fn perform_sync_refuses_when_remote_is_hot() {
    let e = e2e_new("hot");
    let fa = e.seed(true, ".codex/sessions/2026/r.jsonl", &e.pa, "hot gate body");
    let fb = e.seed(
        false,
        ".codex/sessions/2026/r.jsonl",
        &e.pb,
        "hot gate body",
    );
    backdate(&fa);
    let _ = fb; // B's member stays FRESH: the remote gate must fire
    let pair = Pair {
        name: "lab".into(),
        local: e.pa.clone(),
        remote_host: "b".into(),
        remote_path: e.pb.clone(),
        with_files: false,
        agents: Some(vec!["codex".into()]),
    };
    let tr = LocalTransport {
        cb: &e.cb,
        list: &e.list,
    };
    let opts = movara::cli::SyncOpts {
        freshness: 90,
        dry_run: false,
        backup_dir: Some(e.tmpa.join("bk")),
    };
    let err = movara::cli::perform_sync(&e.ca, &pair, &e.list, &tr, &opts).unwrap_err();
    assert!(
        err.to_string().contains("agent activity"),
        "the refusal must name the live gate, got: {err}"
    );
    e.cleanup();
}
