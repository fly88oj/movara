// SPDX-License-Identifier: GPL-3.0-or-later
//! VS Code fork IDEs: Cursor, Windsurf, Antigravity (Google).
//!
//! Common layout (VS Code forks keep chat data in globalStorage):
//! - <config>/<App>/User/globalStorage/state.vscdb
//!   ItemTable(key, value)       JSON blobs referencing workspace URIs;
//!   windsurf's "codeium.windsurf" key holds
//!   windsurf.workspaceCascadeMap
//!   {file:///<path>: <cascade-uuid>}
//!   cursorDiskKV(key, value)    Cursor's chat KV store (composerData:/
//!   bubbleId: rows carry fsPath + file://
//!   URIs)
//! - <config>/<App>/User/workspaceStorage/<opaque-id>/workspace.json
//!   {"folder": "file:///abs/path"}   -- content rewritten; the dir name
//!   is an undocumented VS Code id, so the dir itself is left alone.
//!
//! Agent-specific extras:
//! - Cursor CLI: ~/.cursor/projects/<dash-encoded-without-leading-dash>/
//! - Windsurf: ~/.codeium/windsurf (cascade/*.pb -- encrypted in current
//!   versions, brain/, memories/, codemaps/*.json) and hashed dirs
//!   context_state|database/<md5(path without file:// scheme)>/
//! - Antigravity: ~/.gemini/antigravity (codeium-style layout)

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::spec::ReplaceSpec;
use crate::sqlite;
use anyhow::Result;
use std::path::{Path, PathBuf};

fn codeium_roots(root: &Path) -> Vec<PathBuf> {
    ["cascade", "brain", "memories", "workflows", "implicit"]
        .iter()
        .map(|rel| root.join(rel))
        .filter(|p| p.is_dir())
        .collect()
}

fn codeium_text_roots(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["codemaps", "context_state", "code_tracker", "database"]
        .iter()
        .map(|rel| root.join(rel))
        .filter(|p| p.is_dir())
        .collect();
    let mj = root.join("mcp_config.json");
    if mj.is_file() {
        out.push(mj);
    }
    out
}

fn codeium_hashed_dirs(root: &Path) -> Vec<PathBuf> {
    ["context_state", "database"]
        .iter()
        .map(|rel| root.join(rel))
        .filter(|p| p.is_dir())
        .collect()
}

/// rename parent/md5(old) -> parent/md5(new) (windsurf hashed dirs)
fn md5_dir_renamed(
    parent: &Path,
    old: &str,
    new: &str,
    backup: &mut Backup,
) -> Option<(PathBuf, PathBuf)> {
    let old_h = encodings::md5_hex(old);
    let new_h = encodings::md5_hex(new);
    if old_h == new_h {
        return None;
    }
    let old_d = parent.join(old_h);
    let new_d = parent.join(new_h);
    if super::rename_dir(&old_d, &new_d, backup) {
        Some((old_d, new_d))
    } else {
        None
    }
}

struct VscodeBase {
    app_config_dir: &'static str,
}

impl VscodeBase {
    fn ide_db(&self, ctx: &Ctx) -> PathBuf {
        ctx.c(&format!(
            "{}/User/globalStorage/state.vscdb",
            self.app_config_dir
        ))
    }

    fn scan_itemtable(&self, ctx: &Ctx, spec: &ReplaceSpec, label: &str) -> Vec<Finding> {
        let db = self.ide_db(ctx);
        let mut out = Vec::new();
        if !db.is_file() {
            return out;
        }
        if let Ok(con) = sqlite::open_ro(&db) {
            let n = super::sqlite_like_count(
                &con,
                "SELECT \"key\" FROM \"ItemTable\" WHERE \"value\" LIKE ?",
                &spec.like_pattern(),
            );
            if n > 0 {
                out.push(Finding {
                    agent: label.to_string(),
                    kind: "sqlite".into(),
                    target: format!("{}::ItemTable", db.display()),
                    detail: format!("{} keys", n),
                });
            }
        }
        out
    }

    fn migrate_itemtable(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        label: &str,
    ) -> Result<Vec<Finding>> {
        let db = self.ide_db(ctx);
        let mut actions = Vec::new();
        if !db.is_file() {
            return Ok(actions);
        }
        let pat = spec.like_pattern();
        backup.record_db(&db)?;
        if backup.dry_run {
            return Ok(self.scan_itemtable(ctx, spec, label));
        }
        let con = sqlite::open_rw(&db)?;
        let total = super::rewrite_pair(
            &con,
            &pat,
            spec,
            "SELECT \"key\",\"value\" FROM \"ItemTable\" \
             WHERE \"value\" LIKE ?",
            "UPDATE \"ItemTable\" SET \"value\"=? WHERE \"key\"=?",
        )?;
        if total > 0 {
            actions.push(Finding {
                agent: label.to_string(),
                kind: "sqlite".into(),
                target: db.to_string_lossy().into_owned(),
                detail: format!("{} ItemTable rows updated", total),
            });
        }
        Ok(actions)
    }

    fn scan_workspace_storage(&self, ctx: &Ctx, spec: &ReplaceSpec, label: &str) -> Vec<Finding> {
        let ws = ctx.c(&format!("{}/User/workspaceStorage", self.app_config_dir));
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&ws) {
            for e in entries.filter_map(|e| e.ok()) {
                let wj = e.path().join("workspace.json");
                if wj.is_file() {
                    if let Ok(raw) = std::fs::read(&wj) {
                        if spec.maybe_contains(&raw) {
                            out.push(Finding {
                                agent: label.to_string(),
                                kind: "file".into(),
                                target: wj.to_string_lossy().into_owned(),
                                detail: "workspace.json".into(),
                            });
                        }
                    }
                }
            }
        }
        out
    }

    fn migrate_workspace_storage(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        label: &str,
    ) -> Result<Vec<Finding>> {
        let ws = ctx.c(&format!("{}/User/workspaceStorage", self.app_config_dir));
        let mut actions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&ws) {
            for e in entries.filter_map(|e| e.ok()) {
                let wj = e.path().join("workspace.json");
                if wj.is_file() && crate::rewriters::rewrite_text_file(&wj, spec, backup)? {
                    actions.push(mk(label, "file", &wj, "workspace.json"));
                }
            }
        }
        Ok(actions)
    }
}

pub struct CursorAdapter;
pub struct WindsurfAdapter;
pub struct AntigravityAdapter;

impl CursorAdapter {
    fn base(&self) -> VscodeBase {
        VscodeBase {
            app_config_dir: "Cursor",
        }
    }
}

impl Adapter for CursorAdapter {
    fn name(&self) -> &'static str {
        "cursor"
    }
    fn display(&self) -> &'static str {
        "Cursor (IDE + CLI)"
    }
    fn note(&self) -> &'static str {
        "~/.cursor/projects/<dash-encoded>/ (CLI), globalStorage \
         state.vscdb ItemTable + cursorDiskKV (composerData/bubbleId \
         carry fsPath + file:// URIs), workspaceStorage/*/workspace.json"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.base().ide_db(ctx), ctx.h(".cursor")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.base().scan_itemtable(ctx, spec, self.name());
        // cursorDiskKV
        let db = self.base().ide_db(ctx);
        if db.is_file() {
            if let Ok(con) = sqlite::open_ro(&db) {
                let n = super::sqlite_like_count(
                    &con,
                    "SELECT \"key\" FROM \"cursorDiskKV\" \
                     WHERE \"value\" LIKE ?",
                    &spec.like_pattern(),
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::cursorDiskKV", db.display()),
                        detail: format!("{} keys", n),
                    });
                }
            }
        }
        let projects = ctx.h(".cursor/projects");
        let (old_enc, new_enc) = (
            encodings::dash_encode_nolead(&spec.old),
            encodings::dash_encode_nolead(&spec.new),
        );
        out.extend(super::encoded_bucket_findings(
            self.name(),
            &projects,
            &old_enc,
            &new_enc,
        ));
        out.extend(self.base().scan_workspace_storage(ctx, spec, self.name()));
        let roots: Vec<PathBuf> = [".cursor/mcp.json", ".cursor/argv.json"]
            .iter()
            .map(|rel| ctx.h(rel))
            .filter(|p| p.exists())
            .collect();
        out.extend(self.scan_tree(spec, &roots));
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
        // cursorDiskKV
        let db = self.base().ide_db(ctx);
        if db.is_file() {
            let pat = spec.like_pattern();
            backup.record_db(&db)?;
            if !backup.dry_run {
                let con = sqlite::open_rw(&db)?;
                let total = super::rewrite_pair(
                    &con,
                    &pat,
                    spec,
                    "SELECT \"key\",\"value\" FROM \"cursorDiskKV\" \
                     WHERE \"value\" LIKE ?",
                    "UPDATE \"cursorDiskKV\" SET \"value\"=? \
                     WHERE \"key\"=?",
                )?;
                if total > 0 {
                    actions.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: db.to_string_lossy().into_owned(),
                        detail: format!("{} cursorDiskKV rows updated", total),
                    });
                }
            }
        }
        // CLI project dirs
        let projects = ctx.h(".cursor/projects");
        let (old_enc, new_enc) = (
            encodings::dash_encode_nolead(&spec.old),
            encodings::dash_encode_nolead(&spec.new),
        );
        for (o, n) in super::rename_encoded_children(&projects, &old_enc, &new_enc, backup) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &o,
                &format!("-> {}", n.display()),
            ));
        }
        actions.extend(
            self.base()
                .migrate_workspace_storage(ctx, spec, backup, self.name())?,
        );
        let roots: Vec<PathBuf> = [".cursor/projects", ".cursor/mcp.json", ".cursor/argv.json"]
            .iter()
            .map(|rel| ctx.h(rel))
            .filter(|p| p.exists())
            .collect();
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}

struct CodeiumStyle {
    /// path of the codeium-style state root, resolved per ctx
    root_fn: fn(&Ctx) -> PathBuf,
    name: &'static str,
}

impl CodeiumStyle {
    fn scan_all(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let root = (self.root_fn)(ctx);
        let mut out = Vec::new();
        for f in super::iter_files(&codeium_roots(&root), Some(&["pb"])) {
            if let Ok(raw) = std::fs::read(&f) {
                if spec.maybe_contains(&raw) {
                    out.push(Finding {
                        agent: self.name.to_string(),
                        kind: "protobuf".into(),
                        target: f.to_string_lossy().into_owned(),
                        detail: format!("{} bytes", raw.len()),
                    });
                }
            }
        }
        for f in super::iter_files(&codeium_text_roots(&root), None) {
            if let Ok(raw) = std::fs::read(&f) {
                if spec.maybe_contains(&raw) {
                    out.push(Finding {
                        agent: self.name.to_string(),
                        kind: "file".into(),
                        target: f.to_string_lossy().into_owned(),
                        detail: format!("{} bytes", raw.len()),
                    });
                }
            }
        }
        out
    }

    fn migrate_all(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let root = (self.root_fn)(ctx);
        let mut actions = Vec::new();
        for f in super::iter_files(&codeium_roots(&root), Some(&["pb"])) {
            if super::rewrite_pb_file(&f, spec, backup)? {
                actions.push(mk(self.name, "protobuf", &f, "pb"));
            }
        }
        for f in super::iter_files(&codeium_text_roots(&root), None) {
            // agent-owned state roots: any file may carry the path
            if let Some(finding) =
                super::rewrite_file_by_ext(self.name, &f, spec, backup, deep, true)?
            {
                actions.push(finding);
            }
        }
        for parent in codeium_hashed_dirs(&root) {
            if let Some((o, n)) = md5_dir_renamed(&parent, &spec.old, &spec.new, backup) {
                actions.push(mk(
                    self.name,
                    "dir_rename",
                    &o,
                    &format!("-> {} (md5 hashed dir)", n.display()),
                ));
            }
        }
        Ok(actions)
    }
}

macro_rules! codeium_adapter {
    ($t:ty, $app:expr, $name:expr, $display:expr, $note:expr, $root_fn:expr) => {
        impl $t {
            fn base(&self) -> VscodeBase {
                VscodeBase {
                    app_config_dir: $app,
                }
            }
            fn codeium(&self) -> CodeiumStyle {
                CodeiumStyle {
                    root_fn: $root_fn,
                    name: $name,
                }
            }
        }

        impl Adapter for $t {
            fn name(&self) -> &'static str {
                $name
            }
            fn display(&self) -> &'static str {
                $display
            }
            fn note(&self) -> &'static str {
                $note
            }

            fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
                vec![self.base().ide_db(ctx), (self.codeium().root_fn)(ctx)]
            }

            fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
                let mut out = self.base().scan_itemtable(ctx, spec, $name);
                out.extend(self.base().scan_workspace_storage(ctx, spec, $name));
                out.extend(self.codeium().scan_all(ctx, spec));
                out
            }

            fn migrate(
                &self,
                ctx: &Ctx,
                spec: &ReplaceSpec,
                backup: &mut Backup,
                deep: bool,
            ) -> Result<Vec<Finding>> {
                let mut actions = self.base().migrate_itemtable(ctx, spec, backup, $name)?;
                actions.extend(
                    self.base()
                        .migrate_workspace_storage(ctx, spec, backup, $name)?,
                );
                actions.extend(self.codeium().migrate_all(ctx, spec, backup, deep)?);
                Ok(actions)
            }
        }
    };
}

fn windsurf_root(ctx: &Ctx) -> PathBuf {
    ctx.h(".codeium/windsurf")
}

fn antigravity_root(ctx: &Ctx) -> PathBuf {
    ctx.h(".gemini/antigravity")
}

codeium_adapter!(
    WindsurfAdapter,
    "Windsurf",
    "windsurf",
    "Windsurf",
    "~/.codeium/windsurf (cascade/*.pb, context_state/database \
     <md5(path)>), IDE state.vscdb ItemTable codeium.windsurf \
     workspaceCascadeMap",
    windsurf_root
);

codeium_adapter!(
    AntigravityAdapter,
    "Antigravity",
    "antigravity",
    "Google Antigravity",
    "~/.config/Antigravity globalStorage state.vscdb + \
     ~/.gemini/antigravity (codeium-style state)",
    antigravity_root
);
