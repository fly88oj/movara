// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cross-host move core, exercised in-process over the same seam
//! production uses: `run_export_into` builds the streamed archive,
//! `receive_core` consumes it (extract-all-first → project placement →
//! auto-rule state import → structured report), and `cleanup_source`
//! implements the journaled, carve-out-protected source cleanup.
//! Binary-level streaming is smoke-tested separately against the real
//! executables.

mod common;

use common::Fixture;
use movara::archive::{self, ExportOpts};
use movara::ctx::Ctx;
use std::fs;
use std::path::{Path, PathBuf};

fn target_ctx(tag: &str) -> (Ctx, PathBuf) {
    let raw = std::env::temp_dir().join(format!("movara-mv2-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&raw);
    fs::create_dir_all(&raw).unwrap();
    let tmp = movara::ctx::de_verbatim(&fs::canonicalize(&raw).unwrap_or(raw));
    let ctx = Ctx {
        home: tmp.clone(),
        config_home: tmp.join(".config"),
        data_home: tmp.join(".local").join("share"),
        data_local: Some(tmp.join(".local").join("share")),
    };
    (ctx, tmp)
}

fn adapters() -> Vec<Box<dyn movara::adapters::Adapter>> {
    movara::adapters::get_adapters(None).unwrap()
}

/// stream-equivalent: build the move archive to a file (same content
/// the ssh transport would stream) and hand it to receive_core
fn do_move(
    fx: &Fixture,
    ctx_b: &Ctx,
    dst: &Path,
    state_only: bool,
) -> movara::archive::ExportReport {
    let arch = fx.tmp.join(format!(
        "move-{}.tar.gz",
        dst.file_name().unwrap().to_str().unwrap()
    ));
    let writer = archive::ArchiveWriter::create(&arch).unwrap();
    let opts = ExportOpts {
        out: arch.clone(),
        filtered: true,
        paths: vec![fx.old.clone()],
        project: Some(PathBuf::from(&fx.old)),
        state_only,
    };
    let report = archive::run_export_into(&fx.ctx, &adapters(), &opts, writer, None).unwrap();
    let doc = movara::cli::receive_core(ctx_b, dst, fs::File::open(&arch).unwrap()).unwrap();
    assert!(doc["undo_id"].as_str().is_some());
    report
}

/// a realistic source project tree: code, git, caches, memory files
fn seed_project(fx: &Fixture) {
    for (rel, body) in [
        ("main.rs", "fn main() {}\n"),
        (
            "CLAUDE.md",
            "# project memory: we use pnpm, never edit gen/\n",
        ),
        (".env", "API_KEY=sekret-env\n"),
        (".git/config", "[core]\n"),
        ("target/junk.txt", "build output\n"),
        (".cursor/rules/r.md", "always answer briefly\n"),
    ] {
        let p = Path::new(&fx.old).join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("DBG read failed {:?}: {e}", p))
}

#[test]
fn move_places_project_and_rebases_state() {
    let fx = Fixture::new("xh-e2e");
    seed_project(&fx);
    let (ctx_b, tmp_b) = target_ctx("xh-e2e");
    let dst = ctx_b.home.join("proj").join("cba");
    let report = do_move(&fx, &ctx_b, &dst, false);
    // project tree: code + git + memory carried, caches not
    assert!(dst.join("main.rs").is_file());
    assert!(dst.join(".git/config").is_file(), ".git must ride the move");
    assert!(dst.join("CLAUDE.md").is_file());
    assert!(dst.join(".cursor/rules/r.md").is_file());
    assert!(
        dst.join(".env").is_file(),
        "secrets carried (warned, not dropped)"
    );
    assert!(!dst.join("target").exists(), "target/ must not ride");
    assert!(report.secrets.iter().any(|s| s.ends_with(".env")));
    // state rebased like a migrate
    let new_key = dst.to_string_lossy().into_owned();
    let enc_new = movara::encodings::dash_encode(&new_key);
    assert!(ctx_b.h(".claude/projects").join(&enc_new).is_dir());
    let cj: serde_json::Value = serde_json::from_str(&read(&ctx_b.h(".claude.json"))).unwrap();
    assert!(cj["projects"].get(&new_key).is_some());
    let con = rusqlite::Connection::open(ctx_b.d("opencode/opencode.db")).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, new_key, "db rows not rebased");
    // memory re-keyed by directory (the prose has no path inside)
    assert!(
        ctx_b
            .h(".pi/agent/projects-memory/cba")
            .join("AGENTS.md")
            .is_file(),
        "pi memory not re-keyed"
    );
    let zkey = movara::encodings::zcode_memory_key(&new_key);
    assert!(
        ctx_b
            .h(".zcode/cli/memories/projects")
            .join(zkey)
            .join("MEMORY.md")
            .is_file(),
        "zcode memory not re-keyed"
    );
    // member list is the cleanup contract and covers state + project
    assert!(report.members.iter().any(|m| m.starts_with("data/claude/")));
    assert!(report.members.iter().any(|m| m.starts_with("project/")));
    // source untouched (no cleanup ran)
    assert!(Path::new(&fx.old).join("main.rs").is_file());
    assert!(fx.ctx.h(".claude.json").is_file());
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn cleanup_removes_state_keeps_shared_and_restores_via_undo() {
    let fx = Fixture::new("xh-cl");
    seed_project(&fx);
    let (ctx_b, tmp_b) = target_ctx("xh-cl");
    let dst = ctx_b.home.join("proj").join("cba");
    let report = do_move(&fx, &ctx_b, &dst, false);
    // cleanup, exactly as perform_move drives it
    let mut bk = movara::backup::Backup::new(
        &fx.ctx.default_backup_dir(),
        &movara::spec::ReplaceSpec::identity(),
        report.agents.clone(),
        false,
    );
    let rep = archive::cleanup_source(&fx.ctx, &report.members, &mut bk).unwrap();
    bk.save().unwrap();
    // non-shared state removed on the source (session file, memory)
    let enc_old = movara::encodings::dash_encode(&fx.old);
    assert!(
        !fx.ctx
            .h(".claude/projects")
            .join(&enc_old)
            .join("sess1.jsonl")
            .exists(),
        "session state not cleaned"
    );
    assert!(
        !fx.ctx.h(".pi/agent/projects-memory/abc").exists(),
        "memory not cleaned"
    );
    // shared members survive: db + projection carrier
    assert!(
        fx.ctx.d("opencode/opencode.db").is_file(),
        "shared db must stay"
    );
    assert!(
        fx.ctx.h(".claude.json").is_file(),
        "projection carrier must stay"
    );
    assert!(rep.kept_shared.iter().any(|k| k.contains("opencode.db")));
    // the project itself stays (user code is never deleted)
    assert!(Path::new(&fx.old).join("main.rs").is_file());
    // undo restores the removed state
    movara::backup::undo(&fx.ctx.default_backup_dir(), &bk.manifest.id).unwrap();
    assert!(
        fx.ctx
            .h(".claude/projects")
            .join(&enc_old)
            .join("sess1.jsonl")
            .is_file(),
        "undo did not restore the cleaned session"
    );
    assert!(fx
        .ctx
        .h(".pi/agent/projects-memory/abc/AGENTS.md")
        .is_file());
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn state_only_move_carries_memory_not_code() {
    let fx = Fixture::new("xh-so");
    seed_project(&fx);
    let (ctx_b, tmp_b) = target_ctx("xh-so");
    let dst = ctx_b.home.join("proj").join("cba");
    let report = do_move(&fx, &ctx_b, &dst, true);
    assert!(
        dst.join("CLAUDE.md").is_file(),
        "project memory manifest missing"
    );
    assert!(dst.join(".cursor/rules/r.md").is_file());
    assert!(
        !dst.join("main.rs").exists(),
        "code must not ride a state-only move"
    );
    assert!(!dst.join(".git").exists());
    assert!(!report.members.iter().any(|m| m.starts_with("project/")));
    assert!(report
        .members
        .iter()
        .any(|m| m.starts_with("project-memory/")));
    // state still lands
    let new_key = dst.to_string_lossy().into_owned();
    let enc_new = movara::encodings::dash_encode(&new_key);
    assert!(ctx_b.h(".claude/projects").join(&enc_new).is_dir());
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn same_path_move_is_verbatim() {
    // a crafted archive whose project source_path equals the receive
    // destination: the defined zero-pair verbatim import
    let (ctx_b, tmp_b) = target_ctx("xh-sp");
    let dst = ctx_b.home.join("proj/abc");
    let dst_s = dst.to_string_lossy().into_owned();
    let arch = tmp_b.join("same.tar.gz");
    {
        use movara::archive::ArchiveManifest;
        let mut w = archive::ArchiveWriter::create(&arch).unwrap();
        let enc = movara::encodings::dash_encode(&dst_s);
        w.add_dir(&format!("data/claude/home/.claude/projects/{}", enc))
            .unwrap();
        w.add_file(
            &format!("data/claude/home/.claude/projects/{}/s.jsonl", enc),
            format!("{{\"cwd\":\"{}\"}}\n", dst_s).as_bytes(),
        )
        .unwrap();
        let manifest = ArchiveManifest {
            format: movara::archive::FORMAT,
            created: String::new(),
            host: "t".into(),
            os: "linux".into(),
            source_home: "/home/user".into(),
            movara_version: movara::VERSION.into(),
            agents: vec!["claude".into()],
            filtered: true,
            project: Some(movara::archive::ProjectCarriage {
                source_path: dst_s.clone(),
                included: false,
            }),
            // no registry paths: this test pins the zero-rule import,
            // not path verification
            paths: vec![],
            stats: Default::default(),
        };
        w.finish(&manifest).unwrap();
    }
    let doc = movara::cli::receive_core(&ctx_b, &dst, fs::File::open(&arch).unwrap()).unwrap();
    assert!(
        doc["rules"].as_array().unwrap().is_empty(),
        "same-path must be zero-rule"
    );
    let enc = movara::encodings::dash_encode(&dst_s);
    let placed = ctx_b.h(".claude/projects").join(&enc).join("s.jsonl");
    assert!(placed.is_file(), "verbatim placement missing");
    assert!(read(&placed).contains(&dst_s));
    let _ = fs::remove_dir_all(&tmp_b);
}

#[test]
fn truncated_stream_places_nothing() {
    let (ctx_b, tmp_b) = target_ctx("xh-tr");
    let dst = ctx_b.home.join("proj").join("cba");
    let garbage = std::io::Cursor::new(b"not-a-gzip-stream".to_vec());
    let res = movara::cli::receive_core(&ctx_b, &dst, garbage);
    assert!(res.is_err(), "garbage stream must fail");
    assert!(!dst.exists(), "nothing may be placed from a broken stream");
    assert!(!ctx_b.h(".claude").exists());
    let _ = fs::remove_dir_all(&tmp_b);
}

#[test]
fn occupied_destination_is_refused() {
    let (ctx_b, tmp_b) = target_ctx("xh-oc");
    let dst = ctx_b.home.join("proj").join("cba");
    fs::create_dir_all(&dst).unwrap();
    fs::write(dst.join("occupant"), "x").unwrap();
    assert!(
        !archive::dst_available(&dst),
        "occupied dst must fail preflight"
    );
    // and the plan-only predicate is what receive's preflight consults
    let _ = fs::remove_dir_all(&tmp_b);
}

#[test]
fn parent_dir_escape_member_is_refused() {
    // a crafted archive with a `..` member: tar unpack refuses it, so
    // receive fails before placing anything
    let (ctx_b, tmp_b) = target_ctx("xh-esc");
    let dst = ctx_b.home.join("proj").join("cba");
    let arch = tmp_b.join("evil.tar.gz");
    {
        let f = fs::File::create(&arch).unwrap();
        let gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        let mut b = tar::Builder::new(gz);
        // tar builders refuse `..` member paths outright (verified by the
        // build error itself), so the placement-side guard is exercised
        // with a symlink member pointing out of the destination
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Symlink);
        h.as_gnu_mut().unwrap().linkname[..9].clone_from_slice(b"../escape");
        h.set_cksum();
        b.append_data(&mut h, "project/evil-link", std::io::empty())
            .unwrap();
        b.into_inner().unwrap().finish().unwrap();
    }
    let res = movara::cli::receive_core(&ctx_b, &dst, fs::File::open(&arch).unwrap());
    assert!(res.is_err(), "symlink member must be refused");
    assert!(
        !dst.join("evil-link").exists(),
        "symlink member placed despite the guard"
    );
    let _ = fs::remove_dir_all(&tmp_b);
}
