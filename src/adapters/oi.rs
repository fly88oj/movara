// SPDX-License-Identifier: MIT OR Apache-2.0
//! Open Interpreter (the 2026 Rust CLI, repo openinterpreter/
//! openinterpreter — a Codex rebase).
//!
//! Layout (source-verified): `~/.openinterpreter` (INTERPRETER_HOME
//! overrides; CODEX_HOME deliberately ignored so the two products stay
//! isolated) mirrors the Codex layout re-rooted:
//! - sessions/YYYY/MM/DD/rollout-*.jsonl (optionally .zst — skipped,
//!   binary) first line session_meta payload.cwd
//! - archived_sessions/ (flat) same rollout format
//! - config.toml with [projects."<canonical-path>"] trust keys and MCP
//!   server commands
//! - state_*.sqlite `threads.cwd` (a stale cwd silently drops the
//!   session from cwd-filtered lists and resume --last)
//! - memories_*.sqlite / logs_*.sqlite / goals_*.sqlite — schemas not
//!   documented; swept generically (PRAGMA-discovered text columns)
//!
//! session_index.jsonl carries no cwd. No path-derived names.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::rewriters;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct OpenInterpreterAdapter;

const HOME_REL: &str = ".openinterpreter";

impl OpenInterpreterAdapter {
    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [
            format!("{HOME_REL}/sessions"),
            format!("{HOME_REL}/archived_sessions"),
            format!("{HOME_REL}/config.toml"),
        ]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
    }

    /// state dbs with a known threads table, plus the closed-schema
    /// ones that get the generic sweep
    fn all_dbs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let dir = ctx.h(HOME_REL);
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".sqlite") {
                    out.push(e.path());
                }
            }
        }
        out.sort();
        out
    }
}

impl Adapter for OpenInterpreterAdapter {
    fn name(&self) -> &'static str {
        "openinterpreter"
    }
    fn display(&self) -> &'static str {
        "Open Interpreter"
    }
    fn note(&self) -> &'static str {
        "~/.openinterpreter sessions/**/rollout-*.jsonl session_meta \
         payload.cwd, config.toml [projects], state_*.sqlite threads.cwd; \
         memories/logs/goals dbs swept generically"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(HOME_REL)]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["interpreter"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, &self.roots(ctx));
        for db in self.all_dbs(ctx) {
            if let Ok(con) = sqlite::open_ro(&db) {
                // known shape first
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"threads\" WHERE \"cwd\" LIKE ?",
                    &spec.like_pattern(),
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::threads.cwd", db.display()),
                        detail: format!("{} rows", n),
                    });
                    continue;
                }
                // closed-schema dbs: generic text-column probe
                let mut hits = 0usize;
                for (t, c) in super::sqlite_text_columns(&con) {
                    let tq = t.replace('\'', "''");
                    let cq = c.replace('"', "\"\"");
                    let sql = format!("SELECT rowid FROM \"{tq}\" WHERE \"{cq}\" LIKE ?");
                    for p in spec.like_patterns() {
                        hits += super::sqlite_like_count(&con, &sql, p.as_str());
                    }
                }
                if hits > 0 {
                    out.push(mk(
                        self.name(),
                        "db",
                        &db,
                        &format!("{} rows (sweep)", hits),
                    ));
                }
            }
        }
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        for rel in [
            format!("{HOME_REL}/sessions"),
            format!("{HOME_REL}/archived_sessions"),
        ] {
            let root = ctx.h(&rel);
            for f in super::iter_files(&[root], None) {
                // .zst rollouts are compressed — skipped by the jsonl
                // rewriter's utf-8 guard
                if rewriters::rewrite_jsonl_file(&f, spec, backup, deep)? {
                    actions.push(mk(self.name(), "file", &f, "jsonl"));
                }
            }
        }
        let cfg = ctx.h(&format!("{HOME_REL}/config.toml"));
        if cfg.is_file() && rewriters::rewrite_text_file(&cfg, spec, backup)? {
            actions.push(mk(self.name(), "file", &cfg, "text"));
        }
        for db in self.all_dbs(ctx) {
            let has_threads = sqlite::open_ro(&db)
                .ok()
                .map(|con| {
                    super::sqlite_like_count(
                        &con,
                        "SELECT \"id\" FROM \"threads\" WHERE \"cwd\" LIKE ?",
                        &spec.like_pattern(),
                    )
                })
                .unwrap_or(0);
            if has_threads == 0 && !self.db_has_sweep_hits(&db, spec) {
                continue;
            }
            backup.record_db(&db)?;
            if backup.dry_run {
                actions.push(mk(self.name(), "db", &db, "dry-run"));
                continue;
            }
            let con = sqlite::open_rw(&db)?;
            if has_threads > 0 {
                super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"cwd\" FROM \"threads\" WHERE \"cwd\" LIKE ?",
                    "UPDATE \"threads\" SET \"cwd\"=? WHERE \"id\"=?",
                )?;
                actions.push(mk(self.name(), "db", &db, "threads.cwd"));
            } else {
                let n = super::sqlite_text_sweep(&con, spec)?;
                if n > 0 {
                    actions.push(mk(self.name(), "db", &db, &format!("{} rows (sweep)", n)));
                }
            }
        }
        Ok(actions)
    }
}

impl OpenInterpreterAdapter {
    fn db_has_sweep_hits(&self, db: &std::path::Path, spec: &ReplaceSpec) -> bool {
        let Ok(con) = sqlite::open_ro(db) else {
            return false;
        };
        for (t, c) in super::sqlite_text_columns(&con) {
            let tq = t.replace('\'', "''");
            let cq = c.replace('"', "\"\"");
            let sql = format!("SELECT rowid FROM \"{tq}\" WHERE \"{cq}\" LIKE ?");
            for p in spec.like_patterns() {
                if super::sqlite_like_count(&con, &sql, p.as_str()) > 0 {
                    return true;
                }
            }
        }
        false
    }
}
