// SPDX-License-Identifier: GPL-3.0-or-later
//! Zed editor agent threads.
//!
//! Layout (verified locally + zed source):
//! - <data>/zed/threads/threads.db
//!   threads(id, ..., folder_paths, folder_paths_order)
//!   folder_paths holds the workspace abs paths (newline-joined in
//!   threads.db, JSON array in the older db/0-stable db)
//! - <data>/zed/db/0-stable/db.sqlite
//!   sidebar_threads.folder_paths / main_worktree_paths (JSON arrays),
//!   trusted_worktrees.absolute_path
//! - thread bodies in `data` blobs are zstd-compressed; content mentions
//!   inside messages are left alone (--deep does not touch compressed
//!   blobs).

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct ZedAdapter;

impl ZedAdapter {
    fn threads_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.d("zed/threads/threads.db")
    }

    fn stable_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.d("zed/db/0-stable/db.sqlite")
    }
}

impl Adapter for ZedAdapter {
    fn name(&self) -> &'static str {
        "zed"
    }
    fn display(&self) -> &'static str {
        "Zed"
    }
    fn note(&self) -> &'static str {
        "zed threads.db threads.folder_paths + db/0-stable \
         sidebar_threads/trusted_worktrees"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.d("zed")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let pat = spec.like_pattern();
        let tdb = self.threads_db(ctx);
        if tdb.is_file() {
            if let Ok(con) = sqlite::open_ro(&tdb) {
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"threads\" \
                     WHERE \"folder_paths\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::threads.folder_paths", tdb.display()),
                        detail: format!("{} rows", n),
                    });
                }
            }
        }
        let sdb = self.stable_db(ctx);
        if sdb.is_file() {
            if let Ok(con) = sqlite::open_ro(&sdb) {
                let mut stmt = match con.prepare(
                    "SELECT \"thread_id\" FROM \"sidebar_threads\" \
                     WHERE \"folder_paths\" LIKE ? \
                     OR \"main_worktree_paths\" LIKE ?",
                ) {
                    Ok(s) => s,
                    Err(_) => return out,
                };
                let n = match stmt.query(rusqlite::params![pat, pat]) {
                    Ok(mut rows) => {
                        let mut n = 0;
                        while let Ok(Some(_)) = rows.next() {
                            n += 1;
                        }
                        n
                    }
                    Err(_) => 0,
                };
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::sidebar_threads", sdb.display()),
                        detail: format!("{} rows", n),
                    });
                }
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"trusted_worktrees\" \
                     WHERE \"absolute_path\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::trusted_worktrees", sdb.display()),
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
        _deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let pat = spec.like_pattern();
        let tdb = self.threads_db(ctx);
        if tdb.is_file() {
            backup.record_db(&tdb)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&tdb)?;
                let total = super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"folder_paths\" FROM \"threads\" \
                     WHERE \"folder_paths\" LIKE ?",
                    "UPDATE \"threads\" SET \"folder_paths\"=? \
                     WHERE \"id\"=?",
                )?;
                if total > 0 {
                    actions.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: tdb.to_string_lossy().into_owned(),
                        detail: format!("{} threads updated", total),
                    });
                }
            }
        }
        let sdb = self.stable_db(ctx);
        if sdb.is_file() {
            backup.record_db(&sdb)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&sdb)?;
                // sidebar_threads carries two path columns; rewrite each
                // column separately via the shared (pk, value) walker
                let mut total = super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"thread_id\",\"folder_paths\" \
                     FROM \"sidebar_threads\" WHERE \"folder_paths\" LIKE ?",
                    "UPDATE \"sidebar_threads\" SET \"folder_paths\"=? \
                     WHERE \"thread_id\"=?",
                )?;
                total += super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"thread_id\",\"main_worktree_paths\" \
                     FROM \"sidebar_threads\" \
                     WHERE \"main_worktree_paths\" LIKE ?",
                    "UPDATE \"sidebar_threads\" SET \"main_worktree_paths\"=? \
                     WHERE \"thread_id\"=?",
                )?;
                total += super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"absolute_path\" \
                     FROM \"trusted_worktrees\" \
                     WHERE \"absolute_path\" LIKE ?",
                    "UPDATE \"trusted_worktrees\" \
                     SET \"absolute_path\"=? WHERE \"id\"=?",
                )?;
                if total > 0 {
                    actions.push(mk(
                        self.name(),
                        "sqlite",
                        &sdb,
                        "sidebar/trusted rows updated",
                    ));
                }
            }
        }
        Ok(actions)
    }
}
