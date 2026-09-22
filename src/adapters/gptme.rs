// SPDX-License-Identifier: MIT OR Apache-2.0
//! gptme local sessions.
//!
//! Layout (source-verified against gptme/gptme master):
//! - `~/.local/share/gptme/logs/<YYYY-MM-DD>-<name>/` — FLAT, one dir
//!   per conversation; names derive from date + random/user/LLM name,
//!   never from the project path
//!   - `config.toml` — `[chat] workspace = "..."` is THE project link;
//!     gptme saves it tilde-abbreviated when the project is under the
//!     home dir (`~/works/x`), so both forms are rewritten
//!   - `workspace` — a SYMLINK to the project dir (retargeted, never
//!     followed); magic value `@log` means a real local dir instead
//!   - `conversation.jsonl` — one Message per line; no cwd field, but
//!     `files` lists carry absolute attachment paths (identity list
//!     key); branches/ and views/ duplicate message content
//!   - `files/<sha256[:16]><ext>` — content-addressed attachments
//!     (binary payloads; the hash is over CONTENT, not paths)
//!   - `.lock`, `*.toml.tmp` — runtime files, skipped
//!
//! No path-derived directory names — no bucket renames anywhere.

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct GptmeAdapter;

impl GptmeAdapter {
    fn root(&self, ctx: &Ctx) -> PathBuf {
        ctx.d("gptme/logs")
    }

    /// every conversation dir (flat under the logs root)
    fn conv_dirs(&self, ctx: &Ctx) -> Vec<PathBuf> {
        let root = self.root(ctx);
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for e in entries.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() && p.join("conversation.jsonl").is_file() {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// retarget a conversation's workspace symlink when it points into
    /// the old project (or at it exactly)
    fn retarget_symlink(conv: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> bool {
        let link = conv.join("workspace");
        let meta = match std::fs::symlink_metadata(&link) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if !meta.file_type().is_symlink() {
            return false; // @log real dir or absent
        }
        let target = match std::fs::read_link(&link) {
            Ok(t) => t,
            Err(_) => return false,
        };
        let t = target.to_string_lossy();
        let new_target = if t == spec.old {
            Some(PathBuf::from(&spec.new))
        } else if t.starts_with(&format!("{}/", spec.old))
            || t.starts_with(&format!("{}\\", spec.old))
        {
            Some(PathBuf::from(format!(
                "{}{}",
                spec.new,
                &t[spec.old.len()..]
            )))
        } else {
            None
        };
        if let Some(nt) = new_target {
            backup.record_rename(&link, &link);
            let _ = std::fs::remove_file(&link);
            #[cfg(unix)]
            {
                if std::os::unix::fs::symlink(&nt, &link).is_ok() {
                    return true;
                }
            }
            #[cfg(windows)]
            {
                let _ = nt;
            }
        }
        false
    }
}

impl Adapter for GptmeAdapter {
    fn name(&self) -> &'static str {
        "gptme"
    }
    fn display(&self) -> &'static str {
        "gptme"
    }
    fn note(&self) -> &'static str {
        "~/.local/share/gptme/logs/<date>-<name>/ config.toml [chat] \
         workspace (tilde form included) + workspace symlink + files lists"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![self.root(ctx)]
    }

    fn root_kinds(&self) -> Vec<crate::ctx::RootKind> {
        vec![crate::ctx::RootKind::Data]
    }

    fn process_names(&self) -> &'static [&'static str] {
        &["gptme"]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = self.scan_tree(spec, std::slice::from_ref(&self.root(ctx)));
        for conv in self.conv_dirs(ctx) {
            let link = conv.join("workspace");
            if std::fs::symlink_metadata(&link)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                if let Ok(t) = std::fs::read_link(&link) {
                    let ts = t.to_string_lossy();
                    if ts == spec.old || ts.starts_with(&format!("{}/", spec.old)) {
                        out.push(mk(self.name(), "symlink", &link, "-> new workspace"));
                    }
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
        let roots = [self.root(ctx)];
        actions.extend(self.migrate_text_tree(spec, backup, &roots, deep)?);
        // gptme saves the workspace tilde-abbreviated when the project
        // is under the home dir — the absolute needle never matches
        // that form, and the spec machinery would cwd-join a "~"-path,
        // so this is a direct boundary-aware string replacement
        let home = ctx.home.to_string_lossy().into_owned();
        if spec.old.starts_with(&home) && spec.new.starts_with(&home) {
            let t_old = format!("~{}", &spec.old[home.len()..]);
            let t_new = format!("~{}", &spec.new[home.len()..]);
            for conv in self.conv_dirs(ctx) {
                let cfg = conv.join("config.toml");
                if !cfg.is_file() {
                    continue;
                }
                if let Ok(raw) = std::fs::read_to_string(&cfg) {
                    if let Some(new_text) = boundary_replace(&raw, &t_old, &t_new) {
                        backup.record_file(&cfg)?;
                        crate::rewriters::write_atomic(&cfg, new_text.as_bytes())?;
                        actions.push(mk(self.name(), "file", &cfg, "workspace (tilde)"));
                    }
                }
            }
        }
        for conv in self.conv_dirs(ctx) {
            if Self::retarget_symlink(&conv, spec, backup) {
                actions.push(mk(
                    self.name(),
                    "symlink",
                    &conv.join("workspace"),
                    "retargeted",
                ));
            }
        }
        Ok(actions)
    }
}

/// boundary-aware whole-token replacement (`~/a/abc` does not match
/// inside `~/a/abc2`); None when nothing changed
fn boundary_replace(raw: &str, from: &str, to: &str) -> Option<String> {
    let hb = raw.as_bytes();
    let fb = from.as_bytes();
    let is_name =
        |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-' || b == b'~';
    let mut out = String::with_capacity(raw.len());
    let mut i = 0usize;
    let mut changed = false;
    while i < hb.len() {
        if let Some(pos) = memchr::memmem::find(&hb[i..], fb) {
            let s = i + pos;
            let end = s + fb.len();
            let left_ok = s == 0 || !is_name(hb[s - 1]);
            let right_ok = end >= hb.len() || !is_name(hb[end]);
            if left_ok && right_ok {
                out.push_str(&raw[i..s]);
                out.push_str(to);
                i = end;
                changed = true;
                continue;
            }
            out.push_str(&raw[i..s + 1]);
            i = s + 1;
        } else {
            out.push_str(&raw[i..]);
            break;
        }
    }
    changed.then_some(out)
}
