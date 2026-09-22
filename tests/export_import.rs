// SPDX-License-Identifier: MIT OR Apache-2.0
//! Export / import: round-trip, rebase equivalence with migrate, the
//! exclusion/projection layer, conflict policies and undo of an import.

mod common;

use common::Fixture;
use movara::archive::{self, ExportOpts, ImportOpts, Policy};
use movara::backup::{self, Backup};
use movara::spec::{self, ReplaceSpec};
use std::fs;
use std::path::{Path, PathBuf};

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap()
}

/// a fresh target HOME with the project directory present at `proj`
fn target_home(tag: &str, proj_rel: &str) -> (movara::ctx::Ctx, PathBuf) {
    let tmp = std::env::temp_dir().join(format!("movara-imp-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let home = fs::canonicalize(&tmp).unwrap_or_else(|_| tmp.clone());
    let ctx = movara::ctx::Ctx {
        home: home.clone(),
        config_home: home.join(".config"),
        data_home: home.join(".local").join("share"),
    };
    let proj = ctx
        .home
        .join(proj_rel.split('/').next().unwrap_or(proj_rel))
        .join(proj_rel.split_once('/').map(|(_, r)| r).unwrap_or(""));
    fs::create_dir_all(&proj).unwrap();
    (ctx, tmp)
}

fn adapters() -> Vec<Box<dyn movara::adapters::Adapter>> {
    movara::adapters::get_adapters(None).unwrap()
}

fn do_export(fx: &Fixture, tag: &str) -> PathBuf {
    let out = fx.tmp.join(format!("{}.tar.gz", tag));
    archive::run_export(
        &fx.ctx,
        &adapters(),
        &ExportOpts {
            out: out.clone(),
            filtered: false,
            paths: vec![],
            project: None,
            state_only: false,
        },
    )
    .unwrap();
    assert!(out.is_file(), "archive not written");
    out
}

#[test]
fn export_excludes_secrets_and_projects_configs() {
    let fx = Fixture::new("exp-sec");
    let out = do_export(&fx, "sec");
    let staging = archive::open(&out).unwrap();
    let mut saw_credentials = false;
    let mut saw_snapshot = false;
    let mut saw_aider_conf = false;
    let mut claude_json = String::new();
    for entry in walkdir::WalkDir::new(staging.dir.join("data"))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        let rel = p
            .strip_prefix(staging.dir.join("data"))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if rel.contains("credentials") {
            saw_credentials = true;
        }
        if rel.contains("shell-snap") {
            saw_snapshot = true;
        }
        if rel.ends_with(".aider.conf.yml") {
            saw_aider_conf = true;
        }
        if rel.ends_with(".claude.json") {
            claude_json = read(p);
        }
    }
    assert!(!saw_credentials, "credentials file leaked into archive");
    assert!(!saw_snapshot, "shell snapshot leaked into archive");
    assert!(!saw_aider_conf, "aider config leaked into archive");
    // projection: projects map travels, secrets and settings do not
    // (parsed comparison: Windows keys are backslash-escaped on disk)
    let proj_obj: serde_json::Value = serde_json::from_str(&claude_json).unwrap();
    assert!(
        proj_obj["projects"].get(&fx.old).is_some(),
        "projects map missing"
    );
    assert!(
        !claude_json.contains("marker-mcp-value"),
        "mcpServers leaked"
    );
    assert!(!claude_json.contains("numStartups"), "settings leaked");
    // no secret marker anywhere in the archive
    for entry in walkdir::WalkDir::new(&staging.dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let raw = fs::read(entry.path()).unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(
            !text.contains("marker-mcp-value")
                && !text.contains("sekret-creds")
                && !text.contains("sekret-snap"),
            "secret leaked via {}",
            entry.path().display()
        );
    }
    let _ = fs::remove_dir_all(
        fx.tmp
            .parent()
            .unwrap()
            .join(format!("movara-imp-exp-sec-{}", std::process::id())),
    );
}

#[test]
fn import_rebase_matches_migrate() {
    let fx = Fixture::new("exp-rt");
    let out = do_export(&fx, "rt");

    // target: same project basename under a different home + dir
    let (ctx_b, tmp_b) = target_home("exp-rt", "proj/cba");
    let rules = vec![(
        fx.old.clone(),
        ctx_b
            .home
            .join("proj")
            .join("cba")
            .to_string_lossy()
            .into_owned(),
    )];

    let staging = archive::open(&out).unwrap();
    // verification: every manifest path maps onto the target project
    let missing = archive::verify_paths(&staging.manifest.paths, &rules);
    // "/other" is a stale registry key on the source host: reported as
    // missing, covered by --allow-missing-path
    assert_eq!(missing, vec!["/other".to_string()], "unexpected missing");

    let spec = ReplaceSpec::new(&rules[0].0, &rules[0].1).unwrap();
    let mut bk = Backup::new(
        &tmp_b.join("backups"),
        &spec,
        staging.manifest.agents.clone(),
        false,
    );
    let report = archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: rules.clone(),
            agents: None,
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    bk.save().unwrap();
    assert!(report.placed > 0, "nothing placed");

    // same assertions a migrate would satisfy, on the target host
    let enc_new =
        movara::encodings::dash_encode(&ctx_b.home.join("proj").join("cba").to_string_lossy());
    assert!(ctx_b.h(".claude/projects").join(&enc_new).is_dir());
    let cj: serde_json::Value = serde_json::from_str(&read(&ctx_b.h(".claude.json"))).unwrap();
    let new_key = ctx_b
        .home
        .join("proj")
        .join("cba")
        .to_string_lossy()
        .into_owned();
    assert!(cj["projects"].get(&new_key).is_some());
    assert!(cj["projects"].get(&fx.old).is_none());
    // target settings survive the projection merge (merged into {} here)
    // opencode db row rebased
    let con = rusqlite::Connection::open(ctx_b.d("opencode/opencode.db")).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, new_key);

    // undo returns the target home to pre-import state
    let ids: Vec<_> = fs::read_dir(tmp_b.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    assert_eq!(ids.len(), 1);
    backup::undo(
        &tmp_b.join("backups"),
        ids[0].file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    assert!(!ctx_b.h(".claude").exists(), "created state survived undo");
    assert!(!ctx_b.h(".claude.json").exists());
    assert!(!ctx_b.h(".codex").exists());
    assert!(
        ctx_b.home.join("proj").join("cba").is_dir(),
        "project dir harmed"
    );
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn conflict_policies_skip_and_replace() {
    let fx = Fixture::new("exp-cf");
    let out = do_export(&fx, "cf");
    let (ctx_b, tmp_b) = target_home("exp-cf", "proj/cba");
    // pre-existing local file with the SAME name the archive carries
    let enc_old = movara::encodings::dash_encode(&fx.old);
    let local = ctx_b
        .h(".claude/projects")
        .join(&enc_old)
        .join("sess1.jsonl");
    fs::create_dir_all(local.parent().unwrap()).unwrap();
    fs::write(&local, "LOCAL-WINS\n").unwrap();

    let staging = archive::open(&out).unwrap();
    let spec = ReplaceSpec::identity();
    let mut bk = Backup::new(&tmp_b.join("backups-skip"), &spec, vec![], false);
    archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: vec![],
            agents: Some(vec!["claude".into()]),
            policy: Policy::Skip,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert_eq!(read(&local), "LOCAL-WINS\n", "skip policy clobbered local");

    let staging = archive::open(&out).unwrap();
    let mut bk = Backup::new(&tmp_b.join("backups-rep"), &spec, vec![], false);
    archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: vec![],
            agents: Some(vec!["claude".into()]),
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert_ne!(read(&local), "LOCAL-WINS\n", "replace did not overwrite");
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn rule_validation() {
    let mk = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
    // identity
    assert!(spec::prepare_rules(&mk(&["/a/x:/a/x"])).is_err());
    // root source
    assert!(spec::prepare_rules(&mk(&["/:/a"])).is_err());
    // duplicate source
    assert!(spec::prepare_rules(&mk(&["/a/x:/b", "/a/x:/c"])).is_err());
    // chained: a rule source under another rule's target
    assert!(spec::prepare_rules(&mk(&["/a/x:/b/y", "/b/y/z:/c"])).is_err());
    // the reverse direction: a rule target landing under another source
    assert!(spec::prepare_rules(&mk(&["/a:/b/x", "/b:/c"])).is_err());
    // no separator
    assert!(spec::prepare_rules(&mk(&["/a/x"])).is_err());
    // Windows drive-pair: the NEW path's drive colon must not become
    // the split point (last-colon would yield old='C:\old:C')
    let w = spec::prepare_rules(&mk(&[r"C:\old:C:\new"])).unwrap();
    assert_eq!(w[0].0, r"C:\old");
    assert_eq!(w[0].1, r"C:\new");
    // ordering: longest source first (windows-style last-colon split too)
    let r = spec::prepare_rules(&mk(&["/a:/z", "/a/proj:/z/proj"])).unwrap();
    assert_eq!(r[0].0, "/a/proj");
    assert_eq!(r.len(), 2);
}

#[test]
fn rebase_path_longest_rule_wins() {
    let rules = vec![
        ("/a/proj".to_string(), "/z/proj".to_string()),
        ("/a".to_string(), "/z".to_string()),
    ];
    assert_eq!(archive::rebase_path("/a/proj/sub", &rules), "/z/proj/sub");
    assert_eq!(archive::rebase_path("/a/other", &rules), "/z/other");
    assert_eq!(archive::rebase_path("/q", &rules), "/q");
}

#[test]
fn manifest_shape() {
    let fx = Fixture::new("exp-mf");
    let out = do_export(&fx, "mf");
    let staging = archive::open(&out).unwrap();
    let m = &staging.manifest;
    assert_eq!(m.format, movara::archive::FORMAT);
    assert_eq!(m.os, std::env::consts::OS);
    assert!(!m.agents.is_empty());
    assert!(m.paths.contains(&fx.old), "project path not enumerated");
    assert!(m.stats.files > 0);
    assert!(m.stats.databases > 0);
    assert_eq!(m.movara_version, movara::VERSION);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn crafted_archive_cannot_escape_state_roots() {
    let fx = Fixture::new("exp-evil");
    let out = do_export(&fx, "evil");
    // extract, inject a hostile member, repack with the real writer
    let evil = fx.tmp.join("evil.tar.gz");
    {
        let staging = archive::open(&out).unwrap();
        fs::write(staging.dir.join("data/claude/.bashrc"), b"export PWNED=1\n").unwrap();
        let mut w = archive::ArchiveWriter::create(&evil).unwrap();
        for e in walkdir::WalkDir::new(&staging.dir)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let rel = e
                .path()
                .strip_prefix(&staging.dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if rel.is_empty() {
                continue;
            }
            if e.file_type().is_dir() {
                w.add_dir(&rel).unwrap();
            } else if rel != "manifest.json" {
                w.add_file(&rel, &fs::read(e.path()).unwrap()).unwrap();
            }
        }
        w.finish(&staging.manifest).unwrap();
    }

    let (ctx_b, tmp_b) = target_home("exp-evil", "proj/cba");
    let staging = archive::open(&evil).unwrap();
    let spec = ReplaceSpec::identity();
    let mut bk = Backup::new(&tmp_b.join("bk"), &spec, vec![], false);
    let report = archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: vec![],
            agents: None,
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert!(report.skipped >= 1, "hostile member was not refused");
    assert!(
        !ctx_b.home.join(".bashrc").exists(),
        "archive wrote outside agent state roots"
    );
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn directory_in_the_way_is_skipped() {
    let fx = Fixture::new("exp-dir");
    let out = do_export(&fx, "dir");
    let (ctx_b, tmp_b) = target_home("exp-dir", "proj/cba");
    // a directory where the archive carries a file
    let enc_old = movara::encodings::dash_encode(&fx.old);
    fs::create_dir_all(
        ctx_b
            .h(".claude/projects")
            .join(&enc_old)
            .join("sess1.jsonl"),
    )
    .unwrap();
    let staging = archive::open(&out).unwrap();
    let spec = ReplaceSpec::identity();
    let mut bk = Backup::new(&tmp_b.join("bk"), &spec, vec![], false);
    archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: vec![],
            agents: Some(vec!["claude".into()]),
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert!(
        ctx_b
            .h(".claude/projects")
            .join(&enc_old)
            .join("sess1.jsonl")
            .is_dir(),
        "dir conflict must be left untouched"
    );
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn export_is_deterministic() {
    let fx = Fixture::new("exp-det");
    let a = do_export(&fx, "det-a");
    let b = do_export(&fx, "det-b");
    let inspect = |p: &Path| {
        let staging = archive::open(p).unwrap();
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for e in walkdir::WalkDir::new(staging.dir.join("data"))
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = e
                .path()
                .strip_prefix(&staging.dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            files.push((rel, fs::read(e.path()).unwrap()));
        }
        files.sort();
        (
            files,
            staging.manifest.stats.clone(),
            staging.manifest.agents.clone(),
        )
    };
    let (fa, sa, aa) = inspect(&a);
    let (fb, sb, ab) = inspect(&b);
    assert_eq!(fa, fb, "archive layout/content is not deterministic");
    assert_eq!(sa, sb);
    assert_eq!(aa, ab);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn filtered_import_never_replaces_shared_databases() {
    let fx = Fixture::new("exp-db");
    // export only opencode (its whole-state archive carries the shared db)
    let out = fx.tmp.join("db.tar.gz");
    archive::run_export(
        &fx.ctx,
        &adapters(),
        &ExportOpts {
            out: out.clone(),
            filtered: true,
            paths: vec![],
            project: None,
            state_only: false,
        },
    )
    .unwrap();
    // target already owns an opencode database with OTHER projects' rows
    let (ctx_b, tmp_b) = target_home("exp-db", "proj/cba");
    let db_b = ctx_b.d("opencode/opencode.db");
    fs::create_dir_all(db_b.parent().unwrap()).unwrap();
    {
        let con = rusqlite::Connection::open(&db_b).unwrap();
        con.execute_batch(
            "CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT);\n
             INSERT INTO project VALUES ('local', '/local/keep');",
        )
        .unwrap();
    }
    let staging = archive::open(&out).unwrap();
    let spec = ReplaceSpec::identity();
    let mut bk = Backup::new(&tmp_b.join("bk"), &spec, vec![], false);
    let report = archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules: vec![],
            agents: None,
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    assert!(report.skipped >= 1, "shared db was not refused");
    let wt: String = {
        let con = rusqlite::Connection::open(&db_b).unwrap();
        con.query_row("SELECT worktree FROM project WHERE id='local'", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    assert_eq!(wt, "/local/keep", "local rows were destroyed");
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn path_filtered_export_selects_and_rebases() {
    let fx = Fixture::new("exp-path");
    let out = fx.tmp.join("path.tar.gz");
    archive::run_export(
        &fx.ctx,
        &adapters(),
        &ExportOpts {
            out: out.clone(),
            filtered: true,
            paths: vec![fx.old.clone()],
            project: None,
            state_only: false,
        },
    )
    .unwrap();
    let staging = archive::open(&out).unwrap();
    // the sibling project (old+"2") must not be aboard — a plain
    // substring matcher would have selected it as a prefix hit
    let sibling = format!("{}2", fx.old);
    for e in walkdir::WalkDir::new(staging.dir.join("data"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let raw = fs::read(e.path()).unwrap();
        // boundary-aware: a plain substring check false-positives on
        // SQLite cell concatenations (path cell + "2026-..." timestamp)
        assert!(
            !common::boundary_contains(&raw, &sibling),
            "sibling project leaked: {}",
            e.path().display()
        );
    }
    let enc_sibling = movara::encodings::dash_encode(&sibling);
    assert!(!staging
        .dir
        .join(format!("data/claude/home/.claude/projects/{}", enc_sibling))
        .exists());
    let enc_old = movara::encodings::dash_encode(&fx.old);
    assert!(staging
        .dir
        .join(format!("data/claude/home/.claude/projects/{}", enc_old))
        .join("sess1.jsonl")
        .exists());
    assert!(staging.manifest.filtered);
    assert_eq!(staging.manifest.paths, vec![fx.old.clone()]);
    // path-filtered projection keeps ONLY the selected project's key
    let proj: serde_json::Value =
        serde_json::from_str(&read(&staging.dir.join("data/claude/home/.claude.json"))).unwrap();
    assert!(proj["projects"].get(&fx.old).is_some());
    assert!(
        proj["projects"].get("/other").is_none(),
        "unselected registry key leaked into the projection"
    );
    // shared db included when it references the path
    assert!(staging
        .dir
        .join("data/opencode/data/opencode/opencode.db")
        .exists());
    // hash-named qwen tmp bucket selected by its directory name
    assert!(staging
        .dir
        .join(format!(
            "data/qwen/home/.qwen/tmp/{}",
            movara::encodings::sha256_hex(&fx.old)
        ))
        .exists());

    // filtered import with rebase ≡ migrate on the target host
    let (ctx_b, tmp_b) = target_home("exp-path", "proj/cba");
    let rules = vec![(
        fx.old.clone(),
        ctx_b
            .home
            .join("proj")
            .join("cba")
            .to_string_lossy()
            .into_owned(),
    )];
    let staging = archive::open(&out).unwrap();
    let spec = ReplaceSpec::new(&rules[0].0, &rules[0].1).unwrap();
    let mut bk = Backup::new(
        &tmp_b.join("backups"),
        &spec,
        staging.manifest.agents.clone(),
        false,
    );
    archive::run_import(
        &ctx_b,
        &staging,
        &adapters(),
        &ImportOpts {
            rules,
            agents: None,
            policy: Policy::Replace,
            allow_missing: true,
            dry_run: false,
        },
        &mut bk,
    )
    .unwrap();
    bk.save().unwrap();
    let new_key = ctx_b
        .home
        .join("proj")
        .join("cba")
        .to_string_lossy()
        .into_owned();
    let enc_new = movara::encodings::dash_encode(&new_key);
    assert!(
        ctx_b.h(".claude/projects").join(&enc_new).is_dir(),
        "bucket not rebased"
    );
    let con = rusqlite::Connection::open(ctx_b.d("opencode/opencode.db")).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, new_key, "db rows not rebased");
    let cj: serde_json::Value = serde_json::from_str(&read(&ctx_b.h(".claude.json"))).unwrap();
    assert!(cj["projects"].get(&new_key).is_some());
    // hash bucket selected only by its directory name is re-keyed too
    assert!(
        ctx_b
            .h(".qwen/tmp")
            .join(movara::encodings::sha256_hex(&new_key))
            .is_dir(),
        "hash bucket not renamed on import"
    );
    let _ = fs::remove_dir_all(&tmp_b);
    let _ = fs::remove_dir_all(&fx.tmp);
}

#[test]
fn deep_basename_directory_does_not_leak() {
    let fx = Fixture::new("exp-bn");
    // a generic directory named like the project basename sits at depth 3
    // (below the shallow allowance) with unrelated content — it must not
    // travel in a path-filtered export
    let bn = movara::encodings::basename(&fx.old);
    let junk_dir = fx.ctx.h(".claude/projects/some-deep-dir").join(&bn);
    fs::create_dir_all(&junk_dir).unwrap();
    fs::write(junk_dir.join("notes.txt"), "unrelated content\n").unwrap();
    let out = fx.tmp.join("bn.tar.gz");
    archive::run_export(
        &fx.ctx,
        &adapters(),
        &ExportOpts {
            out: out.clone(),
            filtered: true,
            paths: vec![fx.old.clone()],
            project: None,
            state_only: false,
        },
    )
    .unwrap();
    let staging = archive::open(&out).unwrap();
    assert!(
        !staging
            .dir
            .join(format!(
                "data/claude/home/.claude/projects/some-deep-dir/{}/notes.txt",
                bn
            ))
            .exists(),
        "deep basename-named directory leaked"
    );
    let _ = fs::remove_dir_all(&fx.tmp);
}
