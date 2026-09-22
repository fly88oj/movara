// SPDX-License-Identifier: MIT OR Apache-2.0
//! Warp terminal agent state.
//!
//! Layout (documented locations; the schema itself is closed):
//! - Linux `~/.local/share/warp/`, macOS `~/.warp/`, Windows
//!   `%LOCALAPPDATA%\warp\` — SQLite `warp.db` holds sessions and
//!   agent runs; data dir also holds settings/config files that can
//!   reference workspace paths
//!
//! The warp.db schema is undocumented, so the adapter uses a GENERIC
//! text-column sweep: every table's TEXT columns are discovered via
//! PRAGMA table_info, matched with the boundary-aware LIKE patterns
//! and rewritten row-by-row with bound parameters. Every write is
//! journaled (record_db checkpoints WAL and copies the whole file), so
//! the run is fully reversible; columns that do not reference the path
//! are never touched. Verified against a synthetic warp-shaped db —
//! no live sample was available (documented in docs/research.md).

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct WarpAdapter;

impl WarpAdapter {
    fn data_dir(&self, ctx: &Ctx) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            ctx.home.join(".warp")
        }
        #[cfg(not(target_os = "macos"))]
        {
            ctx.dl("warp")
        }
    }

    fn db(&self, ctx: &Ctx) -> PathBuf {
        self.data_dir(ctx).join("warp.db")
    }
}

impl Adapter for WarpAdapter {
    fn name(&self) -> &'static str {
        "warp"
    }
    fn display(&self) -> &'static str {
        "Warp"
    }
    fn note(&self) -> &'static str {
        "~/.local/share/warp/warp.db — closed schema, generic text-column \
         sweep over PRAGMA-discovered tables"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.data_dir(ctx)]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        #[cfg(target_os = "macos")]
        {
            vec![crate::ctx::RootKind::Home]
        }
        #[cfg(not(target_os = "macos"))]
        {
            vec![crate::ctx::RootKind::DataLocal]
        }
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["warp", "warp-terminal"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let db = self.db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let hits = super::sqlite_text_hit_count(&con, spec);
                if hits > 0 {
                    out.push(mk(self.name(), "db", &db, &format!("{} rows", hits)));
                }
            }
        }
        out.extend(self.scan_tree(spec, std::slice::from_ref(&self.data_dir(ctx))));
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
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                let total = super::sqlite_text_sweep(&con, spec)?;
                if total > 0 {
                    actions.push(mk(self.name(), "db", &db, &format!("{} rows", total)));
                }
            }
        }
        let roots = [self.data_dir(ctx)];
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}
