// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cline family: Cline, Roo Code, Kilo Code (classic) — VS Code
//! extension state, verified against cline/cline, RooCodeInc/Roo-Code
//! and the surviving classic Kilo source.
//!
//! The three extensions share the task-file format and live in the
//! IDE's globalStorage, one dir per marketplace id:
//!   saoudrizwan.claude-dev | rooveterinaryinc.roo-cline |
//!   kilocode.kilo-code
//! under `<config>/<IDE>/User/globalStorage/` for each IDE they run in
//! (Code, Cursor, Windsurf, VSCodium) and under
//! `~/.vscode-server/data/User/globalStorage` for remote sessions.
//!
//! Path carriers:
//! - tasks/<id>/{api_conversation_history,ui_messages,task_metadata,
//!   context_history}.json — tool paths (identity key "path"),
//!   HistoryItem workspace fields
//! - task history INDEX: Roo keeps tasks/_index.json + per-task
//!   history_item.json (`workspace`); Cline 4.x keeps
//!   state/taskHistory.json (`cwdOnTaskInitialization`, legacy
//!   `shadowGitConfigWorkTree`); Cline <=3.x and classic Kilo keep the
//!   array in the IDE's state.vscdb ItemTable under the extension's
//!   key — rewritten row-scoped to the three keys
//! - checkpoints: Cline <=3.x `checkpoints/<cwdHash>/` (polynomial
//!   hash x31 of the cwd, decimal) with a shadow git whose
//!   `.git/config core.worktree` MUST follow the move or the extension
//!   refuses to resume ("Checkpoints can only be used in the original
//!   workspace"); Roo/Kilo keep shadow git under tasks/<id>/checkpoints
//!   (core.worktree rewrite only); classic Kilo also has legacy
//!   `checkpoints/<sha256(cwd)[:8]>/`
//! - classic Kilo `sessions/<sha256(cwd)[:16]>/session.json`
//! - Roo `roo-index-cache-<sha256(cwd)>.json` — a regenerable index,
//!   removed on migration
//!
//! .git directories are skipped by the shared tree walker, so every
//! shadow git's config is rewritten explicitly. The bare decimal
//! cwdHash is deliberately NOT a text needle (any number would match);
//! it is handled as an exact-name directory rename only.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::PathBuf;

pub struct ClineFamilyAdapter;

/// marketplace ids of the three extensions
const EXT_IDS: &[&str] = &[
    "saoudrizwan.claude-dev",
    "rooveterinaryinc.roo-cline",
    "kilocode.kilo-code",
];

/// IDEs the extensions run in (config-relative), plus the remote server
const IDE_DIRS: &[&str] = &["Code", "Cursor", "Windsurf", "VSCodium"];

impl ClineFamilyAdapter {
    fn ide_user_global(&self, ctx: &Ctx, ide: &str) -> PathBuf {
        if ide == ".vscode-server" {
            return ctx.home.join(".vscode-server/data/User/globalStorage");
        }
        ctx.c(&format!("{ide}/User/globalStorage"))
    }

    /// every existing extension globalStorage dir
    fn ext_roots(&self, ctx: &Ctx) -> Vec<(PathBuf, &'static str)> {
        let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
        let mut ides: Vec<&str> = IDE_DIRS.to_vec();
        ides.push(".vscode-server");
        for ide in ides {
            let base = self.ide_user_global(ctx, ide);
            for ext in EXT_IDS {
                let p = base.join(ext);
                if p.is_dir() {
                    out.push((p, *ext));
                }
            }
        }
        out
    }

    /// the IDE state.vscdb files next to the extension dirs
    fn ide_dbs(&self, ctx: &Ctx) -> Vec<(PathBuf, &'static str)> {
        let mut out = Vec::new();
        let mut ides: Vec<&str> = IDE_DIRS.to_vec();
        ides.push(".vscode-server");
        for ide in ides {
            let base = self.ide_user_global(ctx, ide);
            // any of our extensions present makes the IDE's db in scope
            if EXT_IDS.iter().any(|e| base.join(e).is_dir()) {
                let db = if ide == ".vscode-server" {
                    base.parent()
                        .map(|p| p.join("globalStorage/state.vscdb"))
                        .unwrap_or_default()
                } else {
                    ctx.c(&format!("{ide}/User/globalStorage/state.vscdb"))
                };
                if db.is_file() {
                    out.push((db, ide));
                }
            }
        }
        out
    }

    /// roots for the text tree: extension dirs + their settings
    fn tree_roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
        self.ext_roots(ctx).into_iter().map(|(p, _)| p).collect()
    }

    /// rewrite one shadow git's config (core.worktree) inside a
    /// checkpoints dir; .git is invisible to the tree walker
    fn rewrite_checkpoint_git(
        &self,
        ck: &std::path::Path,
        spec: &ReplaceSpec,
        backup: &mut Backup,
    ) {
        let cfg = ck.join(".git").join("config");
        if cfg.is_file() {
            let _ = crate::rewriters::rewrite_text_file(&cfg, spec, backup);
        }
    }

    /// exact-name child rename under `parent` (old -> new), rewriting
    /// any shadow git inside before the rename
    fn rename_child(
        &self,
        parent: &std::path::Path,
        old: &str,
        new: &str,
        spec: &ReplaceSpec,
        backup: &mut Backup,
    ) -> Vec<(PathBuf, PathBuf)> {
        let mut done = Vec::new();
        if !parent.is_dir() || old == new || backup.dry_run {
            return done;
        }
        let old_full = parent.join(old);
        let new_full = parent.join(new);
        if old_full.is_dir() && !new_full.exists() {
            self.rewrite_checkpoint_git(&old_full, spec, backup);
            backup.record_rename(&old_full, &new_full);
            if std::fs::rename(&old_full, &new_full).is_ok() {
                done.push((old_full, new_full));
            }
        }
        done
    }
}

/// Cline <=3.x checkpoints bucket: polynomial hash (x31, u32, UTF-16
/// code units) of the working dir, rendered in decimal — mirrors
/// getHashedWorkingDir in CheckpointUtils.ts
fn cline_cwd_hash(path: &str) -> String {
    let mut h: u32 = 0;
    for u in path.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(u));
    }
    h.to_string()
}

impl Adapter for ClineFamilyAdapter {
    fn name(&self) -> &'static str {
        "cline"
    }
    fn display(&self) -> &'static str {
        "Cline / Roo Code / Kilo Code (VS Code extensions)"
    }
    fn note(&self) -> &'static str {
        "~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code} \
         tasks + taskHistory (file or state.vscdb ItemTable) + \
         checkpoints shadow-git core.worktree + cwdHash/sha256 buckets"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        self.ext_roots(ctx).into_iter().map(|(p, _)| p).collect()
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        // state_paths is filtered by existence, so kinds are computed
        // per root at SCAN time by the caller; a static list cannot
        // zip. The archive/import layer tolerates a shorter kinds list
        // by defaulting missing entries to Home — the extension dirs
        // are config-tree content, so emit Config for the first and
        // let the default apply to any IDE-fork variant beyond it.
        vec![crate::ctx::RootKind::Config]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        // bucket renames
        let old_poly = cline_cwd_hash(&spec.old);
        let new_poly = cline_cwd_hash(&spec.new);
        for (root, _ext) in self.ext_roots(ctx) {
            let ck = root.join("checkpoints");
            for name in [
                old_poly.as_str(),
                &crate::encodings::sha256_hex(&spec.old)[..8],
                &crate::encodings::sha256_hex(&spec.old)[..16],
            ] {
                if ck.join(name).is_dir() {
                    let detail = if name == old_poly {
                        format!("-> {}", new_poly)
                    } else {
                        "hash bucket".to_string()
                    };
                    out.push(mk(self.name(), "dir_rename", &ck.join(name), &detail));
                }
            }
            out.extend(self.scan_tree(spec, std::slice::from_ref(&root)));
        }
        // per-task shadow gits (Roo/Kilo): report the configs we will
        // rewrite (they are invisible to the tree walker)
        for (root, _) in self.ext_roots(ctx) {
            if let Ok(entries) = std::fs::read_dir(root.join("tasks")) {
                for e in entries.filter_map(|e| e.ok()) {
                    let cfg = e.path().join("checkpoints/.git/config");
                    if let Ok(raw) = std::fs::read(&cfg) {
                        if spec.maybe_contains(&raw) {
                            out.push(mk(self.name(), "file", &cfg, "core.worktree"));
                        }
                    }
                }
            }
        }
        // task history in the IDE db
        for (db, ide) in self.ide_dbs(ctx) {
            if let Ok(con) = crate::sqlite::open_ro(&db) {
                let arms = EXT_IDS
                    .iter()
                    .map(|k| format!("\"key\" = '{}'", k.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                let sql = format!(
                    "SELECT \"key\" FROM \"ItemTable\" WHERE ({}) AND \"value\" LIKE ?",
                    arms
                );
                let n: usize = spec
                    .like_patterns()
                    .iter()
                    .map(|p| super::sqlite_like_count(&con, &sql, p))
                    .sum();
                if n > 0 {
                    out.push(mk(
                        self.name(),
                        "sqlite",
                        &db,
                        &format!("{} taskHistory rows ({})", n, ide),
                    ));
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
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let old_poly = cline_cwd_hash(&spec.old);
        let new_poly = cline_cwd_hash(&spec.new);
        for (root, _ext) in self.ext_roots(ctx) {
            // bucket renames (hash dirs) + their shadow git configs
            for (old_h, new_h, sub) in [
                (old_poly.clone(), new_poly.clone(), "checkpoints"),
                (
                    crate::encodings::sha256_hex(&spec.old)[..8].to_string(),
                    crate::encodings::sha256_hex(&spec.new)[..8].to_string(),
                    "checkpoints",
                ),
                (
                    crate::encodings::sha256_hex(&spec.old)[..16].to_string(),
                    crate::encodings::sha256_hex(&spec.new)[..16].to_string(),
                    "sessions",
                ),
            ] {
                for (o, n) in self.rename_child(&root.join(sub), &old_h, &new_h, spec, backup) {
                    actions.push(mk(
                        self.name(),
                        "dir_rename",
                        &o,
                        &format!("-> {}", n.display()),
                    ));
                }
            }
            // per-task shadow gits (Roo/Kilo layout): rewrite configs
            if let Ok(entries) = std::fs::read_dir(root.join("tasks")) {
                for e in entries.filter_map(|e| e.ok()) {
                    let ck = e.path().join("checkpoints");
                    if ck.is_dir() {
                        let cfg = ck.join(".git/config");
                        if cfg.is_file() && crate::rewriters::rewrite_text_file(&cfg, spec, backup)?
                        {
                            actions.push(mk(self.name(), "file", &cfg, "core.worktree"));
                        }
                    }
                }
            }
            // Roo's per-workspace index caches are regenerable derived
            // stores — remove (hash-named, stale shapes resurrect);
            // never during a dry run
            if !backup.dry_run {
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for e in entries.filter_map(|e| e.ok()) {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.starts_with("roo-index-cache-") && name.ends_with(".json") {
                            let _ = std::fs::remove_file(e.path());
                        }
                    }
                }
            }
        }
        actions.extend(self.migrate_text_tree(spec, backup, &self.tree_roots(ctx), deep)?);
        // task history arrays in the IDE state.vscdb, row-scoped to the
        // three extension keys
        for (db, ide) in self.ide_dbs(ctx) {
            backup.record_db(&db)?;
            if backup.dry_run {
                continue;
            }
            let con = crate::sqlite::open_rw(&db)?;
            let arms = EXT_IDS
                .iter()
                .map(|k| format!("\"key\" = '{}'", k.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(" OR ");
            let select = format!(
                "SELECT \"key\",\"value\" FROM \"ItemTable\" WHERE ({}) AND \"value\" LIKE ?",
                arms
            );
            let n = super::rewrite_pair(
                &con,
                &spec.like_patterns(),
                spec,
                &select,
                "UPDATE \"ItemTable\" SET \"value\"=? WHERE \"key\"=?",
            )?;
            if n > 0 {
                actions.push(mk(
                    self.name(),
                    "sqlite",
                    &db,
                    &format!("{} taskHistory rows ({})", n, ide),
                ));
            }
        }
        Ok(actions)
    }
}
