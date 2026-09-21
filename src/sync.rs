// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cross-host sync (v1.3 S1): pair store, live probe, per-pair ledger,
//! pure merge planner and the apply/commit halves. The protocol steps
//! are library functions taking a Ctx — the CLI drives them over ssh in
//! production, tests drive both halves in-process (the receive_core
//! pattern).
//!
//! Load-bearing contracts (design rev 7):
//! - canonical member identity is pair-rule-derived, never decoded
//! - first sync uses per-member ABSENT bases when both sides have
//!   content (nothing is silently overwritten)
//! - A == B is a no-op row (the crash-recovery fixpoint)
//! - conflict rows resolve SYMMETRICALLY: the planner picks the A-side
//!   winner, both ledgers adopt hash(winner), the loser survives only
//!   in a planner-recorded sibling copy
//! - deletions never propagate; one hash function everywhere

use crate::adapters::Adapter;
use crate::backup::Backup;
use crate::ctx::Ctx;
use anyhow::{bail, Context as _, Result};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

// ------------------------------------------------------------- pair store

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pair {
    pub name: String,
    /// absolute local project path
    pub local: String,
    /// ssh destination host ([user@]host)
    pub remote_host: String,
    /// absolute remote project path
    pub remote_path: String,
    #[serde(default)]
    pub with_files: bool,
    #[serde(default)]
    pub agents: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PairFile {
    schema: u32,
    #[serde(default)]
    pairs: Vec<Pair>,
}

fn pairs_path(ctx: &Ctx) -> PathBuf {
    ctx.home.join(".movara").join("sync-pairs.json")
}

/// pair identity = endpoint tuple (local, host, remote) — the same
/// endpoints under a second name are REFUSED (two ledgers with
/// independent bases would conflict-copy storm)
pub fn pair_id(p: &Pair) -> String {
    let mut h = sha2::Sha256::new();
    h.update(p.local.as_bytes());
    h.update(b"\0");
    h.update(p.remote_host.as_bytes());
    h.update(b"\0");
    h.update(p.remote_path.as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("pair-{}", &hex[..16])
}

pub fn pair_add(ctx: &Ctx, p: &Pair) -> Result<()> {
    let path = pairs_path(ctx);
    let mut file: PairFile = if path.is_file() {
        serde_json::from_str(&fs::read_to_string(&path)?)?
    } else {
        PairFile {
            schema: 1,
            pairs: Vec::new(),
        }
    };
    if file.pairs.iter().any(|e| e.name == p.name) {
        bail!("{}", t!("sync.err_dup_name", name = p.name.as_str()));
    }
    let id = pair_id(p);
    if file.pairs.iter().any(|e| pair_id(e) == id) {
        bail!(
            "{}",
            t!(
                "sync.err_dup_endpoints",
                name = p.name.as_str(),
                local = p.local.as_str(),
                remote = p.remote_path.as_str()
            )
        );
    }
    file.pairs.push(p.clone());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&file)?)?;
    Ok(())
}

pub fn pair_remove(ctx: &Ctx, name: &str) -> Result<bool> {
    let path = pairs_path(ctx);
    if !path.is_file() {
        return Ok(false);
    }
    let mut file: PairFile = serde_json::from_str(&fs::read_to_string(&path)?)?;
    let before = file.pairs.len();
    file.pairs.retain(|e| e.name != name);
    let removed = file.pairs.len() < before;
    fs::write(&path, serde_json::to_string_pretty(&file)?)?;
    Ok(removed)
}

pub fn pair_list(ctx: &Ctx) -> Result<Vec<Pair>> {
    let path = pairs_path(ctx);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str::<PairFile>(&fs::read_to_string(&path)?)?.pairs)
}

pub fn pair_get(ctx: &Ctx, name: &str) -> Result<Pair> {
    pair_list(ctx)?
        .into_iter()
        .find(|e| e.name == name)
        .with_context(|| t!("sync.err_unknown_pair", name = name).to_string())
}

// ---------------------------------------------------------------- ledger

/// one member's ledger row
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MemberRow {
    /// last-synced base hash (None = absent base: first observation or
    /// planner-recorded sibling)
    pub base: Option<String>,
    /// planner-recorded sibling conflict copy (exempt from the
    /// conflict-copy enumeration glob)
    pub sibling: Option<String>,
    /// local-form alias when a reserved-name escape or case-dedupe
    /// renamed the member locally
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LedgerState {
    pub schema: u32,
    pub pair_id: String,
    #[serde(default)]
    pub members: BTreeMap<String, MemberRow>,
    #[serde(default)]
    pub last_sync: Option<String>,
}

pub struct Ledger {
    pub dir: PathBuf,
    pub state: LedgerState,
}

impl Ledger {
    pub fn load(ctx: &Ctx, id: &str) -> Result<Self> {
        let dir = ctx.home.join(".movara").join("sync").join(id);
        let spath = dir.join("state.json");
        let state = if spath.is_file() {
            serde_json::from_str(&fs::read_to_string(&spath)?)?
        } else {
            LedgerState {
                schema: 1,
                pair_id: id.to_string(),
                ..Default::default()
            }
        };
        Ok(Ledger { dir, state })
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(
            self.dir.join("state.json"),
            serde_json::to_string_pretty(&self.state)?,
        )?;
        Ok(())
    }
}

// ------------------------------------------------------- normalized hash

/// the ONE hash function: strip BOM, CRLF→LF, NFC — feeds inventories,
/// pre-state guards and ledger bases alike (normalized equality is
/// transitive across the three). Files on disk are never rewritten.
pub fn norm_hash(bytes: &[u8]) -> String {
    let no_bom = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut norm = Vec::with_capacity(no_bom.len());
    let mut i = 0;
    while i < no_bom.len() {
        if no_bom[i] == b'\r' && no_bom.get(i + 1) == Some(&b'\n') {
            norm.push(b'\n');
            i += 2;
        } else {
            norm.push(no_bom[i]);
            i += 1;
        }
    }
    let text = String::from_utf8_lossy(&norm);
    let nfc: String = unicode_normalization::UnicodeNormalization::nfc(text.chars()).collect();
    let mut h = sha2::Sha256::new();
    h.update(nfc.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ------------------------------------------------- path-canonical hashing

/// marker substituting every host-path form inside hashed content
const PROJ_MARK: &str = "\u{0}mvproj\u{0}";

/// every stored form of a project path: the raw string, the
/// forward-slash / JSON-escaped / msys variants (Windows paths), and
/// the derived tokens agents hash paths into (dash-encoded buckets,
/// sha256/md5 digests, memory keys). Boundary discipline matches
/// ReplaceSpec::replace: a form is only a whole token, never a
/// mid-fragment of a longer name.
pub fn path_needles(project: &str) -> Vec<String> {
    let mut out = vec![project.to_string()];
    let mut push = |v: String| {
        if v != project && !out.contains(&v) {
            out.push(v);
        }
    };
    push(project.replace('\\', "/"));
    push(project.replace('\\', "\\\\"));
    let fwd = project.replace('\\', "/");
    let b = fwd.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        push(format!(
            "/{}{}",
            (b[0] as char).to_ascii_lowercase(),
            &fwd[2..]
        ));
    }
    for (tok, _) in crate::encodings::derived_tokens(project, PROJ_MARK) {
        push(tok);
    }
    out
}

/// canonical marker substitution: one left-to-right scan, longest
/// needle first, component boundaries on both sides (the same scanner
/// discipline as ReplaceSpec::replace). Non-UTF-8 / binary content is
/// returned untouched — such members are outside S1's file-backed scope.
fn canonicalize(raw: &[u8], needles: &[String]) -> Vec<u8> {
    if raw.contains(&0u8) {
        return raw.to_vec();
    }
    let Ok(text) = String::from_utf8(raw.to_vec()) else {
        return raw.to_vec();
    };
    let mut sorted: Vec<&String> = needles.iter().collect();
    sorted.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let mut matched = None;
        for n in &sorted {
            let end = i + n.len();
            if end <= bytes.len()
                && &bytes[i..end] == n.as_bytes()
                && (i == 0 || !crate::spec::is_name_byte(bytes[i - 1]))
                && !bytes
                    .get(end)
                    .is_some_and(|nb| crate::spec::is_name_byte(*nb))
            {
                matched = Some(n);
                break;
            }
        }
        if let Some(n) = matched {
            out.push_str(PROJ_MARK);
            i += n.len();
        } else {
            let end = (i + crate::spec::utf8_char_len(bytes[i])).min(bytes.len());
            out.push_str(&text[i..end]);
            i = end;
        }
    }
    out.into_bytes()
}

/// the sync hash: canonicalize BOTH hosts' project-path forms to the
/// marker, then run the normalized hash. This is what makes asymmetric
/// pairs (~/abc vs ~/projects/abc) converge: A's file names A's path,
/// B's rebased copy names B's — after canonicalization they hash equal,
/// so logical equality is hash equality instead of a fast-forward
/// ping-pong that never ends.
pub fn sync_hash(raw: &[u8], needles: &[String]) -> String {
    norm_hash(&canonicalize(raw, needles))
}

// ------------------------------------------------------------ inventory

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInv {
    pub hash: String,
    pub mtime: i64,
}

/// canonical member inventory over the pair's project state: files under
/// every adapter's state roots that reference the project path, keyed by
/// the canonical (portable, NFC) member identity `agent/kind/rel`.
/// Membership and hashes are path-canonical: this host's forms are the
/// membership needles, BOTH hosts' forms feed the hash (see sync_hash).
/// SQLite-backed members are excluded here — they are S2's row-merge
/// layer, never a file-level blob swap. Conflict copies and movara
/// artifacts are excluded UNLESS recorded as planner siblings in the
/// ledger (recorded siblings are ordinary union members thereafter).
pub fn inventory(
    ctx: &Ctx,
    list: &[Box<dyn Adapter>],
    project: &str,
    other: &str,
    ledger: &Ledger,
) -> BTreeMap<String, MemberInv> {
    let mut out = BTreeMap::new();
    let mem_needles = path_needles(project);
    let hash_needles = [path_needles(project), path_needles(other)].concat();
    let siblings: std::collections::BTreeSet<&String> = ledger
        .state
        .members
        .values()
        .filter_map(|r| r.sibling.as_ref())
        .collect();
    for adapter in list {
        if !adapter.installed(ctx) {
            continue;
        }
        let roots = adapter.state_paths(ctx);
        let kinds = adapter.root_kinds();
        let default_kinds = vec![crate::ctx::RootKind::Home; roots.len()];
        for (root, kind) in roots
            .iter()
            .zip(kinds.iter().chain(default_kinds.iter()).take(roots.len()))
        {
            if !root.is_dir() {
                continue;
            }
            let base = ctx.root_of(*kind);
            let prefix = format!("{}/{}/", adapter.name(), kind.segment());
            for entry in walkdir::WalkDir::new(root)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_str().unwrap_or("");
                    e.depth() == 0
                        || (!crate::adapters::SKIP_DIRS.contains(&name)
                            && !name.to_ascii_lowercase().contains("shell-snap"))
                })
                .filter_map(|e| e.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                let name = entry.file_name().to_str().unwrap_or("");
                let lname = name.to_ascii_lowercase();
                if lname.ends_with(".db")
                    || lname.ends_with(".db-wal")
                    || lname.ends_with(".db-shm")
                    || lname.ends_with(".sqlite")
                    || lname.ends_with(".sqlite3")
                {
                    continue; // S2 row-merge territory, never a file swap
                }
                // conflict copies / artifacts never enumerate unless the
                // ledger recorded them as planner siblings
                if is_movara_artifact(name) && !siblings.contains(&name.to_string()) {
                    continue;
                }
                let raw = match fs::read(entry.path()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                // membership: the content references the project in any
                // stored form (literal path, escaped path, derived token)
                // — boundary-aware, so a longer unrelated path never
                // counts. Prose-only memory rides its keyed DIRECTORY
                // which the walker reaches through the referencing file
                // in the same tree.
                if !contains_form(&raw, &mem_needles) {
                    continue;
                }
                let rel = crate::ctx::to_portable_rel(&crate::ctx::path_str(
                    entry.path().strip_prefix(&base).unwrap_or(entry.path()),
                ));
                let key = format!("{prefix}{rel}");
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.insert(
                    key,
                    MemberInv {
                        hash: sync_hash(&raw, &hash_needles),
                        mtime,
                    },
                );
            }
        }
    }
    out
}

/// boundary-aware containment: any needle present as a whole token
fn contains_form(raw: &[u8], needles: &[String]) -> bool {
    for n in needles {
        let nb = n.as_bytes();
        let mut from = 0usize;
        while let Some(pos) = memchr::memmem::find(&raw[from..], nb) {
            let i = from + pos;
            let end = i + nb.len();
            let left_ok = i == 0 || !crate::spec::is_name_byte(raw[i - 1]);
            let right_ok = end >= raw.len() || !crate::spec::is_name_byte(raw[end]);
            if left_ok && right_ok {
                return true;
            }
            from = i + 1;
        }
    }
    false
}

fn is_movara_artifact(name: &str) -> bool {
    name.contains(".movara-conflict-")
        || name.contains(".movara-premigration.bak")
        || name.ends_with(".movara-tmp")
}

/// boundary-aware content probe incl. the JSON-escaped form (Windows
/// paths in JSON columns carry doubled backslashes)
fn content_references(raw: &[u8], path: &str) -> bool {
    let needles: Vec<String> = if path.contains('\\') {
        vec![path.to_string(), path.replace('\\', "\\\\")]
    } else {
        vec![path.to_string()]
    };
    needles.iter().any(|n| {
        let nb = n.as_bytes();
        let mut from = 0usize;
        while let Some(pos) = memchr::memmem::find(&raw[from..], nb) {
            let i = from + pos;
            let end = i + nb.len();
            let left_ok = i == 0 || !crate::spec::is_name_byte(raw[i - 1]);
            let right_ok = end >= raw.len() || !crate::spec::is_name_byte(raw[end]);
            if left_ok && right_ok {
                return true;
            }
            from = i + 1;
        }
        false
    })
}

// ----------------------------------------------------------- the planner

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    /// already converged (either side may differ from base) — the
    /// crash-recovery fixpoint row; adopts equality as base on commit
    Noop,
    /// copy A→B: A's content wins, B's local bytes are preserved in a
    /// sibling when B diverges
    FastForwardToB {
        content: String,
    },
    FastForwardToA {
        content: String,
    },
    /// union copy for members new on one side
    CopyToB,
    CopyToA,
    /// both diverged: A-side content is the deterministic winner on
    /// BOTH hosts; each host keeps its loser bytes in a recorded sibling
    ConflictWinnerA {
        content: String,
    },
    /// deleted one side, changed the other: never propagate deletions
    SkipDeleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedMember {
    pub member: String,
    pub action: Action,
    /// expected pre-state hash of A's file (absent members have None)
    pub pre_a: Option<String>,
    pub pre_b: Option<String>,
}

/// pure merge planner: (A inventory, B inventory, ledger) → plan. The
/// single decision point both sides apply — never two planners.
pub fn plan(
    a_inv: &BTreeMap<String, MemberInv>,
    b_inv: &BTreeMap<String, MemberInv>,
    ledger: &LedgerState,
) -> Vec<PlannedMember> {
    let mut keys: std::collections::BTreeSet<&String> = Default::default();
    keys.extend(a_inv.keys());
    keys.extend(b_inv.keys());
    keys.extend(ledger.members.keys());
    let mut out = Vec::new();
    for k in keys {
        let a = a_inv.get(k);
        let b = b_inv.get(k);
        let base = ledger.members.get(k).and_then(|r| r.base.clone());
        let action = match (a, b) {
            (Some(a), Some(b)) if a.hash == b.hash => Action::Noop,
            (Some(a), Some(b)) => match &base {
                Some(h) if *h == b.hash => Action::FastForwardToB {
                    content: a.hash.clone(),
                },
                Some(h) if *h == a.hash => Action::FastForwardToA {
                    content: b.hash.clone(),
                },
                // absent base (first observation) or both diverged: the
                // planner picks the A-side winner symmetrically
                _ => Action::ConflictWinnerA {
                    content: a.hash.clone(),
                },
            },
            (Some(a), None) => match &base {
                // B deleted, A unchanged: do not propagate deletions
                Some(h) if *h == a.hash => Action::SkipDeleted,
                // A-only member: union copy
                _ => Action::CopyToB,
            },
            (None, Some(b)) => match &base {
                Some(h) if *h == b.hash => Action::SkipDeleted,
                _ => Action::CopyToA,
            },
            (None, None) => continue, // ledger-only: both deleted
        };
        out.push(PlannedMember {
            member: k.clone(),
            action,
            pre_a: a.map(|m| m.hash.clone()),
            pre_b: b.map(|m| m.hash.clone()),
        });
    }
    out
}

/// symmetric base-advance on commit: BOTH halves adopt the plan's
/// designated winner; no-op rows adopt equality; conflict losers live on
/// only in recorded siblings. Rows the receiving side failed to apply
/// (`confirmed`) never advance — a skipped member must re-plan from its
/// old base next run, not silently adopt a state it never reached.
pub fn commit_plan(
    ledger: &mut LedgerState,
    plan: &[PlannedMember],
    confirmed: &std::collections::BTreeSet<String>,
    siblings: &BTreeMap<String, String>,
) {
    for pm in plan {
        if !confirmed.contains(&pm.member) {
            continue;
        }
        let row = ledger.members.entry(pm.member.clone()).or_default();
        let winner = match &pm.action {
            Action::FastForwardToB { content }
            | Action::FastForwardToA { content }
            | Action::ConflictWinnerA { content } => Some(content.clone()),
            Action::Noop => pm.pre_a.clone().or_else(|| pm.pre_b.clone()),
            Action::CopyToB => pm.pre_a.clone(),
            Action::CopyToA => pm.pre_b.clone(),
            Action::SkipDeleted => None,
        };
        if let Some(w) = winner {
            row.base = Some(w);
        }
        if matches!(pm.action, Action::ConflictWinnerA { .. }) {
            if let Some(s) = siblings.get(&pm.member) {
                row.sibling = Some(s.clone());
            }
        }
    }
    ledger.last_sync = Some(chrono::Local::now().to_rfc3339());
}

/// every plan row confirmed (tests and trust-all drivers)
pub fn confirmed_all(plan: &[PlannedMember]) -> std::collections::BTreeSet<String> {
    plan.iter().map(|pm| pm.member.clone()).collect()
}

/// which plan rows actually converged: fixpoint rows by definition,
/// transfer rows only when the RECEIVING side applied them (A-side
/// ConflictWinnerA rows converge when B applied — A already holds the
/// winner)
pub fn confirmed_members(
    plan: &[PlannedMember],
    a_applied: &[String],
    b_applied: &[String],
) -> std::collections::BTreeSet<String> {
    let a: std::collections::BTreeSet<&String> = a_applied.iter().collect();
    let b: std::collections::BTreeSet<&String> = b_applied.iter().collect();
    plan.iter()
        .filter(|pm| match &pm.action {
            Action::Noop | Action::SkipDeleted => true,
            Action::FastForwardToA { .. } | Action::CopyToA => a.contains(&pm.member),
            _ => b.contains(&pm.member),
        })
        .map(|pm| pm.member.clone())
        .collect()
}

// ----------------------------------------------------------------- apply

/// raw member bytes from the other host, keyed by member identity
pub type Staging = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ApplyReport {
    pub placed: u64,
    pub replaced: u64,
    pub conflicts_local_kept: u64,
    pub skipped: u64,
    /// members this host received/verified as the transfer's receiver
    pub applied: Vec<String>,
    /// conflict siblings kept or materialized (member → file name)
    pub siblings: BTreeMap<String, String>,
}

/// deterministic sibling name, identical on both hosts for the same
/// (member, run): the loser copy must be the same union member on both
/// ledgers or the next plan diverges.
pub fn sibling_name(member: &str, run_id: &str) -> String {
    let base = member.rsplit('/').next().unwrap_or(member);
    match base.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.movara-conflict-{run_id}.{ext}"),
        None => format!("{base}.movara-conflict-{run_id}"),
    }
}

/// read a member's raw bytes from THIS host's state tree
pub fn read_member(ctx: &Ctx, list: &[Box<dyn Adapter>], member: &str) -> Option<Vec<u8>> {
    let p = member_path(ctx, list, member)?;
    fs::read(p).ok()
}

/// apply one half of a plan against this host: `side_a` selects which
/// endpoint this host is (A initiator or B remote). Transfer rows that
/// land on this host write base-guarded — the destination must still
/// hash (path-canonically) to the plan's pre-state, so a member that
/// changed between inventory and apply is skipped, never written onto
/// stale assumptions. ConflictWinnerA only ever writes on B; B's
/// diverged bytes first move into a recorded sibling copy. Incoming
/// bytes are rebased from the other host's path forms onto this host's
/// (the four stored forms plus derived tokens) before placement.
#[allow(clippy::too_many_arguments)]
pub fn apply_half(
    ctx: &Ctx,
    list: &[Box<dyn Adapter>],
    project: &str,
    other_project: &str,
    plan: &[PlannedMember],
    side_a: bool,
    staging: &Staging,
    ledger: &mut LedgerState,
    backup: &mut Backup,
    run_id: &str,
) -> Result<ApplyReport> {
    let mut report = ApplyReport::default();
    // the rebase rule is direction-symmetric: both sides map the OTHER
    // host's path onto their own
    let spec = if project != other_project {
        Some(crate::spec::ReplaceSpec::new(other_project, project)?)
    } else {
        None
    };
    let hash_needles = [path_needles(project), path_needles(other_project)].concat();
    for pm in plan {
        let receives = match &pm.action {
            Action::Noop | Action::SkipDeleted => continue,
            Action::FastForwardToB { .. } => !side_a,
            Action::FastForwardToA { .. } => side_a,
            Action::CopyToB => !side_a,
            Action::CopyToA => side_a,
            Action::ConflictWinnerA { .. } => !side_a, // A already holds the winner
        };
        if !receives {
            continue;
        }
        let Some(lpath) = member_path(ctx, list, &pm.member) else {
            report.skipped += 1;
            continue;
        };
        // pre-state re-verification against the LIVE file, not the stale
        // inventory: expected-present members must still hash to the
        // plan's pre-state, expected-absent ones must still be absent
        let cur_raw = fs::read(&lpath).ok();
        let cur_hash = cur_raw.as_ref().map(|b| sync_hash(b, &hash_needles));
        let expected = if side_a { &pm.pre_a } else { &pm.pre_b };
        match (expected, &cur_hash) {
            (Some(e), Some(c)) if e == c => {}
            (None, None) => {}
            _ => {
                report.skipped += 1;
                continue;
            }
        }
        let Some(raw) = staging.get(&pm.member) else {
            report.skipped += 1;
            continue;
        };
        // conflict: this host's diverged bytes survive as the sibling
        // (a previously-recorded sibling holding the same bytes is
        // reused instead of stacking copies)
        if matches!(pm.action, Action::ConflictWinnerA { .. }) {
            if let Some(cur) = &cur_raw {
                report.conflicts_local_kept += 1;
                let name = materialize_sibling(
                    &lpath,
                    &pm.member,
                    cur,
                    cur_hash.as_ref(),
                    &hash_needles,
                    ledger,
                    backup,
                    run_id,
                );
                report.siblings.insert(pm.member.clone(), name);
            }
        }
        let bytes = rebase(raw, &lpath, spec.as_ref());
        write_member(&lpath, &bytes, backup, &mut report)?;
        report.applied.push(pm.member.clone());
    }
    Ok(report)
}

/// keep the loser: copy lpath's bytes to the recorded sibling name (or
/// reuse an existing sibling already holding path-canonically identical
/// bytes — a second conflict of the same unchanged loser must not stack
/// copies)
#[allow(clippy::too_many_arguments)]
fn materialize_sibling(
    lpath: &std::path::Path,
    member: &str,
    cur: &[u8],
    cur_hash: Option<&String>,
    hash_needles: &[String],
    ledger: &mut LedgerState,
    backup: &mut Backup,
    run_id: &str,
) -> String {
    let row = ledger.members.entry(member.to_string()).or_default();
    if let Some(prev) = &row.sibling {
        let prev_path = lpath.with_file_name(prev);
        if let Ok(pb) = fs::read(&prev_path) {
            if Some(&sync_hash(&pb, hash_needles)) == cur_hash {
                return prev.clone();
            }
        }
    }
    let name = sibling_name(member, run_id);
    let spath = lpath.with_file_name(&name);
    if let Some(parent) = spath.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // undo restores the loser at lpath (record_file) and removes the
    // sibling (record_created) — a full revert of the sync
    let _ = backup.record_file(lpath);
    let _ = fs::write(&spath, cur);
    backup.record_created(&spath);
    row.sibling = Some(name.clone());
    name
}

/// rebase incoming bytes onto this host's path forms. Dispatch follows
/// the migrate rewriters (jsonl / json), but a member the format core
/// declines falls back to the boundary-aware text core: convergence
/// REQUIRES the path forms to move, whatever the file's parseability.
fn rebase(raw: &[u8], lpath: &std::path::Path, spec: Option<&crate::spec::ReplaceSpec>) -> Vec<u8> {
    let Some(sp) = spec else {
        return raw.to_vec();
    };
    if !sp.maybe_contains(raw) {
        return raw.to_vec();
    }
    let ext = lpath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let moved = |s: Option<String>| s.map(String::into_bytes);
    match ext.as_str() {
        "jsonl" | "ndjson" => crate::rewriters::jsonl_core(raw, sp, false)
            .ok()
            .flatten()
            .and_then(|s| moved(Some(s)))
            .or_else(|| moved(crate::rewriters::text_core(raw, sp)))
            .unwrap_or_else(|| raw.to_vec()),
        "json" => crate::rewriters::json_core(raw, sp)
            .ok()
            .flatten()
            .or_else(|| moved(crate::rewriters::text_core(raw, sp)))
            .unwrap_or_else(|| raw.to_vec()),
        _ => moved(crate::rewriters::text_core(raw, sp)).unwrap_or_else(|| raw.to_vec()),
    }
}

/// place member bytes base-guarded by the caller: journal the previous
/// state (undo reverts the whole sync), write through a .movara-tmp and
/// rename over (the same placement discipline as the rewriters)
fn write_member(
    lpath: &std::path::Path,
    bytes: &[u8],
    backup: &mut Backup,
    report: &mut ApplyReport,
) -> Result<()> {
    if let Some(parent) = lpath.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("{}: {}", t!("sync.err_mkdir"), parent.display()))?;
    }
    let existed = lpath.exists();
    if existed {
        backup.record_file(lpath)?;
        report.replaced += 1;
    } else {
        backup.record_created(lpath);
        report.placed += 1;
    }
    let tmp = lpath.with_file_name(format!(
        "{}.movara-tmp",
        lpath
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("member")
    ));
    fs::write(&tmp, bytes)?;
    if existed {
        // std::fs::rename refuses to overwrite on Windows
        let _ = fs::remove_file(lpath);
    }
    fs::rename(&tmp, lpath)?;
    Ok(())
}

pub fn member_path(ctx: &Ctx, list: &[Box<dyn Adapter>], member: &str) -> Option<PathBuf> {
    // member = agent/kind/rel
    let mut it = member.splitn(3, '/');
    let agent = it.next()?;
    let kind = crate::ctx::RootKind::from_segment(it.next()?)?;
    let rel = it.next()?;
    let _ = list.iter().find(|a| a.name() == agent)?;
    Some(ctx.root_of(kind).join(rel))
}

// ------------------------------------------------------------ live probe

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProbeReport {
    pub os: String,
    pub processes: Vec<String>,
    pub fresh_wal: Vec<String>,
    pub fresh_members: Vec<String>,
    pub hot: bool,
}

/// liveness probe: known agent processes alive, fresh -wal sidecars, and
/// in-scope members with mtime newer than the freshness window. Hot on
/// this host refuses the whole run (the same gate runs on both).
pub fn probe(
    ctx: &Ctx,
    list: &[Box<dyn Adapter>],
    project: &str,
    freshness_secs: u64,
) -> ProbeReport {
    let mut rep = ProbeReport {
        os: std::env::consts::OS.to_string(),
        ..Default::default()
    };
    // process gate (CLI agents; GUI IDEs are mtime/WAL-gated only)
    for adapter in list {
        for name in adapter.process_names() {
            let alive = std::process::Command::new("pgrep")
                .arg("-x")
                .arg(name)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if alive {
                rep.processes.push(name.to_string());
            }
        }
    }
    // state-root freshness
    let spec = crate::spec::ReplaceSpec::new(project, project).ok();
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(freshness_secs))
        .unwrap_or(0);
    for adapter in list {
        if !adapter.installed(ctx) {
            continue;
        }
        for root in adapter.state_paths(ctx) {
            for entry in walkdir::WalkDir::new(&root)
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let name = entry.file_name().to_str().unwrap_or("");
                if name.ends_with("-shm") {
                    continue;
                }
                // a live -wal sidecar means the owning agent may still be
                // running — the strongest single freshness signal
                if name.ends_with("-wal") {
                    rep.fresh_wal.push(entry.path().display().to_string());
                    continue;
                }
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if mtime >= cutoff && mtime > 0 {
                    let fresh = entry.path().display().to_string();
                    // only files inside our scope (referencing content is
                    // checked loosely: any recently-touched state file in
                    // a referencing adapter counts)
                    if let Some(sp) = &spec {
                        if let Ok(raw) = fs::read(entry.path()) {
                            if content_references(&raw, project) || sp.maybe_contains(&raw) {
                                rep.fresh_members.push(fresh);
                            }
                        }
                    }
                }
            }
        }
    }
    rep.hot =
        !rep.processes.is_empty() || !rep.fresh_wal.is_empty() || !rep.fresh_members.is_empty();
    rep
}

// ---------------------------------------------------------------- lease

pub struct Lease {
    pub path: PathBuf,
}

impl Lease {
    pub fn acquire(ctx: &Ctx, pair_id: &str, ttl_secs: u64) -> Result<Option<Self>> {
        let dir = ctx.home.join(".movara").join("sync").join(pair_id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("lock");
        if path.exists() {
            let txt = fs::read_to_string(&path).unwrap_or_default();
            if let Ok(exp) = txt.trim().parse::<i64>() {
                if exp > chrono::Local::now().timestamp() {
                    return Ok(None); // held
                }
            }
        }
        let exp = chrono::Local::now().timestamp() + ttl_secs as i64;
        fs::write(&path, exp.to_string())?;
        Ok(Some(Self { path }))
    }

    pub fn renew(&self, ttl_secs: u64) -> Result<()> {
        let exp = chrono::Local::now().timestamp() + ttl_secs as i64;
        fs::write(&self.path, exp.to_string())?;
        Ok(())
    }

    pub fn release(&self) {
        let _ = fs::remove_file(&self.path);
    }
}
