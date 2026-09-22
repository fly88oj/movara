// SPDX-License-Identifier: MIT OR Apache-2.0
//! Qoder (Alibaba) and its CN siblings: Tongyi Lingma's migrated
//! qoder-cn layout.
//!
//! Two state surfaces, verified on a live machine:
//! - the IDE is a VS Code fork: `~/.config/Qoder/User/` — state.vscdb
//!   ItemTable (chat/agent state referencing workspace URIs) +
//!   workspaceStorage/*/workspace.json (`folder` file:// URI)
//! - `~/.qoder` (home root; `QODER_CONFIG_DIR` relocates): memories/
//!   <account-hash>/projects/<dash-encoded-path>/** markdown memory
//!   (project linkage is the classic dash encoding — the 8-hex bucket
//!   on top is account-keyed, not path-derived, so it never renames),
//!   mcp.json (server cwd fields), canvas/, plugins/
//! - Lingma (~/.lingma) migrated its CN variant to
//!   `~/.lingma/qoder-cn/` with the same memories layout; the rest of
//!   ~/.lingma (bin/cache/index/extension) is runtime and index
//!   storage — the index is binary and regenerable, left alone

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::PathBuf;

pub struct QoderAdapter;

impl QoderAdapter {
    fn base(&self) -> super::vscode_family::VscodeBase {
        super::vscode_family::VscodeBase {
            app_config_dir: "Qoder",
        }
    }

    /// every memories root with the projects/<dash-encoded> layout
    fn memory_roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for base in [ctx.h(".qoder/memories"), ctx.h(".lingma/qoder-cn/memories")] {
            if !base.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.filter_map(|e| e.ok()) {
                    let projects = e.path().join("projects");
                    if projects.is_dir() {
                        out.push(projects);
                    }
                }
            }
        }
        out
    }

    fn tree_roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        [ctx.h(".qoder"), ctx.h(".lingma")]
            .into_iter()
            .filter(|p| p.is_dir())
            .collect()
    }
}

impl Adapter for QoderAdapter {
    fn name(&self) -> &'static str {
        "qoder"
    }
    fn display(&self) -> &'static str {
        "Qoder / Tongyi Lingma (CN)"
    }
    fn note(&self) -> &'static str {
        "~/.config/Qoder state.vscdb + workspaceStorage; ~/.qoder \
         memories/<account>/projects/<dash-encoded>/ + mcp.json; \
         ~/.lingma/qoder-cn same memories layout"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        // NO existence filtering: root_kinds is a fixed zip-aligned
        // list; the archive layer skips missing paths on its own
        let mut v = vec![self.base().ide_db(ctx)];
        v.push(ctx.h(".qoder"));
        v.push(ctx.h(".lingma"));
        v
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![
            crate::ctx::RootKind::Config,
            crate::ctx::RootKind::Home,
            crate::ctx::RootKind::Home,
        ]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["qoder", "qoder-cn", "lingma"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.base().scan_itemtable(ctx, spec, self.name());
        out.extend(self.base().scan_workspace_storage(ctx, spec, self.name()));
        for projects in self.memory_roots(ctx) {
            out.extend(super::encoded_bucket_findings(
                self.name(),
                &projects,
                &crate::encodings::dash_encode(&spec.old),
                &crate::encodings::dash_encode(&spec.new),
            ));
        }
        out.extend(self.scan_tree(spec, &self.tree_roots(ctx)));
        out
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = self
            .base()
            .migrate_itemtable(ctx, spec, backup, self.name())?;
        actions.extend(
            self.base()
                .migrate_workspace_storage(ctx, spec, backup, self.name())?,
        );
        for projects in self.memory_roots(ctx) {
            for (o, n) in super::rename_encoded_children(
                &projects,
                &crate::encodings::dash_encode(&spec.old),
                &crate::encodings::dash_encode(&spec.new),
                backup,
            ) {
                actions.push(mk(
                    self.name(),
                    "dir_rename",
                    &o,
                    &format!("-> {}", n.display()),
                ));
            }
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.tree_roots(ctx), deep)?);
        Ok(actions)
    }
}
