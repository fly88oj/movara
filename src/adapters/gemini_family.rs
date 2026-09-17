// SPDX-License-Identifier: GPL-3.0-or-later
//! gemini-cli family: gemini, qwen-code, iflow.
//!
//! Verified layouts (see docs/research.md):
//! - gemini  : ~/.gemini/tmp/<slug>/.project_root (ownership marker; slug =
//!   slugify(basename)) + chats/*.json with projectHash =
//!   sha256(cwd); ~/.gemini/history/<slug>/; projects.json
//!   {projects: {abs_path: slug}}; registry self-heals from the
//!   .project_root markers
//! - qwen    : ~/.qwen/projects/<dash-encoded-cwd>/chats/*.jsonl (records
//!   carry cwd, ownership re-checked via sha256) and
//!   ~/.qwen/tmp/<sha256(cwd)>/
//! - iflow   : ~/.iflow/projects/<fromPath-encoded>/ and tmp/history/cache/
//!   snapshots/<sha256(cwd)>/ (fromPath collapses dashes)

use super::{mk, Adapter, Finding};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::rewriters;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct GeminiAdapter;

fn slugify(base: &str) -> String {
    let lower = base.to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed
    }
}

impl GeminiAdapter {
    /// the slug dir under base whose .project_root marker owns spec.old
    fn marker_dir(base: &Path, spec: &ReplaceSpec) -> Option<PathBuf> {
        if !base.is_dir() {
            return None;
        }
        for entry in std::fs::read_dir(base).ok()?.filter_map(|e| e.ok()) {
            let d = entry.path();
            let marker = d.join(".project_root");
            if marker.is_file() {
                if let Ok(content) = std::fs::read_to_string(&marker) {
                    if content.trim() == spec.old {
                        return Some(d);
                    }
                }
            }
        }
        None
    }
}

impl Adapter for GeminiAdapter {
    fn name(&self) -> &'static str {
        "gemini"
    }
    fn display(&self) -> &'static str {
        "Gemini CLI"
    }
    fn note(&self) -> &'static str {
        "~/.gemini/tmp/<slug>/.project_root (ownership marker), chats \
         projectHash=sha256(cwd), history/<slug>/, projects.json \
         {path: slug}"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".gemini")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        for base in [ctx.h(".gemini/tmp"), ctx.h(".gemini/history")] {
            if let Some(d) = Self::marker_dir(&base, spec) {
                out.push(Finding {
                    agent: self.name().into(),
                    kind: "dir_rename".into(),
                    target: d.to_string_lossy().into_owned(),
                    detail: "owned by old path".into(),
                });
            }
        }
        let pj = ctx.h(".gemini/projects.json");
        if pj.is_file() {
            if let Ok(raw) = std::fs::read(&pj) {
                if spec.maybe_contains(&raw) {
                    out.push(mk(self.name(), "file", &pj, "projects.json"));
                }
            }
        }
        out.extend(self.scan_tree(spec, &identity_roots(ctx)));
        out.extend(self.scan_tree(spec, &chat_roots(ctx)));
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
        let old_slug = slugify(&encodings::basename(&spec.old));
        let new_slug = slugify(&encodings::basename(&spec.new));
        for base in [ctx.h(".gemini/tmp"), ctx.h(".gemini/history")] {
            let d = match Self::marker_dir(&base, spec) {
                Some(d) => d,
                None => continue,
            };
            if old_slug != new_slug {
                let new_d = base.join(&new_slug);
                if new_d.exists() {
                    actions.push(Finding {
                        agent: self.name().into(),
                        kind: "info".into(),
                        target: d.to_string_lossy().into_owned(),
                        detail: format!(
                            "slug target {} exists; keeping dir name, \
                             .project_root still rewritten (registry \
                             self-heals)",
                            new_slug
                        ),
                    });
                } else if super::rename_dir(&d, &new_d, backup) {
                    actions.push(mk(
                        self.name(),
                        "dir_rename",
                        &d,
                        &format!("-> {}", new_d.display()),
                    ));
                }
            }
        }
        // projects.json: keys are abs paths, values are slugs
        let pj = ctx.h(".gemini/projects.json");
        if pj.is_file() {
            if let Ok(raw) = std::fs::read_to_string(&pj) {
                if spec.maybe_contains(raw.as_bytes()) {
                    if let Ok(mut obj) = serde_json::from_str::<serde_json::Value>(&raw) {
                        if let Some(projects) =
                            obj.get_mut("projects").and_then(|v| v.as_object_mut())
                        {
                            let mut new_map = serde_json::Map::new();
                            let mut changed = false;
                            for (k, v) in projects.iter_mut() {
                                if k == spec.old.as_str()
                                    || k.starts_with(&format!("{}/", spec.old))
                                {
                                    let nk = spec.replace(k);
                                    let mut nv = v.take();
                                    if nv.as_str() == Some(old_slug.as_str()) {
                                        nv = serde_json::Value::String(new_slug.clone());
                                    }
                                    new_map.insert(nk, nv);
                                    changed = true;
                                } else {
                                    let v = v.take();
                                    new_map.insert(k.clone(), v);
                                }
                            }
                            if changed {
                                *projects = new_map;
                                backup.record_file(&pj)?;
                                if !backup.dry_run {
                                    std::fs::write(&pj, serde_json::to_string_pretty(&obj)?)?;
                                }
                                actions.push(mk(self.name(), "file", &pj, "projects.json keys"));
                            }
                        }
                    }
                }
            }
        }
        // .project_root markers + projects.json + settings: identity scope;
        // chats carry identity fields (projectHash, cwd) rewritten by
        // default, while path mentions inside message content need --deep
        for f in self.scan_tree(spec, &identity_roots(ctx)) {
            if f.kind == "file" && rewriters::rewrite_text_file(Path::new(&f.target), spec, backup)?
            {
                actions.push(mk(self.name(), "file", Path::new(&f.target), "text"));
            }
        }
        for f in self.scan_tree(spec, &chat_roots(ctx)) {
            if f.kind != "file" {
                continue;
            }
            let path = Path::new(&f.target);
            // the ownership marker is plain text and always rewritten
            if path.file_name().is_some_and(|n| n == ".project_root") {
                if rewriters::rewrite_text_file(path, spec, backup)? {
                    actions.push(mk(self.name(), "file", path, "project_root"));
                }
                continue;
            }
            if deep {
                // --deep: raw boundary replace covers content mentions too
                if rewriters::rewrite_text_file(path, spec, backup)? {
                    actions.push(mk(self.name(), "file", path, "text-deep"));
                }
                continue;
            }
            // default: the standard identity-field cascade per extension
            if let Some(hit) =
                super::rewrite_file_by_ext(self.name(), path, spec, backup, false, false)?
            {
                actions.push(hit);
            }
        }
        Ok(actions)
    }
}

fn identity_roots(ctx: &Ctx) -> Vec<PathBuf> {
    [".gemini/projects.json", ".gemini/settings.json"]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
}

fn chat_roots(ctx: &Ctx) -> Vec<PathBuf> {
    [".gemini/tmp", ".gemini/history"]
        .iter()
        .map(|rel| ctx.h(rel))
        .filter(|p| p.exists())
        .collect()
}

// ------------------------------------------------------------- qwen/iflow

struct ForkCfg {
    state_rel: &'static str,
    name: &'static str,
    display: &'static str,
    note: &'static str,
    is_iflow: bool,
}

const QWEN: ForkCfg = ForkCfg {
    state_rel: ".qwen",
    name: "qwen",
    display: "Qwen Code",
    note: "~/.qwen/projects/<dash-encoded-cwd>/chats/*.jsonl (records \
           carry cwd, ownership re-checked via sha256) + \
           ~/.qwen/tmp/<sha256(cwd)>/ + settings.json",
    is_iflow: false,
};

const IFLOW: ForkCfg = ForkCfg {
    state_rel: ".iflow",
    name: "iflow",
    display: "iFlow CLI",
    note: "~/.iflow/projects/<fromPath-encoded>/ + \
           tmp|history|cache|snapshots/<sha256(cwd)>/ + settings.json",
    is_iflow: true,
};

pub struct QwenAdapter;
pub struct IFlowAdapter;

macro_rules! fork_impl {
    ($t:ty, $cfg:expr) => {
        impl $t {
            fn cfg(&self) -> &'static ForkCfg {
                &$cfg
            }

            fn encode(&self, path: &str) -> String {
                if self.cfg().is_iflow {
                    encodings::iflow_bucket(path)
                } else {
                    encodings::dash_encode(path)
                }
            }

            fn hash_dirs(&self, ctx: &Ctx) -> Vec<PathBuf> {
                let rels: &[&str] = if self.cfg().is_iflow {
                    &["tmp", "history", "cache", "snapshots"]
                } else {
                    &["tmp"]
                };
                rels.iter()
                    .map(|r| ctx.h(&format!("{}/{}", self.cfg().state_rel, r)))
                    .collect()
            }

            fn all_roots(&self, ctx: &Ctx) -> Vec<PathBuf> {
                let c = self.cfg();
                [
                    format!("{}/projects", c.state_rel),
                    format!("{}/settings.json", c.state_rel),
                    format!("{}/usage_record.jsonl", c.state_rel),
                    format!("{}/todos", c.state_rel),
                ]
                .iter()
                .map(|rel| ctx.h(rel))
                .chain(self.hash_dirs(ctx))
                .filter(|p| p.exists())
                .collect()
            }
        }

        impl Adapter for $t {
            fn name(&self) -> &'static str {
                self.cfg().name
            }
            fn display(&self) -> &'static str {
                self.cfg().display
            }
            fn note(&self) -> &'static str {
                self.cfg().note
            }

            fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
                vec![ctx.h(self.cfg().state_rel)]
            }

            fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
                let projects = ctx.h(&format!("{}/projects", self.cfg().state_rel));
                let (old_enc, new_enc) = (self.encode(&spec.old), self.encode(&spec.new));
                let mut out =
                    super::encoded_bucket_findings(self.name(), &projects, &old_enc, &new_enc);
                let old_hash = encodings::sha256_hex(&spec.old);
                for base in self.hash_dirs(ctx) {
                    let d = base.join(&old_hash);
                    if d.is_dir() {
                        out.push(Finding {
                            agent: self.name().into(),
                            kind: "dir_rename".into(),
                            target: d.to_string_lossy().into_owned(),
                            detail: "(sha256)".into(),
                        });
                    }
                }
                out.extend(self.scan_tree(spec, &self.all_roots(ctx)));
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
                let projects = ctx.h(&format!("{}/projects", self.cfg().state_rel));
                let (old_enc, new_enc) = (self.encode(&spec.old), self.encode(&spec.new));
                for (o, n) in super::rename_encoded_children(&projects, &old_enc, &new_enc, backup)
                {
                    actions.push(mk(
                        self.name(),
                        "dir_rename",
                        &o,
                        &format!("-> {}", n.display()),
                    ));
                }
                let old_hash = encodings::sha256_hex(&spec.old);
                let new_hash = encodings::sha256_hex(&spec.new);
                if old_hash != new_hash {
                    for base in self.hash_dirs(ctx) {
                        let old_d = base.join(&old_hash);
                        let new_d = base.join(&new_hash);
                        if super::rename_dir(&old_d, &new_d, backup) {
                            actions.push(mk(
                                self.name(),
                                "dir_rename",
                                &old_d,
                                &format!("-> {}", new_d.display()),
                            ));
                        }
                    }
                }
                actions.extend(self.migrate_text_tree(spec, backup, &self.all_roots(ctx), deep)?);
                Ok(actions)
            }
        }
    };
}

fork_impl!(QwenAdapter, QWEN);
fork_impl!(IFlowAdapter, IFLOW);
