// SPDX-License-Identifier: MIT OR Apache-2.0
//! Trae (ByteDance) — a VS Code fork plus a thin home state root.
//!
//! Verified on a live machine:
//! - IDE: `~/.config/Trae CN/User/` (the CN build carries a space in
//!   the app dir; the international build uses plain `Trae`) —
//!   state.vscdb ItemTable + workspaceStorage/*/workspace.json, same
//!   machinery as every VS Code fork
//! - `~/.trae` (home root): agents/, skills/, mcp.json — agent and
//!   MCP definitions that can embed project paths

use super::{Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::PathBuf;

pub struct TraeAdapter;

impl TraeAdapter {
    /// both builds' IDE config dirs (CN first — observed in the wild)
    fn bases(&self) -> Vec<super::vscode_family::VscodeBase> {
        vec![
            super::vscode_family::VscodeBase {
                app_config_dir: "Trae CN",
            },
            super::vscode_family::VscodeBase {
                app_config_dir: "Trae",
            },
        ]
    }

    fn home_root(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".trae")
    }
}

impl Adapter for TraeAdapter {
    fn name(&self) -> &'static str {
        "trae"
    }
    fn display(&self) -> &'static str {
        "Trae (ByteDance)"
    }
    fn note(&self) -> &'static str {
        "~/.config/Trae CN (or Trae) state.vscdb + workspaceStorage; \
         ~/.trae agents/skills/mcp.json"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.bases().iter().map(|b| b.ide_db(ctx)).collect();
        v.push(self.home_root(ctx));
        v.into_iter().filter(|p| p.exists()).collect()
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![
            crate::ctx::RootKind::Config,
            crate::ctx::RootKind::Config,
            crate::ctx::RootKind::Home,
        ]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["trae", "trae-cn"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        for b in self.bases() {
            out.extend(b.scan_itemtable(ctx, spec, self.name()));
            out.extend(b.scan_workspace_storage(ctx, spec, self.name()));
        }
        out.extend(self.scan_tree(spec, std::slice::from_ref(&self.home_root(ctx))));
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
        for b in self.bases() {
            actions.extend(b.migrate_itemtable(ctx, spec, backup, self.name())?);
            actions.extend(b.migrate_workspace_storage(ctx, spec, backup, self.name())?);
        }
        let roots = [self.home_root(ctx)];
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}
