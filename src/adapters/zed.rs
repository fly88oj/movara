// SPDX-License-Identifier: MIT OR Apache-2.0
//! Zed editor agent threads.
//!
//! Layout (verified against zed-industries/zed HEAD 2026-09 and the
//! live local databases):
//! - <data>/zed/threads/threads.db
//!   threads(id, ..., folder_paths, folder_paths_order) — folder_paths
//!   holds newline-joined workspace abs paths; thread bodies are
//!   zstd-compressed blobs left alone
//! - <data>/zed/db/0-<channel>/db.sqlite for every installed channel
//!   (0-stable, 0-preview, 0-nightly, 0-dev, 0-global):
//!   sidebar_threads.folder_paths / main_worktree_paths (Serialized
//!   PathList, newline-joined), trusted_worktrees(trust_id,
//!   absolute_path), workspaces.paths, toolchains.worktree_root_path,
//!   user_toolchains.worktree_root_path, archived_git_worktrees.
//!   worktree_path / main_repo_path

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

    /// db/0-<channel>/db.sqlite for every installed channel
    fn channel_dbs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let root = ctx.d("zed/db");
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let db = e.path().join("db.sqlite");
                if db.is_file() {
                    out.push(db);
                }
            }
        }
        out.sort();
        out
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
        "threads.db folder_paths + db/0-*/db.sqlite sidebar/workspaces/\
         toolchains/trusted worktrees path columns"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.d("zed")]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![crate::ctx::RootKind::Data]
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
        for sdb in self.channel_dbs(ctx) {
            if let Ok(con) = sqlite::open_ro(&sdb) {
                for (table, cols) in [
                    (
                        "sidebar_threads",
                        &["folder_paths", "main_worktree_paths"][..],
                    ),
                    ("workspaces", &["paths"][..]),
                    ("toolchains", &["worktree_root_path"][..]),
                    ("user_toolchains", &["worktree_root_path"][..]),
                    (
                        "archived_git_worktrees",
                        &["worktree_path", "main_repo_path"][..],
                    ),
                    ("trusted_worktrees", &["absolute_path"][..]),
                ] {
                    let mut n = 0usize;
                    for col in cols {
                        n += super::sqlite_like_count(
                            &con,
                            &format!("SELECT rowid FROM \"{}\" WHERE \"{}\" LIKE ?", table, col),
                            &pat,
                        );
                    }
                    if n > 0 {
                        out.push(Finding {
                            agent: self.name().into(),
                            kind: "sqlite".into(),
                            target: format!("{}::{}", sdb.display(), table),
                            detail: format!("{} rows", n),
                        });
                    }
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
        let tdb = self.threads_db(ctx);
        if tdb.is_file() {
            backup.record_db(&tdb)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&tdb)?;
                let total = super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
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
        for sdb in self.channel_dbs(ctx) {
            backup.record_db(&sdb)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&sdb)?;
                let mut total = 0usize;
                // rowid addressing everywhere except the two tables with
                // explicit INTEGER primary keys (thread_id / trust_id)
                for (select, update) in [
                    (
                        "SELECT \"thread_id\",\"folder_paths\" \
                         FROM \"sidebar_threads\" \
                         WHERE \"folder_paths\" LIKE ?",
                        "UPDATE \"sidebar_threads\" SET \"folder_paths\"=? \
                         WHERE \"thread_id\"=?",
                    ),
                    (
                        "SELECT \"thread_id\",\"main_worktree_paths\" \
                         FROM \"sidebar_threads\" \
                         WHERE \"main_worktree_paths\" LIKE ?",
                        "UPDATE \"sidebar_threads\" \
                         SET \"main_worktree_paths\"=? \
                         WHERE \"thread_id\"=?",
                    ),
                    (
                        "SELECT rowid,\"paths\" FROM \"workspaces\" \
                         WHERE \"paths\" LIKE ?",
                        "UPDATE \"workspaces\" SET \"paths\"=? WHERE rowid=?",
                    ),
                    (
                        "SELECT rowid,\"worktree_root_path\" \
                         FROM \"toolchains\" \
                         WHERE \"worktree_root_path\" LIKE ?",
                        "UPDATE \"toolchains\" SET \"worktree_root_path\"=? \
                         WHERE rowid=?",
                    ),
                    (
                        "SELECT rowid,\"worktree_root_path\" \
                         FROM \"user_toolchains\" \
                         WHERE \"worktree_root_path\" LIKE ?",
                        "UPDATE \"user_toolchains\" \
                         SET \"worktree_root_path\"=? WHERE rowid=?",
                    ),
                    (
                        "SELECT rowid,\"worktree_path\" \
                         FROM \"archived_git_worktrees\" \
                         WHERE \"worktree_path\" LIKE ?",
                        "UPDATE \"archived_git_worktrees\" \
                         SET \"worktree_path\"=? WHERE rowid=?",
                    ),
                    (
                        "SELECT rowid,\"main_repo_path\" \
                         FROM \"archived_git_worktrees\" \
                         WHERE \"main_repo_path\" LIKE ?",
                        "UPDATE \"archived_git_worktrees\" \
                         SET \"main_repo_path\"=? WHERE rowid=?",
                    ),
                    (
                        "SELECT \"trust_id\",\"absolute_path\" \
                         FROM \"trusted_worktrees\" \
                         WHERE \"absolute_path\" LIKE ?",
                        "UPDATE \"trusted_worktrees\" \
                         SET \"absolute_path\"=? WHERE \"trust_id\"=?",
                    ),
                ] {
                    let _ = super::rewrite_pair(&con, &spec.like_patterns(), spec, select, update)
                        .map(|n| total += n);
                }
                if total > 0 {
                    actions.push(mk(self.name(), "sqlite", &sdb, "path columns updated"));
                }
            }
        }
        Ok(actions)
    }
}
