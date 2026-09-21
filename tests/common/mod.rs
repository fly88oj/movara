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
