// SPDX-License-Identifier: GPL-3.0-or-later
//! ZCode (desktop coding agent).
//!
//! Layout (verified against ZCode 3.10.2 state dirs):
//! - ~/.zcode/cli/db/db.sqlite
//!   session.directory, session.path, workflow_run.cwd
//! - ~/.zcode/cli/agents/sess_<id>/agent_<id>/metadata.json  workspace path
//! - ~/.zcode/cli/exec/<sess>, artifacts/<sess>, rollout/*.jsonl
//! - ~/.zcode/cli/memories/projects/<basename>-<sha256(cwd)[:16]>/

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct ZcodeAdapter;

impl ZcodeAdapter {
    fn db(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".zcode/cli/db/db.sqlite")
    }

    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [
            ".zcode/cli/agents",
            ".zcode/cli/exec",
            ".zcode/cli/artifacts",
            ".zcode/cli/rollout",
            ".zcode/cli/config.json",
        ]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
    }

    fn memories(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".zcode/cli/memories/projects")
    }
}

impl Adapter for ZcodeAdapter {
    fn name(&self) -> &'static str {
        "zcode"
    }
    fn display(&self) -> &'static str {
        "ZCode"
    }
    fn note(&self) -> &'static str {
        "db.sqlite session.directory/path + workflow_run.cwd, \
         memories/projects/<basename>-<sha256(cwd)[:16]>"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".zcode")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let db = self.db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let pat = spec.like_pattern();
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    &pat,
                ) + super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::session", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"workflow_run\" \
                     WHERE \"cwd\" LIKE ?",
                    &pat,
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::workflow_run", db.display()),
                        detail: format!("{} rows", n),
                    });
                }
            }
        }
        let mem = self.memories(ctx);
        let old_key = encodings::zcode_memory_key(&spec.old);
        let old_d = mem.join(&old_key);
        if old_d.is_dir() {
            out.push(Finding {
                agent: self.name().into(),
                kind: "dir_rename".into(),
                target: old_d.to_string_lossy().into_owned(),
                detail: "-> ".to_string() + &encodings::zcode_memory_key(&spec.new),
            });
        }
        out.extend(self.scan_tree(spec, &self.roots(ctx)));
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
            let pat = spec.like_pattern();
            backup.record_db(&db)?;
            let mut updated = 0usize;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                updated += super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"directory\" FROM \"session\" \
                     WHERE \"directory\" LIKE ?",
                    "UPDATE \"session\" SET \"directory\"=? \
                     WHERE \"id\"=?",
                )?;
                updated += super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"path\" FROM \"session\" \
                     WHERE \"path\" LIKE ?",
                    "UPDATE \"session\" SET \"path\"=? WHERE \"id\"=?",
                )?;
                updated += super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"id\",\"cwd\" FROM \"workflow_run\" \
                     WHERE \"cwd\" LIKE ?",
                    "UPDATE \"workflow_run\" SET \"cwd\"=? \
                     WHERE \"id\"=?",
                )?;
            }
            if updated > 0 {
                actions.push(mk(
                    self.name(),
                    "sqlite",
                    &db,
                    &format!("session/workflow_run rows updated: {}", updated),
                ));
            }
        }
        let mem = self.memories(ctx);
        let old_key = encodings::zcode_memory_key(&spec.old);
        let new_key = encodings::zcode_memory_key(&spec.new);
        let old_d = mem.join(&old_key);
        let new_d = mem.join(&new_key);
        if super::rename_dir(&old_d, &new_d, backup) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &old_d,
                &format!("-> {}", new_d.display()),
            ));
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?);
        Ok(actions)
    }
}
