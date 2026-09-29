// SPDX-License-Identifier: MIT OR Apache-2.0
//! GitHub Copilot CLI local state.
//!
//! Verified against the github/copilot-cli release v1.0.89 app package
//! (2026-09): `~/.copilot` now holds the full session surface —
//! - agents/, hooks/, skills/ — definition layer (identity-aware text
//!   tree)
//! - session-state/<session-id>/ — per-session workspaces (plan.md,
//!   workspace/, checkpoints/, events.jsonl with cwd/gitRoot, session.db
//!   = todos only); rides the text tree (session.db has no path
//!   columns and is skipped by the binary guard)
//! - session-store.db — the chronicle index: sessions.cwd,
//!   session_files.file_path, forge_skill_proposals.git_root_path.
//!   After migration the CLI should be told `/chronicle reindex` if it
//!   reports stale index data.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct CopilotAdapter;

impl CopilotAdapter {
    fn store_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".copilot/session-store.db")
    }
}

impl Adapter for CopilotAdapter {
    fn name(&self) -> &'static str {
        "copilot"
    }
    fn display(&self) -> &'static str {
        "GitHub Copilot CLI"
    }
    fn note(&self) -> &'static str {
        "~/.copilot agents/hooks/skills + session-state/ workspaces + \
         session-store.db (chronicle index) cwd/file_path/git_root_path"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".copilot")]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["copilot"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let db = self.store_db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                for (label, sql) in [
                    (
                        "sessions.cwd",
                        "SELECT \"id\" FROM \"sessions\" WHERE \"cwd\" LIKE ?",
                    ),
                    (
                        "session_files.file_path",
                        "SELECT rowid FROM \"session_files\" \
                         WHERE \"file_path\" LIKE ?",
                    ),
                    (
                        "forge_skill_proposals.git_root_path",
                        "SELECT rowid FROM \"forge_skill_proposals\" \
                         WHERE \"git_root_path\" LIKE ?",
                    ),
                ] {
                    let n = super::sqlite_like_count(&con, sql, &pat);
                    if n > 0 {
                        out.push(Finding {
                            agent: self.name().into(),
                            kind: "sqlite".into(),
                            target: format!("{}::{}", db.display(), label),
                            detail: format!("{} rows", n),
                        });
                    }
                }
            }
        }
        let root = ctx.h(".copilot");
        out.extend(self.scan_tree(spec, std::slice::from_ref(&root)));
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<super::Finding>> {
        let mut actions = Vec::new();
        let db = self.store_db(ctx);
        if db.is_file() {
            let n = sqlite::open_ro(&db)
                .ok()
                .map(|con| {
                    super::sqlite_like_count(
                        &con,
                        "SELECT \"id\" FROM \"sessions\" WHERE \"cwd\" LIKE ?",
                        &spec.like_pattern(),
                    ) + super::sqlite_like_count(
                        &con,
                        "SELECT rowid FROM \"session_files\" \
                         WHERE \"file_path\" LIKE ?",
                        &spec.like_pattern(),
                    ) + super::sqlite_like_count(
                        &con,
                        "SELECT rowid FROM \"forge_skill_proposals\" \
                         WHERE \"git_root_path\" LIKE ?",
                        &spec.like_pattern(),
                    )
                })
                .unwrap_or(0);
            if n > 0 {
                backup.record_db(&db)?;
                if !backup.dry_run {
                    let con = sqlite::open_rw(&db)?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT \"id\",\"cwd\" FROM \"sessions\" \
                         WHERE \"cwd\" LIKE ?",
                        "UPDATE \"sessions\" SET \"cwd\"=? WHERE \"id\"=?",
                    )?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"file_path\" FROM \"session_files\" \
                         WHERE \"file_path\" LIKE ?",
                        "UPDATE \"session_files\" SET \"file_path\"=? \
                         WHERE rowid=?",
                    )?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"git_root_path\" \
                         FROM \"forge_skill_proposals\" \
                         WHERE \"git_root_path\" LIKE ?",
                        "UPDATE \"forge_skill_proposals\" \
                         SET \"git_root_path\"=? WHERE rowid=?",
                    )?;
                }
                actions.push(mk(self.name(), "sqlite", &db, "chronicle index updated"));
                actions.push(Finding {
                    agent: self.name().into(),
                    kind: "hint".into(),
                    target: "/chronicle reindex".into(),
                    detail: "run inside copilot if the session list looks stale".into(),
                });
            }
        }
        let root = ctx.h(".copilot");
        actions.extend(self.migrate_text_tree(spec, backup, &[root], deep)?);
        Ok(actions)
    }
}
