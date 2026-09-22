// SPDX-License-Identifier: MIT OR Apache-2.0
//! Unit tests per adapter against the synthetic fixture HOME.

mod common;

use common::Fixture;
use movara::backup;
use movara::encodings;
use movara::protobuf::pb_replace;
use movara::spec::ReplaceSpec;
use serde_json::json;
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
        let name = p.to_string_lossy().replace('\\', "/");
        let is_content = name.ends_with("rollout-x.jsonl")
            || name.ends_with("opencode/opencode.db")
            || name.ends_with("run-history.jsonl")
            // goose messages.content_json (chat body in the db)
            || name.ends_with("sessions/sessions.db");
        assert!(is_content, "unexpected leftover: {}", name);
    }
    // derived sha256 tokens must be gone too
    assert!(fx.grep(&encodings::sha256_hex(&fx.old)).is_empty());
}

fn json_val(line: &str) -> serde_json::Value {
    serde_json::from_str(line).unwrap()
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
    // JSON on disk escapes Windows backslashes — compare parsed values
    let hist_line = json_val(
        read(&fx.ctx.h(".claude/history.jsonl"))
            .lines()
            .next()
            .unwrap(),
    );
    assert_eq!(hist_line["project"], json!(fx.new));
}

#[test]
fn codex_meta_rewritten_content_only_with_deep() {
    let fx = Fixture::new("codex");
    fx.migrate(false);
    let rollout = read(&fx.ctx.h(".codex/sessions/2026/09/03/rollout-x.jsonl"));
    let first = json_val(rollout.lines().next().unwrap());
    assert_eq!(first["payload"]["cwd"], json!(fx.new));
    let second = json_val(rollout.lines().nth(1).unwrap());
    assert_eq!(
        second["payload"]["content"],
        json!(format!("mentions {} in text", fx.old))
    );
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
    let v = json_val(&data);
    assert_eq!(
        v["info"]["directory"],
        json!(fx.old),
        "non-deep must leave event.data alone"
    );

    let fx = Fixture::new("ocdeep2");
    fx.migrate(true);
    let con = rusqlite::Connection::open(fx.ctx.d("opencode/opencode.db")).unwrap();
    let data: String = con
        .query_row("SELECT data FROM event", [], |r| r.get(0))
        .unwrap();
    let v = json_val(&data);
    assert_eq!(v["info"]["directory"], json!(fx.new));
    assert_eq!(v["text"], json!(format!("at {}", fx.new)));
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
    let meta = json_val(&read(
        &fx.ctx.h(".zcode/cli/agents/sess_1/agent_1/metadata.json"),
    ));
    assert_eq!(meta["workspace"], json!(fx.new));
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
        let v = json_val(&val);
        assert_eq!(v["cwd"], json!(fx.new));
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
    let cd = json_val(&val);
    assert_eq!(cd["workspaceIdentifier"]["uri"]["fsPath"], json!(fx.new));
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
    assert!(
        rh.contains(&fx.new) || {
            let v = json_val(rh.lines().next().unwrap());
            v["cwd"] == json!(fx.new)
        }
    );
    assert!(
        rh.contains(&fx.old) || {
            let v = json_val(rh.lines().next().unwrap());
            v["task"] == json!(format!("work on {}", fx.old))
        }
    );
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
                    let n = p.to_string_lossy().replace('\\', "/");
                    n.ends_with("rollout-x.jsonl")
                        || n.ends_with("opencode/opencode.db")
                        || n.ends_with("run-history.jsonl")
                        || n.ends_with("sessions/sessions.db")
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

#[test]
fn kimi_buckets_files_and_index_all_move() {
    let fx = Fixture::new("kimi");
    fx.migrate(false);
    let b_old = encodings::kimi_bucket(&fx.old);
    let b_new = encodings::kimi_bucket(&fx.new);
    let root = fx.ctx.h(".kimi-code");
    // sessions/<bucket> dir renamed; file-history/ + workspace-trust/
    // bucket FILES renamed
    assert!(root.join(format!("sessions/{b_new}")).is_dir());
    assert!(!root.join(format!("sessions/{b_old}")).exists());
    assert!(root.join(format!("file-history/{b_new}")).is_file());
    assert!(root.join(format!("workspace-trust/{b_new}")).is_file());
    assert!(!root.join(format!("file-history/{b_old}")).exists());
    assert!(!root.join(format!("workspace-trust/{b_old}")).exists());
    // no stale bucket token anywhere; no old path outside the known
    // chat-content layers (rewritten with --deep)
    assert!(fx.grep(&b_old).is_empty(), "old bucket token must be gone");
    assert!(
        fx.grep(&fx.old).iter().all(|p| {
            let n = p.to_string_lossy().replace('\\', "/");
            n.ends_with("rollout-x.jsonl")
                || n.ends_with("opencode/opencode.db")
                || n.ends_with("run-history.jsonl")
                || n.ends_with("sessions/sessions.db")
        }),
        "unexpected leftover: {:?}",
        fx.grep(&fx.old)
    );
    // workspaces.json: bucket key + root value
    let ws: serde_json::Value = serde_json::from_str(&read(&root.join("workspaces.json"))).unwrap();
    assert!(ws["workspaces"].as_object().unwrap().contains_key(&b_new));
    assert_eq!(ws["workspaces"][&b_new]["root"], json!(fx.new));
    assert_eq!(
        ws["workspaces"][&b_new]["name"],
        json!(encodings::basename(&fx.new))
    );
    // session_index: workDir + sessionDir through the new bucket
    let idx: serde_json::Value =
        serde_json::from_str(&read(&root.join("session_index.jsonl"))).unwrap();
    assert_eq!(idx["workDir"], json!(fx.new));
    let session_dir = idx["sessionDir"].as_str().unwrap().replace('\\', "/");
    assert!(
        session_dir.contains(&b_new),
        "sessionDir through new bucket"
    );
    // state.json workDir + homedir; task cwd
    let state: serde_json::Value = serde_json::from_str(&read(
        &root.join(format!("sessions/{b_new}/session_1/state.json")),
    ))
    .unwrap();
    assert_eq!(state["workDir"], json!(fx.new));
    let homedir = state["agents"]["main"]["homedir"]
        .as_str()
        .unwrap()
        .replace('\\', "/");
    assert!(homedir.contains(&b_new), "homedir through new bucket");
    let task: serde_json::Value = serde_json::from_str(&read(&root.join(format!(
        "sessions/{b_new}/session_1/agents/main/tasks/bash-abc123.json"
    ))))
    .unwrap();
    assert_eq!(task["cwd"], json!(fx.new));
    // the wire stream's runtime binding carries the workspace id —
    // the server replays it to associate the session with its
    // workspace, so a stale id revives the old workspace in the UI
    let wire = read(&root.join(format!("sessions/{b_new}/session_1/agents/main/wire.jsonl")));
    assert!(
        wire.contains(&format!("\"workspaceId\":\"{b_new}\"")),
        "runtime.set_binding must follow the move"
    );
    // trust root follows the move
    let trust: serde_json::Value =
        serde_json::from_str(&read(&root.join(format!("workspace-trust/{b_new}")))).unwrap();
    assert_eq!(trust["root"], json!(fx.new));
    // the index cache is removed with the other derived stores (its
    // bare bucket ids were text-rewritten in earlier iterations, but a
    // cache that survives can still serve stale shapes — invalidate)
    assert!(!root.join("sessions/.index-cache/scan.json").exists());
    // derived stores are REMOVED (regenerable): the server serves
    // session queries from cache/query-store and never re-reads the
    // authoritative files until it is invalidated — a stale store
    // revived the old workspace in the live UI
    assert!(!root.join("search-index/CURRENT").exists());
    assert!(!root.join("cache/query-store/shard-00/db.wal").exists());
    // the server event stream: workspace registry + session identity
    // (bucket ids under generic keys, roots and cwds) all moved. The
    // replayed display name follows the move as well.
    // JSON text carries Windows paths escaped, so compare forms
    // normalized: escaped pairs and single backslashes both -> /
    let norm = |s: &str| s.replace("\\\\", "/").replace('\\', "/");
    let global = norm(&read(&root.join("server/events/__global__.jsonl")));
    assert!(global.contains(&b_new) && !global.contains(&b_old));
    assert!(global.contains(&norm(&fx.new)) && !global.contains(&norm(&fx.old)));
    assert!(
        !global.contains(&format!("\"name\":\"{}\"", encodings::basename(&fx.old)))
            && global.contains(&format!("\"name\":\"{}\"", encodings::basename(&fx.new))),
        "event workspace name follows the move"
    );
    let session_evt = norm(&read(&root.join("server/events/session_1.jsonl")));
    assert!(session_evt.contains(&b_new) && !session_evt.contains(&b_old));
    assert!(session_evt.contains(&norm(&fx.new)) && !session_evt.contains(&norm(&fx.old)));
}

#[test]
fn live_agent_processes_reports_nothing_for_idle_names() {
    struct Idle;
    impl movara::adapters::Adapter for Idle {
        fn name(&self) -> &'static str {
            "idle"
        }
        fn display(&self) -> &'static str {
            "Idle"
        }
        fn note(&self) -> &'static str {
            ""
        }
        fn state_paths(&self, _ctx: &movara::ctx::Ctx) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn process_names(&self) -> &'static [&'static str] {
            &["movara-definitely-not-running-xyz"]
        }
    }
    let list: Vec<Box<dyn movara::adapters::Adapter>> = vec![Box::new(Idle)];
    assert!(
        movara::adapters::live_agent_processes(&list).is_empty(),
        "an idle process name must not gate"
    );
    // the real registry never panics, whatever is running locally
    let _ = movara::adapters::live_agent_processes(&movara::adapters::all());
}

#[test]
fn goose_working_dir_legacy_metadata_and_permissions_move() {
    let fx = Fixture::new("units-goose");
    fx.migrate(false);
    let con = rusqlite::Connection::open(fx.ctx.d(&format!(
        "{}/sessions/sessions.db",
        common::goose_data_rel()
    )))
    .unwrap();
    let wd: String = con
        .query_row("SELECT working_dir FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wd, fx.new);
    drop(con);
    let first = read(&fx.ctx.d(&format!(
        "{}/sessions/20260901_000000.jsonl",
        common::goose_data_rel()
    )))
    .lines()
    .next()
    .unwrap()
    .to_string();
    let meta: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(meta["working_dir"], json!(fx.new));
    let perms: serde_json::Value = serde_json::from_str(&read(
        &fx.ctx
            .h(&common::goose_config_rel())
            .join("permissions/tool_permissions.json"),
    ))
    .unwrap();
    assert!(
        perms["per_project"]
            .as_object()
            .unwrap()
            .contains_key(&fx.new),
        "permission keys follow the move"
    );
    assert!(!perms["per_project"]
        .as_object()
        .unwrap()
        .contains_key(&fx.old));
}

#[test]
fn cline_family_buckets_shadow_gits_and_task_history_move() {
    let fx = Fixture::new("units-cline");
    fx.migrate(false);
    let mut h: u32 = 0;
    for u in fx.old.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(u));
    }
    let old_hash = h.to_string();
    let mut h2: u32 = 0;
    for u in fx.new.encode_utf16() {
        h2 = h2.wrapping_mul(31).wrapping_add(u32::from(u));
    }
    let new_hash = h2.to_string();
    let gs = "Code/User/globalStorage/saoudrizwan.claude-dev";
    let base = fx.ctx.c(gs);
    // Cline checkpoints bucket renamed (decimal cwdHash) and its shadow
    // git's core.worktree follows
    if !base.join(format!("checkpoints/{new_hash}")).is_dir() {
        let listing = std::fs::read_dir(base.join("checkpoints"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        panic!(
            "checkpoints/{new_hash} missing; old={old_hash} listing={listing:?} base={:?}",
            base.display()
        );
    }
    assert!(!base.join(format!("checkpoints/{old_hash}")).exists());
    let cfg = read(&base.join(format!("checkpoints/{new_hash}/.git/config")));
    assert!(cfg.contains(&fx.new) && !cfg.contains(&fx.old));
    // task tool paths (identity key "path") + file-based task history
    let hist: serde_json::Value =
        serde_json::from_str(&read(&base.join("state/taskHistory.json"))).unwrap();
    assert_eq!(hist[0]["cwdOnTaskInitialization"], json!(fx.new));
    assert_eq!(hist[0]["shadowGitConfigWorkTree"], json!(fx.new));
    let conv: serde_json::Value = serde_json::from_str(&read(
        &base.join("tasks/1770000000000/api_conversation_history.json"),
    ))
    .unwrap();
    assert_eq!(
        conv[1]["tool_use"]["input"]["path"],
        json!(format!("{}/main.rs", fx.new))
    );
    // Roo: index workspace fields + per-task shadow git core.worktree;
    // the stale index cache is removed
    let roo = fx
        .ctx
        .c("Code/User/globalStorage/rooveterinaryinc.roo-cline");
    let idx: serde_json::Value =
        serde_json::from_str(&read(&roo.join("tasks/_index.json"))).unwrap();
    assert_eq!(idx["entries"][0]["workspace"], json!(fx.new));
    let roo_cfg = read(&roo.join("tasks/1770000000001/checkpoints/.git/config"));
    assert!(roo_cfg.contains(&fx.new) && !roo_cfg.contains(&fx.old));
    let old_cache = format!(
        "roo-index-cache-{}.json",
        movara::encodings::sha256_hex(&fx.old)
    );
    assert!(!roo.join(&old_cache).exists(), "index cache invalidated");
    // Kilo classic: sha256[:16] session bucket renamed
    let kilo = fx.ctx.c("Code/User/globalStorage/kilocode.kilo-code");
    let k16_new = &movara::encodings::sha256_hex(&fx.new)[..16];
    assert!(kilo.join(format!("sessions/{k16_new}")).is_dir());
    // IDE state.vscdb: the Cline key's task history moved; the
    // unrelated extension's row is untouched
    let db = fx.ctx.c("Code/User/globalStorage/state.vscdb");
    let con = rusqlite::Connection::open(&db).unwrap();
    let v: String = con
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'saoudrizwan.claude-dev'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(v.contains(&fx.new) && !v.contains(&fx.old));
    let other: String = con
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'some.other.ext'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(other, "{\"note\": \"not ours\"}");
}

#[test]
fn openhands_project_buckets_and_working_dir_move() {
    let fx = Fixture::new("units-oh");
    fx.migrate(false);
    let old_h = movara::encodings::sha256_hex(&fx.old);
    let new_h = movara::encodings::sha256_hex(&fx.new);
    let root = fx.ctx.h(".openhands");
    assert!(root.join(format!("projects/{new_h}")).is_dir());
    assert!(!root.join(format!("projects/{old_h}")).exists());
    let evt: serde_json::Value = serde_json::from_str(&read(
        &root.join("conversations/conv1/events/event-00001-abc.json"),
    ))
    .unwrap();
    assert_eq!(evt["payload"]["session"]["metadata"]["cwd"], json!(fx.new));
    let settings: serde_json::Value =
        serde_json::from_str(&read(&root.join("agent_settings.json"))).unwrap();
    assert_eq!(settings["working_dir"], json!(fx.new));
}

#[test]
fn codebuff_basename_bucket_and_run_state_move() {
    let fx = Fixture::new("units-cb");
    fx.migrate(false);
    let old_b = movara::encodings::basename(&fx.old);
    let new_b = movara::encodings::basename(&fx.new);
    let root = fx.ctx.c("manicode/projects");
    let chat = "chats/2026-09-01T00-00-00-000Z";
    assert!(root.join(&new_b).join(chat).is_dir());
    assert!(!root.join(&old_b).exists());
    let rs: serde_json::Value =
        serde_json::from_str(&read(&root.join(&new_b).join(chat).join("run-state.json"))).unwrap();
    assert_eq!(rs["sessionState"]["cwd"], json!(fx.new));
}

#[test]
fn gptme_workspace_config_files_lists_and_symlink_move() {
    let fx = Fixture::new("units-gptme");
    fx.migrate(false);
    let conv = fx.ctx.d("gptme/logs/2026-09-01-happy-walrus");
    // config.toml [chat] workspace follows (absolute form)
    let cfg = read(&conv.join("config.toml"));
    assert!(cfg.contains(&fx.new) && !cfg.contains(&fx.old));
    // message files lists follow (identity list key)
    let jsonl = read(&conv.join("conversation.jsonl"));
    let last = jsonl.lines().last().unwrap();
    let msg: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(msg["files"][0], json!(format!("{}/main.rs", fx.new)));
    // the workspace symlink is retargeted, not followed
    #[cfg(unix)]
    {
        let t = std::fs::read_link(conv.join("workspace")).unwrap();
        assert_eq!(t, std::path::PathBuf::from(&fx.new));
    }
}

#[test]
fn gptme_tilde_workspace_form_is_rewritten() {
    // gptme abbreviates under-home paths to ~/... on save; the absolute
    // needle alone never matches that form
    use movara::ctx::Ctx;
    let tmp = std::env::temp_dir().join(format!("movara-gptme-tilde-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("home/.local/share/gptme/logs/c1")).unwrap();
    fs::create_dir_all(tmp.join("home/proj/abc")).unwrap();
    let ctx = Ctx {
        home: tmp.join("home"),
        config_home: tmp.join("home/.config"),
        data_home: tmp.join("home/.local/share"),
        data_local: Some(tmp.join("home/.local/share")),
    };
    let old = tmp.join("home/proj/abc").to_string_lossy().into_owned();
    let new = tmp.join("home/proj/cba").to_string_lossy().into_owned();
    let conv = ctx.d("gptme/logs/c1");
    fs::write(
        conv.join("config.toml"),
        "[chat]\nname = \"n\"\nworkspace = \"~/proj/abc\"\n",
    )
    .unwrap();
    fs::write(conv.join("conversation.jsonl"), "{}\n").unwrap();
    let spec = ReplaceSpec::new(&old, &new).unwrap();
    let mut bk = movara::backup::Backup::new(&tmp.join("bk"), &spec, vec![], false);
    let gptme = movara::adapters::get_adapters(Some(&["gptme".to_string()])).unwrap();
    gptme[0].migrate(&ctx, &spec, &mut bk, false).unwrap();
    let cfg = read(&conv.join("config.toml"));
    assert!(
        cfg.contains("~/proj/cba") && !cfg.contains("~/proj/abc"),
        "tilde form must follow: {}",
        cfg
    );
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn qoder_memories_buckets_ide_state_and_lingma_move() {
    let fx = Fixture::new("units-qoder");
    fx.migrate(false);
    let enc_old = movara::encodings::dash_encode(&fx.old);
    let enc_new = movara::encodings::dash_encode(&fx.new);
    for root in [".qoder", ".lingma/qoder-cn"] {
        let projects = fx.ctx.h(&format!("{root}/memories/019f8e2a/projects"));
        assert!(projects.join(&enc_new).is_dir(), "{root} bucket renamed");
        assert!(!projects.join(&enc_old).exists());
    }
    let mcp: serde_json::Value = serde_json::from_str(&read(&fx.ctx.h(".qoder/mcp.json"))).unwrap();
    assert_eq!(mcp["mcpServers"]["x"]["cwd"], json!(fx.new));
    let db = fx.ctx.c("Qoder/User/globalStorage/state.vscdb");
    let con = rusqlite::Connection::open(&db).unwrap();
    let v: String = con
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'workbench.panel.aichat'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(v.contains(&fx.new) && !v.contains(&fx.old));
    let ws = read(&fx.ctx.c("Qoder/User/workspaceStorage/ws1/workspace.json"));
    assert!(ws.contains(&fx.new) && ws.contains("file://"));
}

#[test]
fn trae_ide_state_and_definitions_move() {
    let fx = Fixture::new("units-trae");
    fx.migrate(false);
    let db = fx.ctx.c("Trae CN/User/globalStorage/state.vscdb");
    let con = rusqlite::Connection::open(&db).unwrap();
    let v: String = con
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'aicode.chatSessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(v.contains(&fx.new) && !v.contains(&fx.old));
    let ws = read(&fx.ctx.c("Trae CN/User/workspaceStorage/ws2/workspace.json"));
    assert!(ws.contains(&fx.new) && !ws.contains(&fx.old));
    let mcp: serde_json::Value = serde_json::from_str(&read(&fx.ctx.h(".trae/mcp.json"))).unwrap();
    assert_eq!(mcp["mcpServers"]["y"]["cwd"], json!(fx.new));
}

#[test]
fn copilot_definitions_move() {
    let fx = Fixture::new("units-copilot");
    fx.migrate(false);
    let a: serde_json::Value =
        serde_json::from_str(&read(&fx.ctx.h(".copilot/agents/review.json"))).unwrap();
    assert_eq!(a["cwd"], json!(fx.new));
}

#[test]
fn warp_generic_text_column_sweep_moves_paths() {
    let fx = Fixture::new("units-warp");
    fx.migrate(false);
    let con = rusqlite::Connection::open(common::warp_db(&fx.ctx)).unwrap();
    let cwd: String = con
        .query_row("SELECT cwd FROM launches", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cwd, fx.new);
    let uri: String = con
        .query_row("SELECT workspace_uri FROM agent_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(uri, format!("file://{}", fx.new));
    // untouched non-path columns keep their values
    let cmd: String = con
        .query_row("SELECT cmd FROM launches", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cmd, "cargo build");
    let score: i64 = con
        .query_row("SELECT score FROM agent_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(score, 5);
}

#[test]
fn openinterpreter_codex_shape_and_generic_memory_sweep() {
    let fx = Fixture::new("units-oi");
    fx.migrate(false);
    let root = fx.ctx.h(".openinterpreter");
    // rollout session_meta cwd (identity cascade)
    let first = read(&root.join("sessions/2026/09/01/rollout-2026-09-01T00-00-00-x.jsonl"))
        .lines()
        .next()
        .unwrap()
        .to_string();
    let meta: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(meta["payload"]["cwd"], json!(fx.new));
    // config.toml [projects] trust key follows
    let cfg = read(&root.join("config.toml"));
    assert!(cfg.contains(&fx.new) && !cfg.contains(&fx.old));
    // threads.cwd (known shape)
    let con = rusqlite::Connection::open(root.join("state_5.sqlite")).unwrap();
    let cwd: String = con
        .query_row("SELECT cwd FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cwd, fx.new);
    // closed-schema memories db swept generically
    let mem = rusqlite::Connection::open(root.join("memories_1.sqlite")).unwrap();
    let body: String = mem
        .query_row("SELECT body FROM memories", [], |r| r.get(0))
        .unwrap();
    assert!(body.contains(&fx.new) && !body.contains(&fx.old));
}
