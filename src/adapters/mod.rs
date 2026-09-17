// SPDX-License-Identifier: GPL-3.0-or-later
//! Adapter registry. Each adapter knows one agent's state layout and can
//! scan it for references to a project path and migrate them.
//!
//! All SQL in adapters is inline literals executed with bound parameters.

pub mod codex;
pub mod gemini_family;
pub mod misc;
pub mod opencode;
pub mod vscode_family;
pub mod zcode;
pub mod zed;

use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::encodings;
use crate::protobuf;
use crate::rewriters;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use rust_i18n::t;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub agent: String,
    pub kind: String, // file | dir_rename | sqlite | protobuf | info
    pub target: String,
    pub detail: String,
}

fn mk(agent: &str, kind: &str, target: &Path, detail: &str) -> Finding {
    Finding {
        agent: agent.to_string(),
        kind: kind.to_string(),
        target: target.to_string_lossy().into_owned(),
        detail: detail.to_string(),
    }
}

pub(crate) const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "cache",
    "Cache",
    "caches",
    "telemetry",
    "blob_storage",
    "CachedData",
    "CachedProfilesData",
    "CachedExtensionVSIXs",
    "logs",
    "logs_2",
    "debug",
];

/// walk files under the roots (a root may itself be a file); when `exts`
/// is given only those extensions pass ("" matches extensionless files)
pub fn iter_files(roots: &[PathBuf], exts: Option<&[&str]>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        if root.is_file() {
            push_if_ext(root, exts, &mut out);
            continue;
        }
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0 || !SKIP_DIRS.contains(&e.file_name().to_str().unwrap_or(""))
            })
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                push_if_ext(entry.path(), exts, &mut out);
            }
        }
    }
    out.sort();
    out
}

fn push_if_ext(p: &Path, exts: Option<&[&str]>, out: &mut Vec<PathBuf>) {
    if let Ok(meta) = std::fs::metadata(p) {
        if meta.len() > 256 * 1024 * 1024 {
            return;
        }
    }
    if let Some(exts) = exts {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !exts.contains(&ext.as_str()) {
            return;
        }
    }
    out.push(p.to_path_buf());
}

/// dirs equal to or prefixed by the encoded old path (sub-project buckets)
pub fn find_encoded_dirs(parent: &Path, old_enc: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == old_enc || name.starts_with(&format!("{}-", old_enc)) {
                let p = e.path();
                if p.is_dir() {
                    found.push(p);
                }
            }
        }
    }
    found.sort();
    found
}

pub fn rename_dir(old: &Path, new: &Path, backup: &mut Backup) -> bool {
    if !old.is_dir() {
        return false;
    }
    if new.exists() {
        eprintln!(
            "{}",
            t!(
                "backup.skip_rename",
                old = old.display().to_string().as_str(),
                new = new.display().to_string().as_str()
            )
        );
        return false;
    }
    backup.record_rename(old, new);
    if backup.dry_run {
        return true;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::rename(old, new).is_ok()
}

/// rename parent/<enc(old)> and parent/<enc(old)>-suffix dirs
pub fn rename_encoded_children(
    parent: &Path,
    old_enc: &str,
    new_enc: &str,
    backup: &mut Backup,
) -> Vec<(PathBuf, PathBuf)> {
    let mut done = Vec::new();
    if !parent.is_dir() || old_enc == new_enc {
        return done;
    }
    for old_full in find_encoded_dirs(parent, old_enc) {
        let name = old_full
            .file_name()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_name = if name == old_enc {
            new_enc.to_string()
        } else {
            format!("{}{}", new_enc, &name[old_enc.len()..])
        };
        let new_full = parent.join(new_name);
        if rename_dir(&old_full, &new_full, backup) {
            done.push((old_full, new_full));
        }
    }
    done
}

pub fn rewrite_pb_file(path: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> Result<bool> {
    let raw = std::fs::read(path)?;
    if !spec.maybe_contains(&raw) {
        return Ok(false);
    }
    let (new, changed) = protobuf::pb_replace(&raw, spec);
    if !changed || new == raw {
        return Ok(false);
    }
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    let bak = path.to_string_lossy().into_owned() + ".movara-premigration.bak";
    let _ = std::fs::write(&bak, &raw);
    std::fs::write(path, &new)?;
    Ok(true)
}

/// count rows matching a LIKE pattern (used by scans)
pub fn sqlite_like_count(con: &rusqlite::Connection, sql: &str, pattern: &str) -> usize {
    if let Ok(mut stmt) = con.prepare(sql) {
        if let Ok(mut rows) = stmt.query([pattern]) {
            let mut n = 0;
            while let Ok(Some(_)) = rows.next() {
                n += 1;
            }
            return n;
        }
    }
    0
}

/// findings for encoded-bucket directories equal to or prefixed by the
/// encoded old path (shared by claude/omp/pi/droid/qwen-iflow/cursor scans)
pub fn encoded_bucket_findings(
    agent: &str,
    parent: &Path,
    old_enc: &str,
    new_enc: &str,
) -> Vec<Finding> {
    let detail = if new_enc != old_enc {
        format!("-> {}", new_enc)
    } else {
        "encoded bucket".to_string()
    };
    find_encoded_dirs(parent, old_enc)
        .into_iter()
        .map(|d| Finding {
            agent: agent.to_string(),
            kind: "dir_rename".into(),
            target: d.to_string_lossy().into_owned(),
            detail: detail.clone(),
        })
        .collect()
}

/// fetch (pk, value) rows with a literal SELECT and rewrite them with a
/// literal UPDATE (bound parameters only); returns the number of rows
/// whose value actually changed. Errors from missing tables are
/// propagated — callers wrap optional tables with `let _ =`.
pub fn rewrite_pair(
    con: &rusqlite::Connection,
    pattern: &str,
    spec: &ReplaceSpec,
    select_sql: &str,
    update_sql: &str,
) -> Result<usize> {
    // pk is fetched as a dynamically-typed Value so both INTEGER and TEXT
    // primary keys (omp history.id vs most agents' TEXT ids) round-trip
    let rows: Vec<(rusqlite::types::Value, String)> = {
        let mut stmt = con.prepare(select_sql)?;
        let it = stmt.query_map([pattern], |r| {
            Ok((
                r.get::<_, rusqlite::types::Value>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })?;
        it.filter_map(|x| x.ok())
            .filter(|(_, b)| b.is_some())
            .map(|(a, b)| (a, b.unwrap()))
            .collect()
    };
    let mut changed = 0;
    for (pk, value) in rows {
        let new_value = spec.replace(&value);
        if new_value != value {
            con.execute(update_sql, rusqlite::params![new_value, pk])?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// the per-extension rewrite cascade shared by every text-tree walker:
/// jsonl -> identity fields (+ content with deep), json -> identity fields
/// (+ raw text with deep), config extensions always raw text, everything
/// else only with `deep` (or when `aggressive`, e.g. agent-owned state
/// roots where any file may carry the path); .pb files are skipped here
/// and handled by the protobuf pass
pub fn rewrite_file_by_ext(
    agent: &str,
    f: &Path,
    spec: &ReplaceSpec,
    backup: &mut Backup,
    deep: bool,
    aggressive: bool,
) -> Result<Option<Finding>> {
    let ext = f
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    if ext == "pb" {
        return Ok(None);
    }
    if ext == "jsonl" {
        if rewriters::rewrite_jsonl_file(f, spec, backup, deep)? {
            return Ok(Some(mk(agent, "file", f, "jsonl")));
        }
    } else if ext == "json" {
        if rewriters::rewrite_json_file(f, spec, backup)? {
            return Ok(Some(mk(agent, "file", f, "json")));
        } else if deep && rewriters::rewrite_text_file(f, spec, backup)? {
            return Ok(Some(mk(agent, "file", f, "json-deep")));
        }
    } else if (aggressive
        || deep
        || matches!(
            ext.as_str(),
            "toml" | "yaml" | "yml" | "conf" | "ini" | "cfg"
        ))
        && rewriters::rewrite_text_file(f, spec, backup)?
    {
        return Ok(Some(mk(agent, "file", f, "text")));
    }
    Ok(None)
}

pub trait Adapter {
    fn name(&self) -> &'static str;
    fn display(&self) -> &'static str;
    fn note(&self) -> &'static str;
    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf>;
    fn scan(&self, _ctx: &Ctx, _spec: &ReplaceSpec) -> Vec<Finding> {
        Vec::new()
    }
    fn migrate(
        &self,
        _ctx: &Ctx,
        _spec: &ReplaceSpec,
        _backup: &mut Backup,
        _deep: bool,
    ) -> Result<Vec<Finding>> {
        Ok(Vec::new())
    }

    fn installed(&self, ctx: &Ctx) -> bool {
        self.state_paths(ctx).iter().any(|p| p.exists())
    }

    // ---------------------------------------------------- shared helpers

    fn scan_tree(&self, spec: &ReplaceSpec, roots: &[PathBuf]) -> Vec<Finding> {
        let mut out = Vec::new();
        for f in iter_files(roots, None) {
            if let Ok(raw) = std::fs::read(&f) {
                if spec.maybe_contains(&raw) {
                    out.push(Finding {
                        agent: self.name().to_string(),
                        kind: "file".into(),
                        target: f.to_string_lossy().into_owned(),
                        detail: format!("{} bytes", raw.len()),
                    });
                }
            }
        }
        out
    }

    /// identity-field rewriting for .json/.jsonl, config extensions always,
    /// everything else only with `deep`; .pb files excluded (separate pass)
    fn migrate_text_tree(
        &self,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        roots: &[PathBuf],
        deep: bool,
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        for f in iter_files(roots, None) {
            if let Some(finding) = rewrite_file_by_ext(self.name(), &f, spec, backup, deep, false)?
            {
                actions.push(finding);
            }
        }
        Ok(actions)
    }

    fn migrate_pb_tree(
        &self,
        spec: &ReplaceSpec,
        backup: &mut Backup,
        roots: &[PathBuf],
    ) -> Result<Vec<Finding>> {
        let mut actions = Vec::new();
        let pb: Vec<PathBuf> = iter_files(roots, Some(&["pb"])).into_iter().collect();
        for f in pb {
            if rewrite_pb_file(&f, spec, backup)? {
                actions.push(mk(self.name(), "protobuf", &f, "pb"));
            }
        }
        Ok(actions)
    }
}

// ============================================================ claude + omp

pub struct ClaudeAdapter;

impl Adapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "claude"
    }
    fn display(&self) -> &'static str {
        "Claude Code"
    }
    fn note(&self) -> &'static str {
        "~/.claude/projects/<dash-encoded-cwd>/ + ~/.claude.json projects \
         keys + history.jsonl project fields"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".claude"), ctx.h(".claude.json")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let mut out = encoded_bucket_findings(
            self.name(),
            &ctx.h(".claude/projects"),
            &encodings::dash_encode(&spec.old),
            &encodings::dash_encode(&spec.new),
        );
        out.extend(self.scan_tree(spec, &extra_roots(ctx)));
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
        let projects = ctx.h(".claude/projects");
        for (o, n) in rename_encoded_children(
            &projects,
            &encodings::dash_encode(&spec.old),
            &encodings::dash_encode(&spec.new),
            backup,
        ) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &o,
                &format!("-> {}", n.display()),
            ));
        }
        actions.extend(self.migrate_text_tree(spec, backup, &extra_roots(ctx), deep)?);
        Ok(actions)
    }
}

fn extra_roots(ctx: &Ctx) -> Vec<PathBuf> {
    [
        ".claude.json",
        ".claude/history.jsonl",
        ".claude/todos",
        ".claude/file-history",
        ".claude/shell-snapshots",
        ".claude/sessions",
        ".claude/agents",
        ".claude/projects",
    ]
    .iter()
    .map(|rel| ctx.h(rel))
    .filter(|p| p.exists())
    .collect()
}

pub struct OmpAdapter;

impl OmpAdapter {
    fn sessions_dir(&self, ctx: &Ctx) -> PathBuf {
        ctx.h(".omp/agent/sessions")
    }
}

impl Adapter for OmpAdapter {
    fn name(&self) -> &'static str {
        "omp"
    }
    fn display(&self) -> &'static str {
        "Oh My Pi (omp)"
    }
    fn note(&self) -> &'static str {
        "~/.omp/agent/sessions/<omp-bucket>/ (home-relative dash encoding \
         of the canonical cwd) with cwd in the session header + \
         history.db history.cwd"
    }

    fn state_paths(&self, ctx: &Ctx) -> Vec<PathBuf> {
        vec![ctx.h(".omp/agent")]
    }

    fn scan(&self, ctx: &Ctx, spec: &ReplaceSpec) -> Vec<Finding> {
        let (old_enc, new_enc) = omp_buckets(ctx, spec);
        let mut out =
            encoded_bucket_findings(self.name(), &self.sessions_dir(ctx), &old_enc, &new_enc);
        out.extend(self.scan_tree(spec, std::slice::from_ref(&self.sessions_dir(ctx))));
        let db = ctx.h(".omp/agent/history.db");
        if db.is_file() {
            if let Ok(con) = crate::sqlite::open_ro(&db) {
                let n = sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"history\" WHERE \"cwd\" LIKE ?",
                    &spec.like_pattern(),
                );
                if n > 0 {
                    out.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::history", db.display()),
                        detail: format!("{} rows (cwd)", n),
                    });
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
        let sessions = self.sessions_dir(ctx);
        let (old_enc, new_enc) = omp_buckets(ctx, spec);
        for (o, n) in rename_encoded_children(&sessions, &old_enc, &new_enc, backup) {
            actions.push(mk(
                self.name(),
                "dir_rename",
                &o,
                &format!("-> {}", n.display()),
            ));
        }
        actions.extend(self.migrate_text_tree(
            spec,
            backup,
            std::slice::from_ref(&sessions),
            deep,
        )?);
        let db = ctx.h(".omp/agent/history.db");
        if db.is_file() {
            if let Ok(con) = crate::sqlite::open_ro(&db) {
                let n = sqlite_like_count(
                    &con,
                    "SELECT \"id\" FROM \"history\" WHERE \"cwd\" LIKE ?",
                    &spec.like_pattern(),
                );
                drop(con);
                if n > 0 {
                    backup.record_db(&db)?;
                    if !backup.dry_run {
                        let con = crate::sqlite::open_rw(&db)?;
                        rewrite_pair(
                            &con,
                            &spec.like_pattern(),
                            spec,
                            "SELECT \"id\",\"cwd\" FROM \"history\" \
                             WHERE \"cwd\" LIKE ?",
                            "UPDATE \"history\" SET \"cwd\"=? \
                             WHERE \"id\"=?",
                        )?;
                    }
                    actions.push(Finding {
                        agent: self.name().into(),
                        kind: "sqlite".into(),
                        target: format!("{}::history", db.display()),
                        detail: format!("{} rows updated (cwd)", n),
                    });
                }
            }
        }
        Ok(actions)
    }
}

fn omp_buckets(ctx: &Ctx, spec: &ReplaceSpec) -> (String, String) {
    (
        encodings::omp_bucket(&spec.old, &ctx.home.to_string_lossy()),
        encodings::omp_bucket(&spec.new, &ctx.home.to_string_lossy()),
    )
}

// ============================================================ registry

pub fn all() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(ClaudeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(gemini_family::GeminiAdapter),
        Box::new(gemini_family::QwenAdapter),
        Box::new(gemini_family::IFlowAdapter),
        Box::new(opencode::OpencodeAdapter),
        Box::new(OmpAdapter),
        Box::new(zcode::ZcodeAdapter),
        Box::new(vscode_family::CursorAdapter),
        Box::new(vscode_family::WindsurfAdapter),
        Box::new(vscode_family::AntigravityAdapter),
        Box::new(misc::CrushAdapter),
        Box::new(misc::DroidAdapter),
        Box::new(misc::ContinueAdapter),
        Box::new(misc::PiAdapter),
        Box::new(misc::AiderAdapter),
        Box::new(misc::CcConnectAdapter),
        Box::new(zed::ZedAdapter),
    ]
}

pub fn get_adapters(names: Option<&[String]>) -> Result<Vec<Box<dyn Adapter>>> {
    let everything = all();
    match names {
        None => Ok(everything),
        Some(names) => {
            for n in names {
                if !everything.iter().any(|a| a.name() == n.as_str()) {
                    anyhow::bail!(
                        "{}",
                        t!(
                            "common.err_unknown_agent",
                            agent = n.as_str(),
                            known = everything
                                .iter()
                                .map(|a| a.name())
                                .collect::<Vec<_>>()
                                .join(", ")
                                .as_str()
                        )
                    );
                }
            }
            Ok(all()
                .into_iter()
                .filter(|a| names.iter().any(|n| n == a.name()))
                .collect())
        }
    }
}
