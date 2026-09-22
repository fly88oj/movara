// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared fixture: a synthetic agent-state HOME with every adapter's real
//! layout (mirrors the verified layouts in docs/research.md).
// shared across test binaries; not every binary uses every helper
#![allow(dead_code)]

use movara::adapters;
use movara::backup::Backup;
use movara::ctx::Ctx;
use movara::spec::ReplaceSpec;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Fixture {
    pub tmp: PathBuf,
    pub ctx: Ctx,
    pub old: String,
    pub new: String,
}

impl Fixture {
    pub fn new(tag: &str) -> Self {
        let raw = std::env::temp_dir().join(format!("movara-rs-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&raw);
        // create first, then canonicalize — canonicalize on a non-existent
        // path fails; macOS /tmp -> /private/tmp must be resolved before
        // tests compare against realpath-derived bucket names
        fs::create_dir_all(&raw).unwrap();
        let tmp = movara::ctx::de_verbatim(&std::fs::canonicalize(&raw).unwrap_or(raw));
        let home = tmp.join("home");
        let old_dir = tmp.join("proj").join("abc");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("hello.txt"), "marker\n").unwrap();
        let ctx = Ctx {
            home: home.clone(),
            config_home: home.join(".config"),
            data_home: home.join(".local").join("share"),
            data_local: Some(home.join(".local").join("share")),
        };
        let f = Fixture {
            old: old_dir.to_string_lossy().into_owned(),
            new: tmp.join("proj").join("cba").to_string_lossy().into_owned(),
            tmp,
            ctx,
        };
        f.build_all();
        f
    }

    /// write a JSON line/object with proper escaping (Windows paths
    /// contain backslashes that format! would emit as invalid JSON
    /// escapes — serde_json escapes them correctly)
    fn wj(&self, rel: &str, v: serde_json::Value) -> PathBuf {
        self.w(rel, &v.to_string())
    }

    fn wl(&self, rel: &str, v: serde_json::Value) -> PathBuf {
        self.w(rel, &format!("{}\n", v))
    }

    pub fn w(&self, rel: &str, data: &str) -> PathBuf {
        let p = self.ctx.home.join(rel);
        fs::create_dir_all(p.parent().unwrap_or(&p)).unwrap();
        fs::write(&p, data).unwrap();
        p
    }

    pub fn build_all(&self) {
        self.build_claude();
        self.build_codex();
        self.build_gemini();
        self.build_gemini_fork(".qwen");
        self.build_gemini_fork(".iflow");
        self.build_opencode();
        self.build_omp();
        self.build_zcode();
        self.build_vscode("Cursor");
        self.build_vscode("Windsurf");
        self.build_vscode("Antigravity");
        self.build_cursor_cli();
        self.build_windsurf_codeium();
        self.build_zed();
        self.build_continue();
        self.build_pi();
        self.build_droid();
        self.build_crush();
        self.build_ccconnect();
        self.build_aider();
        self.build_kimi();
        self.build_goose();
        self.build_cline();
        self.build_openhands();
        self.build_codebuff();
        self.build_gptme();
        self.build_qoder();
        self.build_trae();
        self.build_copilot();
        self.build_warp();
        self.build_openinterpreter();
    }

    /// Open Interpreter: Codex layout re-rooted at ~/.openinterpreter
    fn build_openinterpreter(&self) {
        self.w(
            ".openinterpreter/sessions/2026/09/01/rollout-2026-09-01T00-00-00-x.jsonl",
            &format!(
                "{}\n{}\n",
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {"id": "u1", "cwd": self.old}
                }),
                serde_json::json!({"type": "response_item", "payload": {"type": "message", "content": "hi"}})
            ),
        );
        self.w(
            ".openinterpreter/config.toml",
            &format!("[projects.\"{}\"]\ntrust_level = \"trusted\"\n", self.old),
        );
        let db = self.ctx.h(".openinterpreter/state_5.sqlite");
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, cwd TEXT NOT NULL);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO threads VALUES (?,?,?)",
            rusqlite::params!["t1", "/x/rollout.jsonl", self.old],
        )
        .unwrap();
        drop(con);
        // a closed-schema db (memories) — swept generically
        let mem = self.ctx.h(".openinterpreter/memories_1.sqlite");
        let con2 = rusqlite::Connection::open(&mem).unwrap();
        con2.execute_batch("CREATE TABLE memories (id INTEGER PRIMARY KEY, body TEXT);")
            .unwrap();
        con2.execute(
            "INSERT INTO memories VALUES (?,?)",
            rusqlite::params![1, format!("project lives at {}", self.old)],
        )
        .unwrap();
        drop(con2);
    }

    /// Warp: synthetic warp-shaped db — closed real schema, the adapter
    /// sweeps PRAGMA-discovered text columns generically
    fn build_warp(&self) {
        let db = warp_db(&self.ctx);
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE launches (id INTEGER PRIMARY KEY, cwd TEXT, cmd TEXT); \
             CREATE TABLE agent_runs (id INTEGER PRIMARY KEY, prompt TEXT, \
             workspace_uri TEXT, score INTEGER);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO launches VALUES (?,?,?)",
            rusqlite::params![1, self.old, "cargo build"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO agent_runs VALUES (?,?,?,?)",
            rusqlite::params![1, "fix it", format!("file://{}", self.old), 5],
        )
        .unwrap();
        drop(con);
    }

    /// Qoder/Lingma: VS Code fork IDE state + memories projects buckets
    /// (dash-encoded) in both ~/.qoder and ~/.lingma/qoder-cn
    fn build_qoder(&self) {
        let enc_old = movara::encodings::dash_encode(&self.old);
        for root in [".qoder", ".lingma/qoder-cn"] {
            self.w(
                &format!("{root}/memories/019f8e2a/projects/{enc_old}/note.md"),
                "memory note\n",
            );
        }
        self.wj(
            ".qoder/mcp.json",
            serde_json::json!({"mcpServers": {"x": {"command": "npx", "cwd": self.old}}}),
        );
        let u = ".config/Qoder/User";
        let db = self.ctx.c("Qoder/User/globalStorage/state.vscdb");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        con.execute(
            "INSERT INTO ItemTable VALUES (?,?)",
            rusqlite::params![
                "workbench.panel.aichat",
                serde_json::json!({"history": [{"workspace": self.old}]}).to_string()
            ],
        )
        .unwrap();
        drop(con);
        self.w(
            &format!("{u}/workspaceStorage/ws1/workspace.json"),
            &format!("{{\"folder\": \"file://{}\"}}\n", self.old),
        );
    }

    /// Trae: VS Code fork IDE state (CN build dir with a space) + the
    /// thin ~/.trae definition root
    fn build_trae(&self) {
        let u = ".config/Trae CN/User";
        let db = self.ctx.c("Trae CN/User/globalStorage/state.vscdb");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        con.execute(
            "INSERT INTO ItemTable VALUES (?,?)",
            rusqlite::params![
                "aicode.chatSessions",
                serde_json::json!({"sessions": [{"workspace": self.old}]}).to_string()
            ],
        )
        .unwrap();
        drop(con);
        self.w(
            &format!("{u}/workspaceStorage/ws2/workspace.json"),
            &format!("{{\"folder\": \"file://{}\"}}\n", self.old),
        );
        self.wj(
            ".trae/mcp.json",
            serde_json::json!({"mcpServers": {"y": {"command": "npx", "cwd": self.old}}}),
        );
    }

    /// GitHub Copilot CLI: definition layer under ~/.copilot
    fn build_copilot(&self) {
        self.wj(
            ".copilot/agents/review.json",
            serde_json::json!({"name": "review", "cwd": self.old, "tools": ["bash"]}),
        );
        self.w(
            ".copilot/skills/notes.md",
            "# notes\nsome agent skill documentation\n",
        );
    }

    /// gptme: ~/.local/share/gptme/logs/<date>-<name>/ with config.toml
    /// [chat] workspace, a workspace symlink, and files lists in the
    /// conversation jsonl
    fn build_gptme(&self) {
        let conv = ".local/share/gptme/logs/2026-09-01-happy-walrus";
        self.w(
            &format!("{conv}/config.toml"),
            &format!(
                "[chat]\nname = \"happy walrus\"\nworkspace = \"{}\"\n",
                self.old
            ),
        );
        self.w(
            &format!("{conv}/conversation.jsonl"),
            &format!(
                "{}\n{}\n",
                serde_json::json!({"role": "user", "content": "hi"}),
                serde_json::json!({"role": "assistant", "content": "done",
                    "files": [format!("{}/main.rs", self.old)]})
            ),
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(&self.old, self.ctx.h(&format!("{conv}/workspace"))).unwrap();
    }

    /// OpenHands: ~/.openhands conversations/<uuid>/events + base
    /// state working_dir + projects/<sha256(realpath)> prompt history
    fn build_openhands(&self) {
        let h = &self.old;
        self.wj(
            ".openhands/conversations/conv1/events/event-00001-abc.json",
            serde_json::json!({
                "type": "event.session.created",
                "payload": {"session": {"id": "conv1", "metadata": {"cwd": h}}}
            }),
        );
        self.wj(
            ".openhands/agent_settings.json",
            serde_json::json!({"working_dir": h, "model": "x"}),
        );
        let p = movara::encodings::sha256_hex(h);
        self.wj(
            &format!(".openhands/projects/{p}/prompt_history.json"),
            serde_json::json!({"prompts": ["hi"]}),
        );
    }

    /// Codebuff/Freebuff: ~/.config/manicode/projects/<basename>/
    /// chats/<ts>/ with run-state sessionState cwd
    fn build_codebuff(&self) {
        let base = movara::encodings::basename(&self.old);
        let chat = format!(".config/manicode/projects/{base}/chats/2026-09-01T00-00-00-000Z");
        self.wj(
            &format!("{chat}/chat-meta.json"),
            serde_json::json!({"messageCount": 2, "firstPrompt": "hi"}),
        );
        self.wj(
            &format!("{chat}/run-state.json"),
            serde_json::json!({"sessionState": {"cwd": self.old, "note": "x"}}),
        );
    }

    /// Cline family: one globalStorage per marketplace id under the
    /// IDE's User dir, with task files, hash-named checkpoint buckets
    /// holding shadow gits (core.worktree), the Roo task index, and
    /// task history in the IDE's state.vscdb ItemTable
    fn build_cline(&self) {
        // the w() helper is HOME-relative; the IDE globalStorage lives
        // under the config root
        let gs = ".config/Code/User/globalStorage/saoudrizwan.claude-dev";
        // Cline <=3.x checkpoints bucket: polynomial hash of the cwd
        let mut h: u32 = 0;
        for u in self.old.encode_utf16() {
            h = h.wrapping_mul(31).wrapping_add(u32::from(u));
        }
        let cwd_hash = h.to_string();
        let ck = format!("{gs}/checkpoints/{cwd_hash}");
        self.w(
            &format!("{ck}/.git/config"),
            &format!(
                "[core]\n\tworktree = {}\n\trepositoryformatversion = 0\n",
                self.old
            ),
        );
        // task files (tool paths under the identity key "path")
        self.wj(
            &format!("{gs}/tasks/1770000000000/api_conversation_history.json"),
            serde_json::json!([
                {"role": "user", "content": "hi"},
                {"role": "assistant", "tool_use": {"name": "write", "input": {"path": format!("{}/main.rs", self.old), "content": "x"}}}
            ]),
        );
        // Cline 4.x file-based task history
        self.wj(
            &format!("{gs}/state/taskHistory.json"),
            serde_json::json!([
                {"id": "1770000000000", "task": "t", "cwdOnTaskInitialization": self.old,
                 "shadowGitConfigWorkTree": self.old}
            ]),
        );
        // Roo: task index + per-task shadow git + a stale index cache
        let roo = ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline";
        self.wj(
            &format!("{roo}/tasks/_index.json"),
            serde_json::json!({"version": 1, "entries": [{"ts": "1770000000001", "workspace": self.old}]}),
        );
        self.wj(
            &format!("{roo}/tasks/1770000000001/history_item.json"),
            serde_json::json!({"ts": "1770000000001", "workspace": self.old}),
        );
        self.w(
            &format!("{roo}/tasks/1770000000001/checkpoints/.git/config"),
            &format!("[core]\n\tworktree = {}\n", self.old),
        );
        let roo_cache = movara::encodings::sha256_hex(&self.old);
        self.w(
            &format!("{roo}/roo-index-cache-{roo_cache}.json"),
            "{\"stale\":true}",
        );
        // Kilo classic: sha256[:16] session bucket
        let kilo = ".config/Code/User/globalStorage/kilocode.kilo-code";
        let k16 = &movara::encodings::sha256_hex(&self.old)[..16];
        self.wj(
            &format!("{kilo}/sessions/{k16}/session.json"),
            serde_json::json!({"workspace": self.old, "turns": 3}),
        );
        // IDE state.vscdb ItemTable with the Cline task history array
        let db = self.ctx.c("Code/User/globalStorage/state.vscdb");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        con.execute(
            "INSERT INTO ItemTable VALUES (?,?)",
            rusqlite::params![
                "saoudrizwan.claude-dev",
                serde_json::json!([
                    {"id": "1770000000002", "cwdOnTaskInitialization": self.old,
                     "shadowGitConfigWorkTree": self.old}
                ])
                .to_string()
            ],
        )
        .unwrap();
        // an unrelated extension's row in the same db must stay alone
        con.execute(
            "INSERT INTO ItemTable VALUES (?,?)",
            rusqlite::params!["some.other.ext", "{\"note\": \"not ours\"}"],
        )
        .unwrap();
        drop(con);
    }

    /// Goose: data/sessions/sessions.db (sessions.working_dir) + a
    /// legacy flat jsonl whose first line is session metadata, plus a
    /// config-dir permissions file
    fn build_goose(&self) {
        let db = self
            .ctx
            .d(&format!("{}/sessions/sessions.db", goose_data_rel()));
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, description TEXT, \
             working_dir TEXT NOT NULL, created_at TEXT); \
             CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, content_json TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO sessions VALUES (?,?,?,?)",
            rusqlite::params!["20260901_1", "s", self.old, "2026-09-01T00:00:00Z"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO messages VALUES (?,?,?)",
            rusqlite::params![
                "m1",
                "20260901_1",
                format!("{{\"text\":\"work in {}\"}}", self.old)
            ],
        )
        .unwrap();
        drop(con);
        self.w(
            &format!(
                ".local/share/{}/sessions/20260901_000000.jsonl",
                goose_data_rel()
            ),
            &format!(
                "{}\n{}\n",
                serde_json::json!({"id": "legacy1", "working_dir": self.old}),
                serde_json::json!({"role": "user", "content": "hi"})
            ),
        );
        self.wj(
            &format!(
                "{}/permissions/tool_permissions.json",
                goose_config_rel()
            ),
            serde_json::json!({"version": 1, "per_project": {self.old.clone(): {"developer-tools": true}}}),
        );
    }

    /// Kimi Code: wd_<basename>_<sha256[:12]> buckets — a sessions/
    /// directory per bucket, file-history/ + workspace-trust/ FILES
    /// named by the bucket, session_index/state/wire identity fields and
    /// an index cache holding bare bucket ids
    fn build_kimi(&self) {
        let bucket = movara::encodings::kimi_bucket(&self.old);
        let root = ".kimi-code";
        let sess_dir = format!("{root}/sessions/{bucket}/session_1");
        let agents_main = format!("{sess_dir}/agents/main");
        self.wj(
            &format!("{root}/workspaces.json"),
            serde_json::json!({
                "version": 1,
                "workspaces": {
                    bucket.clone(): {
                        "root": self.old,
                        "name": movara::encodings::basename(&self.old),
                        "created_at": "2026-09-01T00:00:00.000Z",
                        "last_opened_at": "2026-09-01T00:00:00.000Z",
                    }
                }
            }),
        );
        self.wl(
            &format!("{root}/session_index.jsonl"),
            serde_json::json!({
                "sessionId": "ses_1",
                "sessionDir": self
                    .ctx
                    .h(&format!("{root}/sessions/{bucket}/session_1"))
                    .to_string_lossy(),
                "workDir": self.old,
            }),
        );
        self.wj(
            &format!("{sess_dir}/state.json"),
            serde_json::json!({
                "createdAt": "2026-09-01T00:00:00.000Z",
                "title": "t",
                "workDir": self.old,
                "agents": {
                    "main": {
                        "homedir": self.ctx.h(&agents_main).to_string_lossy(),
                        "type": "main",
                        "parentAgentId": null,
                    }
                },
            }),
        );
        self.w(
            &format!("{agents_main}/wire.jsonl"),
            &format!(
                "{}\n{}\n{}\n",
                serde_json::json!({"type": "metadata", "protocol_version": "1.4"}),
                serde_json::json!({"type": "session.start", "workDir": self.old}),
                // the server replays the wire stream and binds the
                // session to its workspace through this record
                serde_json::json!({
                    "type": "runtime.set_binding",
                    "workspaceId": bucket,
                })
            ),
        );
        self.wj(
            &format!("{agents_main}/tasks/bash-abc123.json"),
            serde_json::json!({"cwd": self.old, "command": "cargo build"}),
        );
        // bucket FILES under file-history/ (no paths inside) and
        // workspace-trust/ ({"root": ...})
        self.wj(
            &format!("{root}/file-history/{bucket}"),
            serde_json::json!({"sessions": [{"id": "session_1", "touchedAt": 1789975118890_i64}]}),
        );
        self.wj(
            &format!("{root}/workspace-trust/{bucket}"),
            serde_json::json!({"root": self.old, "trustedAt": 1789975118890_i64}),
        );
        // regenerable scan cache with a bare bucket id in a
        // non-identity field
        self.wj(
            &format!("{root}/sessions/.index-cache/scan.json"),
            serde_json::json!({
                "version": 1,
                "sessions": {
                    "session_1": {"ws": bucket, "meta": "state.json", "mtimeMs": 1.5, "size": 100}
                }
            }),
        );
        // the background server's event stream: workspace registry
        // events carry the bucket id + root; session events carry
        // workspace_id + metadata.cwd
        self.wl(
            &format!("{root}/server/events/__global__.jsonl"),
            serde_json::json!({
                "kind": "event",
                "seq": 1,
                "envelope": {
                    "type": "event.workspace.updated",
                    "seq": 1,
                    "session_id": null,
                    "timestamp": "2026-09-01T00:00:00.000Z",
                    "payload": {
                        "type": "event.workspace.updated",
                        "workspace": {
                            "id": bucket,
                            "root": self.old,
                            "name": movara::encodings::basename(&self.old),
                        }
                    }
                }
            }),
        );
        self.wl(
            &format!("{root}/server/events/session_1.jsonl"),
            serde_json::json!({
                "kind": "event",
                "seq": 2,
                "envelope": {
                    "type": "event.session.created",
                    "seq": 2,
                    "session_id": "session_1",
                    "timestamp": "2026-09-01T00:00:00.000Z",
                    "payload": {
                        "type": "event.session.created",
                        "session": {
                            "id": "session_1",
                            "workspace_id": bucket,
                            "metadata": {"cwd": self.old},
                        }
                    }
                }
            }),
        );
        // derived stores (the server's query store, the scan cache, the
        // search index) hold pre-migration metadata and are REMOVED by
        // the adapter — they rebuild on next launch
        self.w(&format!("{root}/search-index/CURRENT"), "ACME");
        self.w(
            &format!("{root}/cache/query-store/shard-00/db.wal"),
            "STALE",
        );
    }

    fn build_claude(&self) {
        let enc_old = movara::encodings::dash_encode(&self.old);
        self.wl(
            &format!(".claude/projects/{}/sess1.jsonl", enc_old),
            serde_json::json!({
                "type": "user", "cwd": self.old, "sessionId": "s1",
                "message": {"role": "user", "content": "hello"}
            }),
        );
        let sibling = format!("{}2", self.old);
        self.wl(
            &format!(
                ".claude/projects/{}/x.jsonl",
                movara::encodings::dash_encode(&sibling)
            ),
            serde_json::json!({"cwd": sibling}),
        );
        self.wl(
            ".claude/history.jsonl",
            serde_json::json!({"display": "hi", "project": self.old}),
        );
        self.wj(
            ".claude.json",
            serde_json::json!({
                "numStartups": 5,
                "mcpServers": {"db": {"env": {"MARKER_MCP": "marker-mcp-value"}}},
                "projects": {
                    self.old.clone(): {"allowedTools": []},
                    "/other".to_string(): {}
                }
            }),
        );
        // secret carriers that must never leave the machine in an export
        self.w(".claude/.credentials.json", "{\"token\":\"sekret-creds\"}");
        self.w(
            ".claude/shell-snapshots/snap-1.sh",
            "export SECRET=sekret-snap\n",
        );
    }

    fn build_codex(&self) {
        let meta = |id: &str| {
            serde_json::json!({
                "type": "session_meta",
                "payload": {"id": id, "cwd": self.old}
            })
            .to_string()
        };
        self.w(
            ".codex/sessions/2026/09/03/rollout-x.jsonl",
            &format!(
                "{}\n{}\n",
                meta("u1"),
                serde_json::json!({
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "content": format!("mentions {} in text", self.old)
                    }
                })
            ),
        );
        self.w(
            ".codex/archived_sessions/rollout-old.jsonl",
            &format!("{}\n", meta("u2")),
        );
        self.w(
            ".codex/config.toml",
            &format!("[projects.\"{}\"]\ntrust_level = \"trusted\"\n", self.old),
        );
    }

    fn build_gemini(&self) {
        self.w(".gemini/tmp/abc/.project_root", &format!("{}\n", self.old));
        let ph = movara::encodings::sha256_hex(&self.old);
        self.wj(
            ".gemini/tmp/abc/chats/session-1.json",
            serde_json::json!({
                "sessionId": "1",
                "projectHash": ph,
                "messages": [{"role": "user", "parts": {"text": "hi"}}]
            }),
        );
        self.w(".gemini/history/abc/prompts.jsonl", "{\"prompt\":\"hi\"}\n");
        self.wj(
            ".gemini/projects.json",
            serde_json::json!({"projects": {self.old.clone(): "abc"}}),
        );
    }

    fn build_gemini_fork(&self, rel: &str) {
        // use the vendor's own encoding: dash for qwen, iflow_bucket for
        // iflow (which keeps underscores and collapses dashes — differs
        // from dash_encode when the temp path contains `_`, as on macOS)
        let enc_old = if rel == ".iflow" {
            movara::encodings::iflow_bucket(&self.old)
        } else {
            movara::encodings::dash_encode(&self.old)
        };
        self.wl(
            &format!("{}/projects/{}/sess.jsonl", rel, enc_old),
            serde_json::json!({"cwd": self.old}),
        );
        let tmp_old = movara::encodings::sha256_hex(&self.old);
        self.wj(
            &format!("{}/tmp/{}/checkpoint.json", rel, tmp_old),
            serde_json::json!({"cwd": self.old}),
        );
    }

    fn build_opencode(&self) {
        let db = self.ctx.d("opencode/opencode.db");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT, \
             vcs TEXT, sandboxes TEXT, commands TEXT);\
             CREATE TABLE workspace (id TEXT PRIMARY KEY, type TEXT, \
             name TEXT, branch TEXT, directory TEXT, extra TEXT, \
             project_id TEXT, time_used INTEGER);\
             CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, \
             parent_id TEXT, slug TEXT, directory TEXT, title TEXT, \
             path TEXT, version TEXT);\
             CREATE TABLE project_directory (project_id TEXT, \
             directory TEXT, type TEXT, strategy TEXT, \
             time_created INTEGER);\
             CREATE TABLE event (id TEXT PRIMARY KEY, data TEXT);\
             CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO event VALUES (?,?)",
            rusqlite::params![
                "evt_1",
                serde_json::json!({
                    "info": {"directory": self.old},
                    "text": format!("at {}", self.old)
                })
                .to_string()
            ],
        )
        .unwrap();
        con.execute(
            "INSERT INTO project VALUES (?,?,?,?,?)",
            rusqlite::params!["pid1", self.old, "git", "[]", "[]"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO workspace VALUES (?,?,?,?,?,?,?,?)",
            rusqlite::params!["w1", "folder", "", "", self.old, "", "pid1", 1],
        )
        .unwrap();
        con.execute(
            "INSERT INTO session VALUES (?,?,?,?,?,?,?,?)",
            rusqlite::params!["ses_1", "pid1", "", "slug", self.old, "title", self.old, "1.0"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO project_directory VALUES (?,?,?,?,?)",
            rusqlite::params!["pid1", self.old, "folder", "auto", 1],
        )
        .unwrap();
        drop(con);
        self.wj(
            ".local/share/opencode/storage/session/abc.json",
            serde_json::json!({"id": "x", "directory": self.old}),
        );
    }

    fn build_omp(&self) {
        let enc_old = movara::encodings::omp_bucket(&self.old, &self.ctx.home.to_string_lossy());
        self.w(
            &format!(".omp/agent/sessions/{}/2026-s1.jsonl", enc_old),
            &format!(
                "{}\n{}\n",
                serde_json::json!({"type": "title", "title": "t"}),
                serde_json::json!({
                    "type": "session", "version": 3, "id": "s1", "cwd": self.old
                })
            ),
        );
        let db = self.ctx.h(".omp/agent/history.db");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE history (id INTEGER PRIMARY KEY, prompt TEXT, \
             created_at INTEGER, cwd TEXT, session_id TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO history VALUES (?,?,?,?,?)",
            rusqlite::params![1, "prompt", 1, self.old, "s1"],
        )
        .unwrap();
    }

    fn build_zcode(&self) {
        let db = self.ctx.h(".zcode/cli/db/db.sqlite");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT, \
             path TEXT, title TEXT);\
             CREATE TABLE workflow_run (id TEXT PRIMARY KEY, cwd TEXT, \
             status TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO session VALUES (?,?,?,?)",
            rusqlite::params!["sess_1", self.old, self.old, "t"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO workflow_run VALUES (?,?,?)",
            rusqlite::params!["run_1", self.old, "done"],
        )
        .unwrap();
        let key_old = movara::encodings::zcode_memory_key(&self.old);
        self.w(
            &format!(".zcode/cli/memories/projects/{}/MEMORY.md", key_old),
            "# mem\n",
        );
        self.wj(
            ".zcode/cli/agents/sess_1/agent_1/metadata.json",
            serde_json::json!({"workspace": self.old}),
        );
    }

    fn build_vscode(&self, app: &str) {
        let db = self
            .ctx
            .c(&format!("{}/User/globalStorage/state.vscdb", app));
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);\
             CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO ItemTable VALUES (?,?)",
            rusqlite::params![
                "workbench.panel.aichat",
                serde_json::json!({
                    "workspace": format!("file://{}", self.old),
                    "cwd": self.old
                })
                .to_string()
            ],
        )
        .unwrap();
        if app == "Cursor" {
            con.execute(
                "INSERT INTO cursorDiskKV VALUES (?,?)",
                rusqlite::params![
                    "composerData:1",
                    serde_json::json!({
                        "workspaceIdentifier": {
                            "uri": {
                                "fsPath": self.old,
                                "external": format!("file://{}", self.old)
                            }
                        }
                    })
                    .to_string()
                ],
            )
            .unwrap();
        }
        drop(con);
        self.wj(
            &format!(".config/{}/User/workspaceStorage/hash1/workspace.json", app),
            serde_json::json!({"folder": format!("file://{}", self.old)}),
        );
    }

    fn build_cursor_cli(&self) {
        let enc_old = movara::encodings::dash_encode_nolead(&self.old);
        self.wl(
            &format!(".cursor/projects/{}/agent-transcripts/t1.jsonl", enc_old),
            serde_json::json!({"cwd": self.old}),
        );
    }

    fn build_windsurf_codeium(&self) {
        let md5_old = movara::encodings::md5_hex(&self.old);
        self.wj(
            &format!(".codeium/windsurf/context_state/{}/state.json", md5_old),
            serde_json::json!({"cwd": self.old}),
        );
        self.w(
            ".codeium/windsurf/mcp_config.json",
            &serde_json::json!({
                "mcpServers": {"x": {"command": "/bin/ls", "cwd": self.old}}
            })
            .to_string(),
        );
    }

    fn build_zed(&self) {
        let db = self.ctx.d("zed/threads/threads.db");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, summary TEXT, \
             folder_paths TEXT, folder_paths_order TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO threads VALUES (?,?,?,?)",
            rusqlite::params!["t1", "s", self.old, "0"],
        )
        .unwrap();
    }

    fn build_continue(&self) {
        self.wj(
            ".continue/sessions/uuid1.json",
            serde_json::json!({
                "sessionId": "uuid1",
                "title": "t",
                "workspaceDirectory": format!("file://{}", self.old),
                "history": []
            }),
        );
        let db = self.ctx.h(".continue/index/index.sqlite");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE tag_catalog (dir TEXT, branch TEXT, \
             artifactId TEXT, path TEXT, cacheKey TEXT);",
        )
        .unwrap();
        con.execute(
            "INSERT INTO tag_catalog VALUES (?,?,?,?,?)",
            rusqlite::params![self.old, "main", "a", "p", "c"],
        )
        .unwrap();
    }

    fn build_pi(&self) {
        self.w(".pi/agent/projects-memory/abc/AGENTS.md", "mem\n");
        self.w(
            ".pi/agent/run-history.jsonl",
            &serde_json::json!({
                "agent": "x", "cwd": self.old, "task": format!("work on {}", self.old)
            })
            .to_string(),
        );
        let enc_old = movara::encodings::pi_bucket(&self.old);
        self.w(
            &format!(".pi/agent/sessions/{}/2026-01-01_uuid7.jsonl", enc_old),
            &format!(
                "{}\n",
                serde_json::json!({
                    "type": "session", "version": 3, "id": "uuid7", "cwd": self.old
                })
            ),
        );
    }

    fn build_droid(&self) {
        self.wj(
            ".factory/sessions/s1.json",
            serde_json::json!({"sessionId": "s1", "cwd": self.old}),
        );
        self.wj(
            ".factory/background-processes.json",
            serde_json::json!({"procs": [{"cwd": self.old}]}),
        );
    }

    fn build_crush(&self) {
        self.wj(
            ".local/share/crush/projects.json",
            serde_json::json!({
                "projects": [
                    {
                        "path": self.old,
                        "data_dir": format!("{}/.crush", self.old),
                        "last_accessed": 1
                    },
                    {
                        "path": "/other",
                        "data_dir": "/other/.crush",
                        "last_accessed": 2
                    }
                ]
            }),
        );
    }

    fn build_ccconnect(&self) {
        self.wj(
            ".cc-connect/dir_history.json",
            serde_json::json!({"sandbox": [self.old], "other": ["/x"]}),
        );
        let h8 = movara::encodings::sha256_8(&self.old);
        self.wj(
            &format!(".cc-connect/sessions/proj_{}.json", h8),
            serde_json::json!({"workDir": self.old}),
        );
    }

    fn build_aider(&self) {
        self.w(".aider.conf.yml", &format!("read: {}/notes.md\n", self.old));
    }

    /// run a full migration over every adapter
    pub fn migrate(&self, deep: bool) -> Backup {
        let spec = ReplaceSpec::new(&self.old, &self.new).unwrap();
        let mut backup = Backup::new(
            &self.tmp.join("backups"),
            &spec,
            vec!["*".to_string()],
            false,
        );
        for a in adapters::all() {
            if a.installed(&self.ctx) {
                a.migrate(&self.ctx, &spec, &mut backup, deep).unwrap();
            }
        }
        backup.save().unwrap();
        backup
    }

    /// boundary-aware grep over the fixture home (backups excluded): the
    /// needle counts only when NOT followed by a name byte (same rule the
    /// engine enforces), so "/a/abc2" does not count for "/a/abc"
    #[allow(clippy::only_used_in_recursion)]
    fn boundary_ok_escaped(raw: &[u8], needle_b: &[u8]) -> bool {
        let mut i = 0;
        while let Some(pos) = memchr::memmem::find(&raw[i..], needle_b) {
            let after = i + pos + needle_b.len();
            match raw.get(after) {
                None => return true,
                Some(b) => {
                    let name = b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.' || *b == b'-';
                    if !name {
                        return true;
                    }
                    i = after;
                }
            }
        }
        false
    }

    pub fn grep(&self, needle: &str) -> Vec<PathBuf> {
        let mut hits = Vec::new();
        self.walk(&self.ctx.home, needle, &mut hits);
        hits
    }

    #[allow(clippy::only_used_in_recursion)]
    fn walk(&self, root: &Path, needle: &str, hits: &mut Vec<PathBuf>) {
        let needle_b = needle.as_bytes();
        // JSON on disk stores Windows paths with doubled backslashes; the
        // raw needle never matches the escaped bytes, so probe both
        let escaped_b = needle.replace('\\', "\\\\").into_bytes();
        let boundary_ok = |raw: &[u8]| -> bool {
            let mut i = 0;
            while let Some(pos) = memchr::memmem::find(&raw[i..], needle_b) {
                let after = i + pos + needle_b.len();
                match raw.get(after) {
                    None => return true,
                    Some(b) => {
                        let name =
                            b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.' || *b == b'-';
                        if !name {
                            return true;
                        }
                        i = after;
                    }
                }
            }
            false
        };
        if root.is_file() {
            if let Ok(raw) = fs::read(root) {
                if boundary_ok(&raw) || Self::boundary_ok_escaped(&raw, &escaped_b) {
                    hits.push(root.to_path_buf());
                }
            }
            return;
        }
        if !root.is_dir() {
            return;
        }
        for entry in fs::read_dir(root).ok().into_iter().flatten() {
            let entry = entry.ok().unwrap();
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "backups" {
                continue;
            }
            self.walk(&p, needle, hits);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

/// boundary-aware containment over raw (possibly binary) bytes: the
/// needle counts only as a whole token, never as a fragment glued to
/// adjacent name bytes — the same discipline the engine enforces.
/// Plain substring checks over SQLite pages false-positive on cell
/// concatenations (a path cell followed by a "2026-..." timestamp
/// contains the literal bytes of "<path>2").
pub fn boundary_contains(raw: &[u8], needle: &str) -> bool {
    let nb = needle.as_bytes();
    let is_name = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-';
    let mut from = 0usize;
    while let Some(pos) = memchr::memmem::find(&raw[from..], nb) {
        let i = from + pos;
        let end = i + nb.len();
        let left_ok = i == 0 || !is_name(raw[i - 1]);
        let right_ok = end >= raw.len() || !is_name(raw[end]);
        if left_ok && right_ok {
            return true;
        }
        from = i + 1;
    }
    false
}

/// goose's data dir relative to the XDG data root — the adapter mirrors
/// goose's etcetera strategy (bundle-id dir on macOS)
pub fn goose_data_rel() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Block.block.goose"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "goose"
    }
}

/// goose's config dir relative to the fixture home (HOME-relative,
/// because the macOS Preferences dir is not under the config root)
pub fn goose_config_rel() -> String {
    #[cfg(target_os = "macos")]
    {
        "Library/Preferences/Block.block.goose".to_string()
    }
    #[cfg(windows)]
    {
        ".config/Block/goose".to_string()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ".config/goose".to_string()
    }
}

/// warp's database path — macOS keeps it under ~/.warp (home root),
/// elsewhere it is the local-data root
pub fn warp_db(ctx: &Ctx) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        ctx.home.join(".warp/warp.db")
    }
    #[cfg(not(target_os = "macos"))]
    {
        ctx.dl("warp/warp.db")
    }
}
