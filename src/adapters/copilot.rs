// SPDX-License-Identifier: MIT OR Apache-2.0
//! GitHub Copilot CLI local state.
//!
//! `~/.copilot` holds agents/, hooks/ and skills/ — agent and hook
//! definitions (JSON/Markdown) that can embed project paths. Session
//! transcripts are not stored under this root on observed installs
//! (GA CLI, Feb 2026); the definition layer is the local surface, and
//! it rides the standard identity-aware text tree.

use super::{Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::PathBuf;

pub struct CopilotAdapter;

impl Adapter for CopilotAdapter {
    fn name(&self) -> &'static str {
        "copilot"
    }
    fn display(&self) -> &'static str {
        "GitHub Copilot CLI"
    }
    fn note(&self) -> &'static str {
        "~/.copilot agents/hooks/skills definitions (session transcripts \
         are not stored locally on observed installs)"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".copilot")]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["copilot"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let root = ctx.h(".copilot");
        self.scan_tree(spec, std::slice::from_ref(&root))
    }

    fn migrate(
        &self,
        ctx: &Ctx,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        deep: bool,
    ) -> Result<Vec<super::Finding>> {
        let root = ctx.h(".copilot");
        let roots = [root];
        self.migrate_text_tree(spec, backup, &roots, deep)
    }
}
