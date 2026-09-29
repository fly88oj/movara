// SPDX-License-Identifier: MIT OR Apache-2.0
//! OpenCode (anomalyco/opencode, formerly sst/opencode).
//!
//! Layout (verified against the anomalyco/opencode tree HEAD 2026-09):
//! - ~/.local/share/opencode/opencode-<variant>.db (sqlite) — the
//!   channel decides the name: opencode.db (latest/beta/prod),
//!   opencode-stable.db, opencode-local.db; every present variant is
//!   migrated. Windows stores the path columns with forward slashes.
//!   project.worktree / project.sandboxes (JSON abs-path array)
//!   project_directory.directory (composite PK — rowid addressing)
//!   workspace.directory
//!   session.directory, session.path
//!   part.data / session_message.data / event.data / message.data JSON
//!   blobs embed the directory (content layer: --deep only)
//!   project.id derives from the git remote / git root (NOT the path);
//!   a legacy id that literally equals sha256(old path) is re-keyed.
//! - ~/.local/share/opencode/storage/  legacy JSON storage

use super::rewrite_pair;
use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct OpencodeAdapter;

impl OpencodeAdapter {
    /// every installed channel DB (opencode.db, opencode-stable.db, …)
    fn dbs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let dir = ctx.d("opencode");
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("opencode") && name.ends_with(".db") {
                    out.push(e.path());
                }
            }
        }
        out.sort();
        out
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
    fn path_derived_id(&self, db: &Path, spec: &ReplaceSpec) -> Option<String> {
        if !db.is_file() {
            return None;
        }
        let candidate = encodings::sha256_hex(&spec.old);
        let con = sqlite::open_ro(db).ok()?;
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
        "opencode*.db project.worktree/sandboxes, session.directory/path, \
         workspace/project_directory.directory, legacy JSON storage"
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["opencode"]
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.d("opencode")]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![crate::ctx::RootKind::Data]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        for db in self.dbs(ctx) {
            if let Ok(con) = sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                for (label, sql) in [
                    (
                        "project.worktree",
                        "SELECT \"id\" FROM \"project\" \
                      WHERE \"worktree\" LIKE ?",
                    ),
                    (
                        "project.sandboxes",
                        "SELECT \"id\" FROM \"project\" \
                      WHERE \"sandboxes\" LIKE ?",
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
            if let Some(pid) = self.path_derived_id(&db, spec) {
                out.push(Finding {
                    agent: self.name().into(),
                    kind: "info".into(),
                    target: pid,
                    detail: "path-derived project id (legacy) will be re-keyed".into(),
                });
            }
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
        for db in self.dbs(ctx) {
            let legacy_pid = self.path_derived_id(&db, spec);
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"worktree\" FROM \"project\" \
                     WHERE \"worktree\" LIKE ?",
                    "UPDATE \"project\" SET \"worktree\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"sandboxes\" FROM \"project\" \
                     WHERE \"sandboxes\" LIKE ?",
                    "UPDATE \"project\" SET \"sandboxes\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"workspace\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"workspace\" SET \"directory\"=? \
                     WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"session\" SET \"directory\"=? WHERE \"id\"=?",
                )?;
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\",\"path\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    "UPDATE \"session\" SET \"path\"=? WHERE \"id\"=?",
                )?;
                // composite PK (project_id, directory): rowid addressing
                // keeps multi-worktree rows distinct instead of
                // collapsing them onto one value (UNIQUE violation)
                rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT rowid,\"directory\" FROM \"project_directory\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"project_directory\" SET \"directory\"=? \
                     WHERE rowid=?",
                )?;
                // JSON blobs are the content layer: --deep only (tables
                // are optional across opencode versions)
                if deep {
                    let _ = rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT \"id\",\"data\" FROM \"event\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"event\" SET \"data\"=? WHERE \"id\"=?",
                    );
                    let _ = rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT \"id\",\"data\" FROM \"message\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"message\" SET \"data\"=? WHERE \"id\"=?",
                    );
                    let _ = rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"data\" FROM \"part\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"part\" SET \"data\"=? WHERE rowid=?",
                    );
                    let _ = rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"data\" FROM \"session_message\" \
                     WHERE \"data\" LIKE ?",
                        "UPDATE \"session_message\" SET \"data\"=? \
                     WHERE rowid=?",
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
