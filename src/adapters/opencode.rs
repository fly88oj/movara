// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenCode (sst/opencode).
//!
//! Layout (verified against OpenCode storage schema v1.1.36):
//! - ~/.local/share/opencode/opencode.db (sqlite)
//!   project.worktree          abs directory
//!   project_directory.directory
//!   workspace.directory
//!   session.directory, session.path
//!   event.data / message.data JSON blobs embed the directory (content
//!   layer: only rewritten with --deep)
//!   project.id is NOT a hash of the path on this install (derived from the
//!   git remote); legacy path-derived ids (sha256(path)) are re-keyed.
//! - ~/.local/share/opencode/storage/  legacy JSON storage

use super::rewrite_pair;
use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct OpencodeAdapter;

impl OpencodeAdapter {
    fn db(&self, ctx: &Ctx) -> PathBuf {
        ctx.d("opencode/opencode.db")
    }

    fn storage(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let s = ctx.d("opencode/storage");
        if s.exists() {
            vec![s]
        } else {
            Vec::new()
        }
    }

    /// legacy path-derived project id (sha256 of the old path)
    fn path_derived_id(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Option<String> {
        let db = self.db(ctx);
        if !db.is_file() {
            return None;
        }
        let candidate = encodings::sha256_hex(&spec.old);
        let con = sqlite::open_ro(&db).ok()?;
        let mut stmt = con
            .prepare("SELECT \"id\",\"worktree\" FROM \"project\"")
            .ok()?;
        let it = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .ok()?;
        for row in it.filter_map(|x| x.ok()) {
            let (id, worktree) = row;
            if id == candidate && worktree.contains(&spec.old) {
                return Some(id);
            }
        }
        None
    }
}

impl Adapter for OpencodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }
    fn display(&self) -> &'static str {
        "OpenCode"
    }
    fn note(&self) -> &'static str {
        "opencode.db project.worktree / session.directory / \
         workspace.directory / project_directory.directory"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.d("opencode")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let db = self.db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                for (label, sql) in [
                    (
                        "project.worktree",
                        "SELECT \"id\" FROM \"project\" \
                      WHERE \"worktree\" LIKE ?",
                    ),
                    (
                        "workspace.directory",
                        "SELECT \"id\" FROM \"workspace\" \
                      WHERE \"directory\" LIKE ?",
                    ),
                    (
                        "session.directory",
                        "SELECT \"id\" FROM \"session\" \
                      WHERE \"directory\" LIKE ?",
                    ),
                    (
                        "session.path",
                        "SELECT \"id\" FROM \"session\" \
                      WHERE \"path\" LIKE ?",
                    ),
                    (
                        "project_directory.directory",
                        "SELECT \"project_id\" FROM \"project_directory\" \
                      WHERE \"directory\" LIKE ?",
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
        if let Some(pid) = self.path_derived_id(ctx, spec) {
            out.push(Finding {
                agent: self.name().into(),
                kind: "info".into(),
                target: pid,
                detail: "path-derived project id (legacy) will be re-keyed".into(),
            });
        }
        out.extend(self.scan_tree(spec, &self.storage(ctx)));
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
        let db = self.db(ctx);
        if db.is_file() {
            let legacy_pid = self.path_derived_id(ctx, spec);
            let pat = spec.like_pattern();
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"worktree\" FROM \"project\" \
                     WHERE \"worktree\" LIKE ?",
                    "UPDATE \"project\" SET \"worktree\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"workspace\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"workspace\" SET \"directory\"=? \
                     WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"session\" SET \"directory\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"path\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    "UPDATE \"session\" SET \"path\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"project_id\",\"directory\" \
                     FROM \"project_directory\" WHERE \"directory\" LIKE ?",
                    "UPDATE \"project_directory\" SET \"directory\"=? \
                     WHERE \"project_id\"=?",
                )?;
                // event/message JSON blobs are the content layer: --deep
                // only (tables are optional across opencode versions)
                if deep {
                    let _ = rewrite_pair(
                        &con,
                        &pat,
                        spec,
                        "SELECT \"id\",\"data\" FROM \"event\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"event\" SET \"data\"=? WHERE \"id\"=?",
                    );
                    let _ = rewrite_pair(
                        &con,
                        &pat,
                        spec,
                        "SELECT \"id\",\"data\" FROM \"message\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"message\" SET \"data\"=? WHERE \"id\"=?",
                    );
                }
                // purge replaced strings left in free pages
                let _ = sqlite::vacuum(&con);

                if let Some(old_pid) = legacy_pid {
                    let new_pid = encodings::sha256_hex(&spec.new);
                    con.execute(
                        "UPDATE \"project\" SET \"id\"=? WHERE \"id\"=?",
                        rusqlite::params![new_pid, old_pid],
                    )?;
                    con.execute(
                        "UPDATE \"session\" SET \"project_id\"=? \
                         WHERE \"project_id\"=?",
                        rusqlite::params![new_pid, old_pid],
                    )?;
                    con.execute(
                        "UPDATE \"workspace\" SET \"project_id\"=? \
                         WHERE \"project_id\"=?",
                        rusqlite::params![new_pid, old_pid],
                    )?;
                    con.execute(
                        "UPDATE \"project_directory\" SET \"project_id\"=? \
                         WHERE \"project_id\"=?",
                        rusqlite::params![new_pid, old_pid],
                    )?;
                    actions.push(Finding {
                        agent: self.name().into(),
                        kind: "info".into(),
                        target: old_pid,
                        detail: format!("re-keyed legacy project id -> {}", new_pid),
                    });
                }
            }
            actions.push(mk(self.name(), "sqlite", &db, "directory columns updated"));
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.storage(ctx), deep)?);
        Ok(actions)
    }
}
