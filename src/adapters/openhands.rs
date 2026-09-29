// SPDX-License-Identifier: MIT OR Apache-2.0
//! OpenHands (formerly OpenDevin) local state.
//!
//! Layout (source-verified against OpenHands/OpenHands-CLI and the
//! software-agent-sdk): everything under `~/.openhands` (three env
//! vars can relocate it — OPENHANDS_PERSISTENCE_DIR for the CLI,
//! OH_PERSISTENCE_DIR for the SDK — not tracked here):
//! - conversations/<conversation_id>/events/event-*.json — one JSON
//!   event per file; ids are UUIDs, not path-derived
//! - base_state.json / agent settings carry `working_dir` (identity)
//! - projects/<sha256(realpath(cwd))>/prompt_history.json — the
//!   project bucket is sha256 of the RESOLVED absolute path
//!   (realpath — our paths are canonicalized already)
//! - mcp.json, cli_config.json, agent_settings.json, memory/,
//!   skills/ — plain text/JSON
//!
//! No binary formats. Cloud conversation stores are stubs.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::PathBuf;

pub struct OpenHandsAdapter;

impl OpenHandsAdapter {
    fn root(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".openhands")
    }
}

impl Adapter for OpenHandsAdapter {
    fn name(&self) -> &'static str {
        "openhands"
    }
    fn display(&self) -> &'static str {
        "OpenHands (All Hands AI)"
    }
    fn note(&self) -> &'static str {
        "~/.openhands conversations/<uuid>/events + base_state working_dir \
         + projects/<sha256(realpath)>/prompt_history.json"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.root(ctx)]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["openhands"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let projects = self.root(ctx).join("projects");
        let old_h = crate::encodings::sha256_hex(&spec.old);
        let new_h = crate::encodings::sha256_hex(&spec.new);
        if projects.join(&old_h).is_dir() {
            out.push(mk(
                self.name(),
                "dir_rename",
                &projects.join(&old_h),
                &format!("-> {}", &new_h[..16.min(new_h.len())]),
            ));
        }
        out.extend(self.scan_tree(spec, std::slice::from_ref(&self.root(ctx))));
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
        // the projects bucket is sha256 of the resolved absolute path
        let projects = self.root(ctx).join("projects");
        let old_h = crate::encodings::sha256_hex(&spec.old);
        let new_h = crate::encodings::sha256_hex(&spec.new);
        let old_d = projects.join(&old_h);
        let new_d = projects.join(&new_h);
        if old_d.is_dir() && !new_d.exists() && !backup.dry_run {
            backup.record_rename(&old_d, &new_d);
            if std::fs::rename(&old_d, &new_d).is_ok() {
                actions.push(mk(
                    self.name(),
                    "dir_rename",
                    &old_d,
                    &format!("-> {}", new_d.display()),
                ));
            }
        }
        // V1 openhands.db: conversation_metadata.tags embeds the
        // archived workspace path under the 'archiveworkspacepath' key
        let db = self.root(ctx).join("openhands.db");
        if db.is_file() {
            // the tags value JSON-escapes the path (doubled separators
            // on Windows) — pre-filter across every LIKE form
            let n = crate::sqlite::open_ro(&db)
                .ok()
                .map(|con| {
                    spec.like_patterns()
                        .iter()
                        .map(|pat| {
                            super::sqlite_like_count(
                                &con,
                                "SELECT rowid FROM \"conversation_metadata\" \
                                 WHERE \"tags\" LIKE ?",
                                pat,
                            )
                        })
                        .sum()
                })
                .unwrap_or(0);
            if n > 0 {
                backup.record_db(&db)?;
                if !backup.dry_run {
                    let con = crate::sqlite::open_rw(&db)?;
                    super::rewrite_pair(
                        &con,
                        &spec.like_patterns(),
                        spec,
                        "SELECT rowid,\"tags\" FROM \"conversation_metadata\" \
                         WHERE \"tags\" LIKE ?",
                        "UPDATE \"conversation_metadata\" SET \"tags\"=? \
                         WHERE rowid=?",
                    )?;
                }
                actions.push(mk(self.name(), "sqlite", &db, "conversation_metadata.tags"));
            }
        }
        let roots = [self.root(ctx)];
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}
