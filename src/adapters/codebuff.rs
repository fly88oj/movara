// SPDX-License-Identifier: MIT OR Apache-2.0
//! Codebuff / Freebuff agent state.
//!
//! Layout (source-verified against CodebuffAI/codebuff,
//! cli/src/project-files.ts + utils/run-state-storage.ts): everything
//! under `~/.config/manicode` (the legacy name survives the Freebuff
//! rebrand; FREEBUFF_CONFIG_DIR can relocate it):
//! - projects/<basename(projectRoot)>/chats/<chatId>/ — chatId is an
//!   ISO timestamp (colons -> dashes); chat-messages.json (full
//!   transcript), chat-meta.json sidecar, run-state.json (whose
//!   sessionState embeds the runtime cwd)
//! - credentials.json, byok/ — credentials, never rewritten
//!
//! The project key is the BARE BASENAME of the project root — not a
//! hash, not the full path. Two projects with the same basename share
//! one storage dir (an upstream design quirk); on migration the dir is
//! renamed to the new basename and a collision at the target is
//! refused loudly rather than merged.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use rust_i18n::t;
use std::path::PathBuf;

pub struct CodebuffAdapter;

impl CodebuffAdapter {
    fn root(&self, ctx: &Ctx) -> PathBuf {
        ctx.c("manicode")
    }
}

impl Adapter for CodebuffAdapter {
    fn name(&self) -> &'static str {
        "codebuff"
    }
    fn display(&self) -> &'static str {
        "Codebuff / Freebuff"
    }
    fn note(&self) -> &'static str {
        "~/.config/manicode/projects/<basename>/chats/<ts>/ — bare-basename \
         project key (upstream collision quirk), run-state sessionState cwd"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.root(ctx)]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![crate::ctx::RootKind::Config]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["codebuff", "freebuff"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let old_b = crate::encodings::basename(&spec.old);
        let new_b = crate::encodings::basename(&spec.new);
        let pdir = self.root(ctx).join("projects").join(&old_b);
        if pdir.is_dir() {
            let detail = if old_b != new_b {
                format!("-> projects/{}", new_b)
            } else {
                "basename bucket".to_string()
            };
            out.push(mk(self.name(), "dir_rename", &pdir, &detail));
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
        let old_b = crate::encodings::basename(&spec.old);
        let new_b = crate::encodings::basename(&spec.new);
        if old_b != new_b {
            let projects = self.root(ctx).join("projects");
            let old_d = projects.join(&old_b);
            let new_d = projects.join(&new_b);
            if old_d.is_dir() {
                if new_d.exists() {
                    // same-basename projects share storage upstream; a
                    // merge is not ours to attempt
                    eprintln!(
                        "{}",
                        t!(
                            "backup.skip_rename",
                            old = old_d.display().to_string().as_str(),
                            new = new_d.display().to_string().as_str()
                        )
                    );
                } else if !backup.dry_run {
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
            }
        }
        let roots = [self.root(ctx)];
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        Ok(actions)
    }
}
