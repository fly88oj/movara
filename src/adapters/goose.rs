// SPDX-License-Identifier: MIT OR Apache-2.0
//! Goose (Block) agent state.
//!
//! Layout (source-verified against block/goose, crates/goose/src/config/
//! paths.rs + session/session_manager.rs — etcetera's per-OS strategy):
//! - data dir: `~/.local/share/goose` (Linux), `~/Library/Application
//!   Support/Block.block.goose` (macOS), `%APPDATA%\Block\goose\data`
//!   (Windows); `GOOSE_PATH_ROOT` relocates everything
//!   - `sessions/sessions.db` — SQLite (WAL), schema v9: sessions(
//!     id, ... working_dir TEXT NOT NULL ...), messages(...)
//!   - `sessions/*.jsonl` — legacy flat sessions (pre-db releases):
//!     first line is session metadata carrying `working_dir`
//! - config dir: `~/.config/goose` (Linux), `~/Library/Preferences/
//!   Block.block.goose` (macOS), `%APPDATA%\Block\goose\config`
//!   (Windows) — config.yaml, permissions/tool_permissions.json
//!
//! No path-derived bucket names: sessions are keyed by date ids
//! (`YYYYMMDD_N`); the only path carrier is the `working_dir` column /
//! metadata key plus content mentions (rewritten with --deep).

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::PathBuf;

pub struct GooseAdapter;

impl GooseAdapter {
    fn data_dir(&self, ctx: &Ctx) -> PathBuf {
        // etcetera Apple strategy keys the data dir by bundle id; the
        // Windows strategy nests author/app under the config root and
        // puts data there too (%APPDATA%\Block\goose\data — the data
        // ROOT itself, not a data-local sibling)
        #[cfg(target_os = "macos")]
        {
            ctx.d("Block.block.goose")
        }
        #[cfg(windows)]
        {
            ctx.c("Block").join("goose").join("data")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            ctx.d("goose")
        }
    }

    fn config_dir(&self, ctx: &Ctx) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            ctx.home
                .join("Library")
                .join("Preferences")
                .join("Block.block.goose")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            ctx.c("goose")
        }
        #[cfg(windows)]
        {
            ctx.c("Block").join("goose")
        }
    }

    fn roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let mut v = vec![self.data_dir(ctx), self.config_dir(ctx)];
        v.retain(|p| p.exists());
        v
    }
}

impl Adapter for GooseAdapter {
    fn name(&self) -> &'static str {
        "goose"
    }
    fn display(&self) -> &'static str {
        "Goose (Block)"
    }
    fn note(&self) -> &'static str {
        "~/.local/share/goose/sessions/sessions.db sessions.working_dir \
         + legacy *.jsonl working_dir + config/permissions"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.data_dir(ctx), self.config_dir(ctx)]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        // aligned with state_paths: data dir, then config dir (the
        // macOS Preferences dir has no managed root — Home segment;
        // on Windows BOTH live under the %APPDATA% config root)
        #[cfg(target_os = "macos")]
        {
            vec![crate::ctx::RootKind::Data, crate::ctx::RootKind::Home]
        }
        #[cfg(windows)]
        {
            vec![crate::ctx::RootKind::Config, crate::ctx::RootKind::Config]
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            vec![crate::ctx::RootKind::Data, crate::ctx::RootKind::Config]
        }
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["goose"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let db = self.data_dir(ctx).join("sessions/sessions.db");
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let sql = "SELECT \"id\" FROM \"sessions\" WHERE \"working_dir\" LIKE ?";
                let n: usize = spec
                    .like_patterns()
                    .iter()
                    .map(|p| super::sqlite_like_count(&con, sql, p))
                    .sum();
                if n > 0 {
                    out.push(mk(
                        self.name(),
                        "db",
                        &db,
                        &format!("{} sessions.working_dir", n),
                    ));
                }
            }
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
        let db = self.data_dir(ctx).join("sessions/sessions.db");
        if db.is_file() {
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                let n = super::rewrite_pair(
                    &con,
                    &spec.like_patterns(),
                    spec,
                    "SELECT \"id\", \"working_dir\" FROM \"sessions\" WHERE \"working_dir\" LIKE ?",
                    "UPDATE \"sessions\" SET \"working_dir\" = ? WHERE \"id\" = ?",
                )?;
                if n > 0 {
                    actions.push(mk(
                        self.name(),
                        "db",
                        &db,
                        &format!("{} sessions.working_dir", n),
                    ));
                }
            }
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.roots(ctx), deep)?);
        Ok(actions)
    }
}
