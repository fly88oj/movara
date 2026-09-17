// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenAI Codex CLI.
//!
//! Layout (verified locally on 0.122 + docs for 0.147):
//! - ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl first line session_meta
//!   payload.cwd; turn records may also carry cwd fields
//! - ~/.codex/archived_sessions/  same rollout format
//! - ~/.codex/history.jsonl  {session_id, ts, text} (no path; text may
//!   mention paths -> --deep)
//! - ~/.codex/config.toml  mcp server commands and [projects."<path>"]
//!   trust entries embed absolute paths
//! - ~/.codex/state_*.sqlite (0.147+)  threads.cwd / rollout_path

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::rewriters;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct CodexAdapter;

impl CodexAdapter {
    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [
            ".codex/sessions",
            ".codex/archived_sessions",
            ".codex/config.toml",
            ".codex/history.jsonl",
            ".codex/hooks.json",
        ]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
    }

    fn state_dbs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let dir = ctx.h(".codex");
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("state_") && name.ends_with(".sqlite") {
                    out.push(e.path());
                }
            }
        }
        out.sort();
        out
    }
}

impl Adapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }
    fn display(&self) -> &'static str {
        "OpenAI Codex CLI"
    }
    fn note(&self) -> &'static str {
        "~/.codex/sessions/**/rollout-*.jsonl session_meta payload.cwd, \
         archived_sessions/, config.toml [projects], state_*.sqlite \
         threads.cwd (0.147+)"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".codex")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, &self.roots(ctx));
        for db in self.state_dbs(ctx) {
            if let Ok(con) = sqlite::open_ro(&db) {
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
            ".codex/sessions",
            ".codex/archived_sessions",
            ".codex/history.jsonl",
        ] {
            let root = ctx.h(rel);
            for f in super::iter_files(&[root], None) {
                if rewriters::rewrite_jsonl_file(&f, spec, backup, deep)? {
                    actions.push(mk(self.name(), "file", &f, "jsonl"));
                }
            }
        }
        for rel in [".codex/config.toml", ".codex/hooks.json"] {
            let p = ctx.h(rel);
            if p.is_file() && rewriters::rewrite_text_file(&p, spec, backup)? {
                actions.push(mk(self.name(), "file", &p, "text"));
            }
        }
        for db in self.state_dbs(ctx) {
            let n = sqlite::open_ro(&db)
                .ok()
                .map(|con| {
                    super::sqlite_like_count(
                        &con,
                        "SELECT \"id\" FROM \"threads\" \
                         WHERE \"cwd\" LIKE ?",
                        &spec.like_pattern(),
                    )
                })
                .unwrap_or(0);
            if n == 0 {
                continue;
            }
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                super::rewrite_pair(
                    &con,
                    &spec.like_pattern(),
                    spec,
                    "SELECT \"id\",\"cwd\" FROM \"threads\" \
                     WHERE \"cwd\" LIKE ?",
                    "UPDATE \"threads\" SET \"cwd\"=? WHERE \"id\"=?",
                )?;
            }
            actions.push(Finding {
                agent: self.name().into(),
                kind: "sqlite".into(),
                target: format!("{}::threads", db.display()),
                detail: format!("{} rows updated (cwd)", n),
            });
        }
        Ok(actions)
    }
}
