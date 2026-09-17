// SPDX-License-Identifier: MIT OR Apache-2.0
//! Unit tests per adapter against the synthetic fixture HOME.

mod common;

use common::Fixture;
use movara::backup;
use movara::encodings;
use movara::protobuf::pb_replace;
use movara::spec::ReplaceSpec;
use std::fs;

fn read(p: &std::path::Path) -> String {
    fs::read_to_string(p).unwrap()
}

#[test]
fn boundary_safety() {
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    let s = spec.replace("/p/abc2 /p/abc-def /p/abc /p/abc/x \"");
    assert_eq!(s, "/p/abc2 /p/abc-def /p/cba /p/cba/x \"");
}

#[test]
fn md5_derived_token_replaced_in_content() {
    // windsurf keys context_state/database dirs by md5(path); content
    // mentioning that hash must be rewritten alongside the path itself
    let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
    let content = format!(
        "{{\"dir_hash\":\"{}\",\"other\":\"{}\"}}",
        encodings::md5_hex("/p/abc"),
        encodings::md5_hex("/p/zzz"),
    );
    let out = spec.replace(&content);
    assert!(out.contains(&encodings::md5_hex("/p/cba")));
    // unrelated md5 stays untouched
    assert!(out.contains(&encodings::md5_hex("/p/zzz")));
}

#[test]
fn encodings_match_reference_values() {
    assert_eq!(encodings::dash_encode("/home/u/a b"), "-home-u-a-b");
    assert_eq!(encodings::dash_encode_nolead("/home/u/x"), "home-u-x");
    assert_eq!(
        encodings::sha256_hex("x"),
        "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"
    );
    assert_eq!(encodings::md5_hex("x"), "9dd4e461268c8034f5c8564e155c67a6");
    // omp: home-relative bucket, no home prefix
    assert_eq!(
        encodings::omp_bucket("/home/u/works/x", "/home/u"),
        "-works-x"
    );
    assert_eq!(encodings::omp_bucket("/home/u", "/home/u"), "-home");
    assert_eq!(encodings::omp_bucket("/srv/x", "/home/u"), "-srv-x");
    // pi: --encoded--
    assert_eq!(encodings::pi_bucket("/home/u/x"), "--home-u-x--");
    // droid: only slashes become dashes
    assert_eq!(encodings::droid_bucket("/srv/my.app"), "-srv-my.app");
    // iflow keeps dots (they are in [\w\-_.]) but collapses dash runs
    assert_eq!(
        encodings::iflow_bucket("/home/u/.hidden/x"),
        "-home-u-.hidden-x"
    );
    assert_eq!(encodings::iflow_bucket("/home/u//double"), "-home-u-double");
    // zcode memory key
    assert_eq!(
        encodings::zcode_memory_key("/home/u/x"),
        format!("x-{}", encodings::sha256_16("/home/u/x"))
    );
}

#[test]
fn no_old_references_left_after_full_migration() {
    let fx = Fixture::new("full");
    fx.migrate(false);
    // allowed leftovers: chat-content layers (codex rollout line 2 message
    // text; opencode event.data blob; pi run-history task text) —
    // rewritten only with --deep
    for p in fx.grep(&fx.old) {
        let name = p.to_string_lossy().into_owned();
        let is_content = name.ends_with("rollout-x.jsonl")
            || name.ends_with("opencode/opencode.db")
            || name.ends_with("run-history.jsonl");
        assert!(is_content, "unexpected leftover: {}", name);
    }
    // derived sha256 tokens must be gone too
    assert!(fx.grep(&encodings::sha256_hex(&fx.old)).is_empty());
}

#[test]
fn claude_bucket_rename_and_json_keys() {
    let fx = Fixture::new("claude");
    fx.migrate(false);
    let projects = fx.ctx.h(".claude/projects");
    let new_enc = encodings::dash_encode(&fx.new);
    let old_enc = encodings::dash_encode(&fx.old);
    assert!(projects.join(&new_enc).is_dir());
    assert!(!projects.join(&old_enc).exists());
    // sibling /proj/abc2 untouched
    let sibling = format!("{}2", fx.old);
    assert!(projects.join(encodings::dash_encode(&sibling)).is_dir());
    let cj: serde_json::Value = serde_json::from_str(&read(&fx.ctx.h(".claude.json"))).unwrap();
    assert!(cj["projects"].get(&fx.new).is_some());
    assert!(cj["projects"].get(&fx.old).is_none());
    assert!(cj["projects"].get("/other").is_some());
    let hist = read(&fx.ctx.h(".claude/history.jsonl"));
    assert!(hist.contains(&fx.new));
    assert!(!hist.contains(&fx.old));
}

#[test]
fn codex_meta_rewritten_content_only_with_deep() {
    let fx = Fixture::new("codex");
    fx.migrate(false);
    let rollout = read(&fx.ctx.h(".codex/sessions/2026/09/03/rollout-x.jsonl"));
    let first = rollout.lines().next().unwrap();
    assert!(first.contains(&fx.new));
    assert!(rollout.lines().nth(1).unwrap().contains(&fx.old));
    let cfg = read(&fx.ctx.h(".codex/config.toml"));
    assert!(cfg.contains(&fx.new));
    assert!(!cfg.contains(&fx.old));
}

#[test]
fn gemini_slug_marker_and_project_hash() {
    let fx = Fixture::new("gemini");
    fx.migrate(false);
    let marker = read(&fx.ctx.h(".gemini/tmp/cba/.project_root"));
    assert_eq!(marker.trim(), fx.new);
    let pj: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.h(".gemini/projects.json"))).unwrap();
    assert_eq!(pj["projects"][&fx.new], "cba");
    assert!(pj["projects"].get(&fx.old).is_none());
    let chat = read(&fx.ctx.h(".gemini/tmp/cba/chats/session-1.json"));
    assert!(chat.contains(&encodings::sha256_hex(&fx.new)));
    assert!(!chat.contains(&encodings::sha256_hex(&fx.old)));
}

#[test]
fn qwen_iflow_bucket_and_hash_dir() {
    let fx = Fixture::new("qwen");
    fx.migrate(false);
    for rel in [".qwen", ".iflow"] {
        let projects = fx.ctx.h(&format!("{}/projects", rel));
        // each fork uses its own encoding: dash for qwen, iflow_bucket
        // for iflow (which preserves underscores and collapses dashes)
        let expected = if rel == ".iflow" {
            encodings::iflow_bucket(&fx.new)
        } else {
            encodings::dash_encode(&fx.new)
        };
        assert!(projects.join(&expected).is_dir());
        let tmp = fx.ctx.h(&format!("{}/tmp", rel));
        let h_new = encodings::sha256_hex(&fx.new);
        assert!(tmp.join(&h_new).is_dir());
        assert!(!tmp.join(encodings::sha256_hex(&fx.old)).exists());
    }
}

#[test]
fn opencode_directory_columns_updated() {
    let fx = Fixture::new("opencode");
    fx.migrate(false);
    let db = fx.ctx.d("opencode/opencode.db");
    let con = rusqlite::Connection::open(&db).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, fx.new);
    let dir: String = con
        .query_row("SELECT directory FROM session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dir, fx.new);
}

#[test]
fn opencode_event_message_blobs_only_with_deep() {
    // event.data / message.data are the content layer: untouched by
    // default, rewritten with --deep
    let fx = Fixture::new("ocdeep1");
    fx.migrate(false);
    let con = rusqlite::Connection::open(fx.ctx.d("opencode/opencode.db")).unwrap();
    let data: String = con
        .query_row("SELECT data FROM event", [], |r| r.get(0))
        .unwrap();
    assert!(
        data.contains(&fx.old),
        "non-deep must leave event.data alone"
    );

    let fx = Fixture::new("ocdeep2");
    fx.migrate(true);
    let con = rusqlite::Connection::open(fx.ctx.d("opencode/opencode.db")).unwrap();
    let data: String = con
        .query_row("SELECT data FROM event", [], |r| r.get(0))
        .unwrap();
    assert!(data.contains(&fx.new));
    assert!(!data.contains(&fx.old));
}

#[test]
fn omp_bucket_and_history_db() {
    let fx = Fixture::new("omp");
    fx.migrate(false);
    let sessions = fx.ctx.h(".omp/agent/sessions");
    let bucket = encodings::omp_bucket(&fx.new, &fx.ctx.home.to_string_lossy());
    assert!(sessions.join(&bucket).is_dir());
    let con = rusqlite::Connection::open(fx.ctx.h(".omp/agent/history.db")).unwrap();
    let cwd: String = con
        .query_row("SELECT cwd FROM history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cwd, fx.new);
}

#[test]
fn zcode_db_and_memory_key() {
    let fx = Fixture::new("zcode");
    fx.migrate(false);
    let con = rusqlite::Connection::open(fx.ctx.h(".zcode/cli/db/db.sqlite")).unwrap();
    let dir: String = con
        .query_row("SELECT directory FROM session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dir, fx.new);
    let cwd: String = con
        .query_row("SELECT cwd FROM workflow_run", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cwd, fx.new);
    let mem = fx.ctx.h(".zcode/cli/memories/projects");
    assert!(mem.join(encodings::zcode_memory_key(&fx.new)).is_dir());
    let meta = read(&fx.ctx.h(".zcode/cli/agents/sess_1/agent_1/metadata.json"));
    assert!(meta.contains(&fx.new));
}

#[test]
fn vscode_forks_itemtable_diskkv_and_workspace_json() {
    let fx = Fixture::new("vscode");
    fx.migrate(false);
    for app in ["Cursor", "Windsurf", "Antigravity"] {
        let db = fx.ctx.c(&format!("{}/User/globalStorage/state.vscdb", app));
        let con = rusqlite::Connection::open(&db).unwrap();
        let val: String = con
            .query_row(
                "SELECT value FROM ItemTable WHERE key='workbench.panel.aichat'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.contains(&fx.new));
        assert!(!val.contains(&fx.old));
        let wj: serde_json::Value = serde_json::from_str(&read(&fx.ctx.c(&format!(
            "{}/User/workspaceStorage/hash1/workspace.json",
            app
        ))))
        .unwrap();
        assert_eq!(wj["folder"], format!("file://{}", fx.new));
    }
    let db = fx.ctx.c("Cursor/User/globalStorage/state.vscdb");
    let con = rusqlite::Connection::open(&db).unwrap();
    let val: String = con
        .query_row(
            "SELECT value FROM cursorDiskKV WHERE key='composerData:1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(val.contains(&fx.new));
    assert!(fx
        .ctx
        .h(".cursor/projects")
        .join(encodings::dash_encode_nolead(&fx.new))
        .is_dir());
}

#[test]
fn windsurf_md5_hashed_dirs() {
    let fx = Fixture::new("windsurf");
    fx.migrate(false);
    let ctx_dir = fx.ctx.h(".codeium/windsurf/context_state");
    assert!(ctx_dir.join(movara::encodings::md5_hex(&fx.new)).is_dir());
    assert!(!ctx_dir.join(movara::encodings::md5_hex(&fx.old)).exists());
}

#[test]
fn zed_folder_paths() {
    let fx = Fixture::new("zed");
    fx.migrate(false);
    let con = rusqlite::Connection::open(fx.ctx.d("zed/threads/threads.db")).unwrap();
    let fp: String = con
        .query_row("SELECT folder_paths FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fp, fx.new);
}

#[test]
fn continue_workspace_directory_and_index() {
    let fx = Fixture::new("continue");
    fx.migrate(false);
    let sess: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.h(".continue/sessions/uuid1.json"))).unwrap();
    assert_eq!(sess["workspaceDirectory"], format!("file://{}", fx.new));
    let con = rusqlite::Connection::open(fx.ctx.h(".continue/index/index.sqlite")).unwrap();
    let dir: String = con
        .query_row("SELECT dir FROM tag_catalog", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dir, fx.new);
}

#[test]
fn pi_droid_ccconnect_aider_crush() {
    let fx = Fixture::new("misc");
    fx.migrate(false);
    // pi: memory dir + sessions bucket
    assert!(fx.ctx.h(".pi/agent/projects-memory/cba").is_dir());
    assert!(fx
        .ctx
        .h(".pi/agent/sessions")
        .join(encodings::pi_bucket(&fx.new))
        .is_dir());
    // pi: run-history cwd identity rewritten, task text waits for --deep
    let rh = read(&fx.ctx.h(".pi/agent/run-history.jsonl"));
    assert!(rh.contains(&fx.new));
    assert!(rh.contains(&fx.old));
    // droid json
    let bp: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.h(".factory/background-processes.json"))).unwrap();
    assert_eq!(bp["procs"][0]["cwd"], fx.new);
    // cc-connect dir history + hash-suffixed session file rename
    let dh: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.h(".cc-connect/dir_history.json"))).unwrap();
    assert_eq!(dh["sandbox"][0], fx.new);
    assert_eq!(dh["other"][0], "/x");
    let sess_dir = fx.ctx.h(".cc-connect/sessions");
    let expected = format!("proj_{}.json", encodings::sha256_8(&fx.new));
    assert!(sess_dir.join(&expected).is_file());
    assert!(
        fx.grep(&fx.old).is_empty()
            || fx
                .grep(&fx.old)
                .iter()
                // content layers only (rewritten with --deep)
                .all(|p| {
                    p.ends_with("rollout-x.jsonl")
                        || p.ends_with("opencode/opencode.db")
                        || p.ends_with("run-history.jsonl")
                })
    );
    // aider conf
    let conf = read(&fx.ctx.h(".aider.conf.yml"));
    assert!(conf.contains(&fx.new));
    // crush projects.json (path/data_dir identity fields)
    let pj: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.d("crush/projects.json"))).unwrap();
    let entry = &pj["projects"][0];
    assert_eq!(entry["path"], fx.new);
    assert_eq!(entry["data_dir"], format!("{}/.crush", fx.new));
    assert_eq!(pj["projects"][1]["path"], "/other");
}

#[test]
fn undo_restores_everything() {
    let fx = Fixture::new("undo");
    let backup = fx.migrate(false);
    assert!(!fx.grep(&fx.new).is_empty());
    backup::undo(&fx.tmp.join("backups"), &backup.manifest.id).unwrap();
    assert!(!fx.grep(&fx.old).is_empty());
    assert!(fx.grep(&fx.new).is_empty());
    assert!(fx
        .ctx
        .h(".claude/projects")
        .join(encodings::dash_encode(&fx.old))
        .is_dir());
    let con = rusqlite::Connection::open(fx.ctx.d("opencode/opencode.db")).unwrap();
    let wt: String = con
        .query_row("SELECT worktree FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wt, fx.old);
    // cc-connect hash-suffixed file name restored
    let expected = format!("proj_{}.json", encodings::sha256_8(&fx.old));
    assert!(fx.ctx.h(".cc-connect/sessions").join(&expected).is_file());
}

#[test]
fn dry_run_changes_nothing() {
    let fx = Fixture::new("dry");
    let spec = ReplaceSpec::new(&fx.old, &fx.new).unwrap();
    let mut backup =
        movara::backup::Backup::new(&fx.tmp.join("backups"), &spec, vec!["*".to_string()], true);
    for a in movara::adapters::all() {
        if a.installed(&fx.ctx) {
            a.migrate(&fx.ctx, &spec, &mut backup, false).unwrap();
        }
    }
    backup.save().unwrap();
    assert!(!fx.grep(&fx.old).is_empty());
    assert!(fx.grep(&fx.new).is_empty());
    assert!(!fx.tmp.join("backups").exists());
}

#[test]
fn protobuf_rewriter_roundtrip() {
    let fx = Fixture::new("pb");
    let path = fx.old.clone();
    let buf = {
        let mut b = vec![0x0a, path.len() as u8];
        b.extend_from_slice(path.as_bytes());
        b.extend_from_slice(&[0x10, 0x2a]);
        b
    };
    let spec = ReplaceSpec::new(&fx.old, &fx.new).unwrap();
    let (out, changed) = pb_replace(&buf, &spec);
    assert!(changed);
    let new_path = fx.new.clone();
    let mut expected = vec![0x0a, new_path.len() as u8];
    expected.extend_from_slice(new_path.as_bytes());
    expected.extend_from_slice(&[0x10, 0x2a]);
    assert_eq!(out, expected);
    let (out2, ch2) = pb_replace(&[0x0a, 0x03, b'a', b'b', b'c', 0x10, 0x2a], &spec);
    assert!(!ch2);
    assert_eq!(out2, vec![0x0a, 0x03, b'a', b'b', b'c', 0x10, 0x2a]);
}
