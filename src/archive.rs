// SPDX-License-Identifier: MIT OR Apache-2.0
//! Portable state archives: export agent state into a single standard
//! `.tar.gz` and import it on another host, optionally rebasing paths.
//!
//! Format (`format: 1`):
//! ```text
//! manifest.json
//! data/<agent>/<home-relative-path>...
//! ```
//! Creation and extraction use the pure-Rust `tar` + `flate2` crates —
//! the single `movara` binary is self-sufficient, no system toolchain,
//! yet the output stays a bog-standard tar.gz any OS tool can open.
//!
//! Import model: place the staged trees into the live state (honoring
//! the conflict policy and journaling everything, including creations),
//! then run the ordinary migration engine once per `--rebase` rule —
//! bucket renames, row rewrites and registry keys are exactly what
//! `migrate` already does, so import adds no second rewriting engine.

use crate::adapters::{self, Adapter};
use crate::backup::Backup;
use crate::ctx::Ctx;
use crate::spec::{self, ReplaceSpec};
use anyhow::{bail, Context as _, Result};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

pub const FORMAT: u32 = 1;

// ------------------------------------------------------------- manifest

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ExportStats {
    pub files: u64,
    pub bytes: u64,
    pub databases: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub format: u32,
    pub created: String,
    pub host: String,
    pub os: String,
    pub source_home: String,
    pub movara_version: String,
    pub agents: Vec<String>,
    /// true when the export was filtered (--agents / --path): filtered
    /// archives may never replace existing shared databases
    #[serde(default)]
    pub filtered: bool,
    /// every source project path the export could enumerate (registries,
    /// markers) — load-bearing for import-time path verification
    pub paths: Vec<String>,
    pub stats: ExportStats,
}

// ------------------------------------------------------------ exclusion

/// file basenames that must never leave the machine (prefix/suffix globs);
/// `SKIP_DIRS` covers directories on the walk
fn file_excluded(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let n = n.trim_start_matches('.');
    n.starts_with("auth")
        || n.starts_with("credential")
        || n.starts_with("token")
        || n.contains("shell-snap")
        || n == "aider.conf.yml"
}

/// dual-purpose configs are archived as sanitized projections instead of
/// raw copies (path-keyed project data only — settings and secrets stay)
fn projection_for(home_rel: &str) -> Option<Projection> {
    match home_rel {
        ".claude.json" => Some(Projection::ClaudeProjects),
        ".codex/config.toml" => Some(Projection::CodexProjects),
        ".continue/config.json" => Some(Projection::ContinueIdentity),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum Projection {
    ClaudeProjects,
    CodexProjects,
    ContinueIdentity,
}

fn is_db(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("sqlite" | "db" | "vscdb")
    )
}

// --------------------------------------------------------------- writer

pub struct ArchiveWriter {
    final_path: PathBuf,
    tmp_path: PathBuf,
    builder: tar::Builder<flate2::write::GzEncoder<fs::File>>,
}

impl ArchiveWriter {
    /// build at a temp path first; the archive appears atomically on
    /// finish() and never exists half-written
    pub fn create(out: &Path) -> Result<Self> {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp_path = PathBuf::from(format!("{}.part", out.display()));
        let file = fs::File::create(&tmp_path)?;
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        Ok(ArchiveWriter {
            final_path: out.to_path_buf(),
            tmp_path,
            builder: tar::Builder::new(gz),
        })
    }

    fn header(len: u64, dir: bool) -> tar::Header {
        let mut h = tar::Header::new_gnu();
        h.set_size(len);
        // mtime 0 keeps archives deterministic for the golden test
        h.set_mtime(0);
        h.set_mode(if dir { 0o755 } else { 0o644 });
        h.set_cksum();
        h
    }

    pub fn add_dir(&mut self, rel: &str) -> Result<()> {
        let mut h = Self::header(0, true);
        h.set_entry_type(tar::EntryType::Directory);
        self.builder
            .append_data(&mut h, format!("{}/", rel), std::io::empty())
            .with_context(|| format!("archive dir {}", rel))?;
        Ok(())
    }

    pub fn add_file(&mut self, rel: &str, bytes: &[u8]) -> Result<()> {
        let mut h = Self::header(bytes.len() as u64, false);
        self.builder
            .append_data(&mut h, rel, std::io::Cursor::new(bytes))
            .with_context(|| format!("archive file {}", rel))?;
        Ok(())
    }

    pub fn finish(mut self, manifest: &ArchiveManifest) -> Result<PathBuf> {
        let bytes = serde_json::to_vec_pretty(manifest)?;
        self.add_file("manifest.json", &bytes)?;
        self.builder
            .into_inner()
            .and_then(|gz| gz.finish())
            .context("finalize archive")?;
        fs::rename(&self.tmp_path, &self.final_path)?;
        Ok(self.final_path)
    }
}

/// extracted archive; removing the staging dir on drop (success or error)
pub struct Staging {
    pub dir: PathBuf,
    pub manifest: ArchiveManifest,
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn open(path: &Path) -> Result<Staging> {
    let base = std::env::temp_dir().join(format!(
        "movara-import-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S-%6f")
    ));
    fs::create_dir_all(&base)?;
    let file = fs::File::open(path).with_context(|| path.display().to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut ar = tar::Archive::new(gz);
    // tar's unpack refuses `..` and absolute members
    ar.unpack(&base)
        .with_context(|| format!("{}: {}", t!("archive.err_extract"), path.display()))?;
    let mpath = base.join("manifest.json");
    let manifest: ArchiveManifest = serde_json::from_str(&fs::read_to_string(&mpath)?)
        .with_context(|| t!("archive.err_manifest").to_string())?;
    if manifest.format != FORMAT {
        bail!(
            "{}",
            t!(
                "archive.err_format",
                found = manifest.format,
                supported = FORMAT
            )
        );
    }
    Ok(Staging {
        dir: base,
        manifest,
    })
}

// --------------------------------------------------------------- export

pub struct ExportOpts {
    pub out: PathBuf,
    /// true when only a subset of agents/projects was selected
    pub filtered: bool,
    /// only state referencing these project paths (absolute, normalized)
    pub paths: Vec<String>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ExportReport {
    pub archive: String,
    pub agents: Vec<String>,
    pub files: u64,
    pub bytes: u64,
    pub databases: u64,
    pub excluded: u64,
    pub projections: u64,
    pub paths: Vec<String>,
}

pub fn run_export(ctx: &Ctx, list: &[Box<dyn Adapter>], opts: &ExportOpts) -> Result<ExportReport> {
    let mut writer = ArchiveWriter::create(&opts.out)?;
    let mut report = ExportReport {
        archive: crate::ctx::path_str(&opts.out),
        ..Default::default()
    };
    let sel = if opts.paths.is_empty() {
        None
    } else {
        Some(selection_tokens(ctx, &opts.paths))
    };
    for adapter in list {
        if !adapter.installed(ctx) {
            continue;
        }
        for root in adapter.state_paths(ctx) {
            export_root(
                ctx,
                adapter.name(),
                &root,
                sel.as_ref(),
                opts.paths.as_slice(),
                &mut writer,
                &mut report,
            )?;
        }
        report.agents.push(adapter.name().to_string());
    }
    let manifest = ArchiveManifest {
        format: FORMAT,
        created: chrono::Local::now().to_rfc3339(),
        host: std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown".into()),
        os: std::env::consts::OS.to_string(),
        source_home: crate::ctx::path_str(&ctx.home),
        movara_version: crate::VERSION.to_string(),
        agents: report.agents.clone(),
        filtered: opts.filtered || !opts.paths.is_empty(),
        paths: if opts.paths.is_empty() {
            enumerate_paths(ctx)
        } else {
            opts.paths.clone()
        },
        stats: ExportStats {
            files: report.files,
            bytes: report.bytes,
            databases: report.databases,
        },
    };
    writer.finish(&manifest)?;
    Ok(report)
}

/// archive one state root (a directory tree or a single file) under
/// `data/<agent>/<home-relative-path>`
fn export_root(
    ctx: &Ctx,
    agent: &str,
    root: &Path,
    sel: Option<&SelectionTokens>,
    filter_paths: &[String],
    writer: &mut ArchiveWriter,
    report: &mut ExportReport,
) -> Result<()> {
    let home_rel = match root.strip_prefix(&ctx.home) {
        Ok(r) => crate::ctx::path_str(r),
        Err(_) => {
            // state outside the home (e.g. foreign XDG bases) is not
            // portable in format 1 — skip it loudly
            eprintln!(
                "{}",
                t!(
                    "archive.skip_outside_home",
                    agent = agent,
                    root = root.display().to_string().as_str()
                )
            );
            return Ok(());
        }
    };
    if !root.exists() {
        return Ok(());
    }
    let keep: Option<&[String]> = (!filter_paths.is_empty()).then_some(filter_paths);
    if root.is_file() {
        let raw = fetch_state_bytes(root)?;
        if let Some(sel) = sel {
            if !bytes_match(&raw, &sel.content) {
                return Ok(());
            }
            export_file_bytes(ctx, agent, &home_rel, root, &raw, keep, writer, report)?;
            return Ok(());
        }
        export_file_bytes(ctx, agent, &home_rel, root, &raw, keep, writer, report)?;
        return Ok(());
    }
    if let Some(sel) = sel {
        return export_root_filtered(ctx, agent, root, sel, keep, writer, report);
    }
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            e.depth() == 0
                || (!adapters::SKIP_DIRS.contains(&name)
                    && !name.to_ascii_lowercase().contains("shell-snap"))
        })
        .filter_map(|e| e.ok())
    {
        let rel =
            crate::ctx::path_str(entry.path().strip_prefix(&ctx.home).unwrap_or(entry.path()));
        if entry.file_type().is_dir() {
            writer.add_dir(&format!("data/{}/{}", agent, rel))?;
        } else if entry.file_type().is_file() {
            if file_excluded(entry.file_name().to_str().unwrap_or("")) {
                report.excluded += 1;
                continue;
            }
            export_file(ctx, agent, &rel, entry.path(), keep, writer, report)?;
        }
    }
    Ok(())
}

fn export_file(
    ctx: &Ctx,
    agent: &str,
    home_rel: &str,
    path: &Path,
    keep: Option<&[String]>,
    writer: &mut ArchiveWriter,
    report: &mut ExportReport,
) -> Result<()> {
    let raw = fetch_state_bytes(path)?;
    if is_db(path) {
        report.databases += 1;
    }
    export_file_bytes(ctx, agent, home_rel, path, &raw, keep, writer, report)
}

#[allow(clippy::too_many_arguments)]
fn export_file_bytes(
    _ctx: &Ctx,
    agent: &str,
    home_rel: &str,
    path: &Path,
    raw: &[u8],
    keep: Option<&[String]>,
    writer: &mut ArchiveWriter,
    report: &mut ExportReport,
) -> Result<()> {
    // dual-purpose configs leave as sanitized projections; under a path
    // filter the projection keeps only the selected projects' keys
    if let Some(proj) = projection_for(home_rel) {
        let out = project_bytes(proj, raw, keep)?;
        return match out {
            Some(bytes) => {
                writer.add_file(&format!("data/{}/{}", agent, home_rel), &bytes)?;
                report.projections += 1;
                report.files += 1;
                report.bytes += bytes.len() as u64;
                Ok(())
            }
            None => {
                report.excluded += 1;
                Ok(())
            }
        };
    }
    if file_excluded(path.file_name().and_then(|n| n.to_str()).unwrap_or("")) {
        report.excluded += 1;
        return Ok(());
    }
    writer.add_file(&format!("data/{}/{}", agent, home_rel), raw)?;
    report.files += 1;
    report.bytes += raw.len() as u64;
    Ok(())
}

/// read a state file; databases are checkpointed first and a busy/locked
/// database fails the whole export (atomic — no partial archives)
fn fetch_state_bytes(path: &Path) -> Result<Vec<u8>> {
    if is_db(path) {
        crate::sqlite::checkpoint(path).with_context(|| {
            t!(
                "archive.err_db_busy",
                db = path.display().to_string().as_str()
            )
            .to_string()
        })?;
    }
    fs::read(path).with_context(|| path.display().to_string())
}

/// every name a project path can appear as in agent state, split by how
/// each may match: structured encodings are unambiguous (hashes, wrapped
/// buckets, dash-encoded paths) and may match directory names at any
/// depth, file names and file content; the bare basename is ambiguous
/// ("api" names many directories) and may only name a SHALLOW directory
/// (pi keeps memory at projects-memory/<basename>, depth 2) — never file
/// names or content, so a generic same-named tree cannot leak
#[derive(Debug, Default)]
pub struct SelectionTokens {
    /// match file CONTENT (boundary-aware) — path + structured encodings
    pub content: Vec<String>,
    /// match directory NAMES at any depth — structured encodings only
    pub dir_any: Vec<String>,
    /// match directory NAMES at depth <= 2 — adds the basename
    pub dir_shallow: Vec<String>,
    /// match FILE NAMES — hash tokens only (cc-connect <name>_<sha8>.json)
    pub file_hash: Vec<String>,
}

fn selection_tokens(ctx: &Ctx, paths: &[String]) -> SelectionTokens {
    let home = ctx.home.to_string_lossy().into_owned();
    let mut content = std::collections::BTreeSet::new();
    let mut dir_any = std::collections::BTreeSet::new();
    let mut file_hash = std::collections::BTreeSet::new();
    for p in paths {
        content.insert(p.clone());
        for enc in [
            crate::encodings::dash_encode(p),
            crate::encodings::dash_encode_nolead(p),
            crate::encodings::zcode_memory_key(p),
            crate::encodings::omp_bucket(p, &home),
            crate::encodings::pi_bucket(p),
            crate::encodings::droid_bucket(p),
            crate::encodings::iflow_bucket(p),
        ] {
            if !enc.is_empty() {
                content.insert(enc.clone());
                dir_any.insert(enc);
            }
        }
        for h in [
            crate::encodings::sha256_hex(p),
            crate::encodings::sha256_16(p),
            crate::encodings::sha256_8(p),
            crate::encodings::md5_hex(p),
        ] {
            if !h.is_empty() {
                content.insert(h.clone());
                dir_any.insert(h.clone());
                file_hash.insert(h);
            }
        }
    }
    let dir_any: Vec<String> = dir_any.into_iter().collect();
    let mut dir_shallow = dir_any.clone();
    for p in paths {
        let b = crate::encodings::basename(p);
        if !b.is_empty() && !dir_shallow.contains(&b) {
            dir_shallow.push(b);
        }
    }
    SelectionTokens {
        content: content.into_iter().collect(),
        dir_any,
        dir_shallow,
        file_hash: file_hash.into_iter().collect(),
    }
}

/// boundary-aware content match, same component semantics as the
/// migration scanner: the raw path /p/abc must not match inside
/// /p/abc2 — a plain substring test would select sibling projects
fn bytes_match(bytes: &[u8], tokens: &[String]) -> bool {
    for t in tokens {
        let tb = t.as_bytes();
        if tb.is_empty() {
            continue;
        }
        let mut from = 0usize;
        while let Some(pos) = memchr::memmem::find(&bytes[from..], tb) {
            let i = from + pos;
            let end = i + tb.len();
            let left_ok = i == 0 || !crate::spec::is_name_byte(bytes[i - 1]);
            let right_ok = end >= bytes.len() || !crate::spec::is_name_byte(bytes[end]);
            if left_ok && right_ok {
                return true;
            }
            from = i + 1;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn export_root_filtered(
    ctx: &Ctx,
    agent: &str,
    root: &Path,
    sel: &SelectionTokens,
    keep: Option<&[String]>,
    writer: &mut ArchiveWriter,
    report: &mut ExportReport,
) -> Result<()> {
    let mut matched_dirs: Vec<PathBuf> = Vec::new();
    let mut added_dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            e.depth() == 0
                || (!adapters::SKIP_DIRS.contains(&name)
                    && !name.to_ascii_lowercase().contains("shell-snap"))
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        let name = entry.file_name().to_str().unwrap_or("").to_string();
        let under_matched = matched_dirs.iter().any(|m| path.starts_with(m));
        if entry.file_type().is_dir() {
            // a directory named after an encoding of the path selects its
            // whole subtree; the ambiguous basename only at shallow depth
            let named = sel.dir_any.contains(&name)
                || (entry.depth() <= 2 && sel.dir_shallow.contains(&name));
            if named || under_matched {
                matched_dirs.push(path);
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if file_excluded(&name) {
            report.excluded += 1;
            continue;
        }
        // one read feeds both the selection test and the archive write
        let raw = fetch_state_bytes(&path)?;
        if is_db(&path) {
            report.databases += 1;
        }
        let included = under_matched
            || sel.file_hash.iter().any(|t| name.contains(t.as_str()))
            || bytes_match(&raw, &sel.content);
        if !included {
            continue;
        }
        // emit the ancestor chain (deterministic order) before the file
        let mut anc = entry.path().parent().map(|p| p.to_path_buf());
        let mut chain = Vec::new();
        while let Some(a2) = anc {
            if a2 == root {
                break;
            }
            chain.push(a2.clone());
            anc = a2.parent().map(|p| p.to_path_buf());
        }
        for a2 in chain.into_iter().rev() {
            if let Ok(rel) = a2.strip_prefix(&ctx.home) {
                let key = crate::ctx::path_str(rel);
                if added_dirs.insert(key.clone()) {
                    writer.add_dir(&format!("data/{}/{}", agent, key))?;
                }
            }
        }
        let rel = crate::ctx::path_str(path.strip_prefix(&ctx.home).unwrap_or(&path));
        export_file_bytes(ctx, agent, &rel, &path, &raw, keep, writer, report)?;
    }
    Ok(())
}

/// enumerate source project paths from authoritative registries and
/// markers (never by decoding one-way bucket names)
fn enumerate_paths(ctx: &Ctx) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    let absorb_json_keys =
        |file: &Path, field: &str, out: &mut std::collections::BTreeSet<String>| {
            if let Ok(raw) = fs::read_to_string(file) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(obj) = v.get(field).and_then(|f| f.as_object()) {
                        for k in obj.keys() {
                            if k.starts_with('/') {
                                out.insert(k.clone());
                            }
                        }
                    }
                }
            }
        };
    absorb_json_keys(&ctx.h(".claude.json"), "projects", &mut out);
    absorb_json_keys(&ctx.h(".gemini/projects.json"), "projects", &mut out);
    // gemini ownership markers carry the raw path
    for base in [ctx.h(".gemini/tmp"), ctx.h(".gemini/history")] {
        if let Ok(entries) = fs::read_dir(&base) {
            for e in entries.filter_map(|e| e.ok()) {
                if let Ok(c) = fs::read_to_string(e.path().join(".project_root")) {
                    let c = c.trim().to_string();
                    if c.starts_with('/') {
                        out.insert(c);
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

// --------------------------------------------------------------- import

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Skip,
    Replace,
}

pub struct ImportOpts {
    pub rules: Vec<(String, String)>,
    pub agents: Option<Vec<String>>,
    pub policy: Policy,
    pub allow_missing: bool,
    pub dry_run: bool,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportReport {
    pub archive_agents: Vec<String>,
    pub placed: u64,
    pub skipped: u64,
    pub replaced: u64,
    pub merged: u64,
    pub missing_paths: Vec<String>,
    pub rules: Vec<(String, String)>,
    pub changes: usize,
}

/// final destination of a source path under the (already longest-first
/// sorted) rules
pub fn rebase_path(p: &str, rules: &[(String, String)]) -> String {
    for (old, new) in rules {
        if p == old {
            return new.clone();
        }
        if let Some(rest) = p.strip_prefix(&format!("{}/", old)) {
            return format!("{}/{}", new, rest);
        }
    }
    p.to_string()
}

/// verify manifest paths against the target host; returns the missing ones
pub fn verify_paths(paths: &[String], rules: &[(String, String)]) -> Vec<String> {
    paths
        .iter()
        .map(|p| rebase_path(p, rules))
        .filter(|p| !Path::new(p).is_dir())
        .collect()
}

pub fn run_import(
    ctx: &Ctx,
    staging: &Staging,
    list: &[Box<dyn Adapter>],
    opts: &ImportOpts,
    backup: &mut Backup,
) -> Result<ImportReport> {
    let mut report = ImportReport {
        archive_agents: staging.manifest.agents.clone(),
        rules: opts.rules.clone(),
        ..Default::default()
    };
    let wanted = |agent: &str| {
        opts.agents
            .as_ref()
            .map(|a| a.iter().any(|x| x == agent))
            .unwrap_or(true)
    };
    // a filtered exchange may never replace a shared database: it holds
    // other projects' rows (additive import arrives with v1.2)
    let filtered_exchange = staging.manifest.filtered || opts.agents.is_some();
    // confinement: an archive may only place state inside the roots the
    // matching adapter owns — a crafted archive must never write
    // elsewhere under the home
    let state_roots: std::collections::HashMap<String, Vec<PathBuf>> = list
        .iter()
        .map(|a| (a.name().to_string(), a.state_paths(ctx)))
        .collect();
    for agent_dir in sorted_dirs(&staging.dir.join("data"))? {
        let agent = agent_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let Some(allowed_roots) = state_roots.get(&agent) else {
            eprintln!(
                "{}",
                t!("archive.skip_unknown_agent", agent = agent.as_str())
            );
            continue;
        };
        if !wanted(&agent) {
            continue;
        }
        for entry in WalkDir::new(&agent_dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let home_rel = crate::ctx::path_str(
                entry
                    .path()
                    .strip_prefix(&agent_dir)
                    .unwrap_or(entry.path()),
            );
            let src = entry.path();
            let dst = ctx.home.join(&home_rel);
            if dst.components().any(|c| c == Component::ParentDir) {
                continue;
            }
            if !allowed_roots.iter().any(|r| dst.strip_prefix(r).is_ok()) {
                report.skipped += 1;
                eprintln!(
                    "{}",
                    t!(
                        "archive.skip_outside_state",
                        agent = agent.as_str(),
                        path = dst.display().to_string().as_str()
                    )
                );
                continue;
            }
            // projections merge into the target config instead of placing
            if let Some(proj) = projection_for(&home_rel) {
                if opts.dry_run {
                    report.merged += 1;
                    continue;
                }
                let raw = fs::read(src)?;
                merge_projection(proj, &dst, &raw, &home_rel, backup)?;
                report.merged += 1;
                continue;
            }
            // a directory in the way is reported and skipped, never
            // written through
            if dst.is_dir() {
                report.skipped += 1;
                eprintln!(
                    "{}",
                    t!(
                        "archive.skip_dir_conflict",
                        path = dst.display().to_string().as_str()
                    )
                );
                continue;
            }
            if dst.exists() && opts.policy == Policy::Skip {
                report.skipped += 1;
                continue;
            }
            if opts.dry_run {
                report.placed += 1;
                continue;
            }
            ensure_dir_journaled(dst.parent(), backup)?;
            let raw = fs::read(src)?;
            if dst.exists() {
                if is_db(&dst) && filtered_exchange {
                    report.skipped += 1;
                    eprintln!(
                        "{}",
                        t!(
                            "archive.skip_db_filtered",
                            path = dst.display().to_string().as_str()
                        )
                    );
                    continue;
                }
                if is_db(&dst) {
                    backup.record_db(&dst)?;
                    let _ = fs::remove_file(format!("{}-wal", dst.display()));
                    let _ = fs::remove_file(format!("{}-shm", dst.display()));
                } else {
                    backup.record_file(&dst)?;
                }
                crate::rewriters::write_atomic(&dst, &raw)?;
                report.replaced += 1;
            } else {
                backup.record_created(&dst);
                crate::rewriters::write_atomic(&dst, &raw)?;
                backup.record_created(Path::new(&format!("{}-wal", dst.display())));
                backup.record_created(Path::new(&format!("{}-shm", dst.display())));
                report.placed += 1;
            }
        }
    }
    // rebase: the ordinary migration engine, one pass per rule (rules are
    // longest-source-first, so prefix overlaps resolve correctly);
    // skipped entirely in dry-run
    let mut changes = 0usize;
    if !opts.dry_run {
        for (old, new) in &opts.rules {
            let spec = ReplaceSpec::new(old, new)?;
            for adapter in list {
                if adapter.installed(ctx) {
                    changes += adapter.migrate(ctx, &spec, backup, false)?.len();
                }
            }
        }
    }
    report.changes = changes;
    backup.manifest.rules = opts.rules.clone();
    Ok(report)
}

/// create the parent chain, journaling every directory level that did not
/// exist before so undo can prune it again
fn ensure_dir_journaled(parent: Option<&Path>, backup: &mut Backup) -> Result<()> {
    let Some(p) = parent else { return Ok(()) };
    let mut to_create = Vec::new();
    let mut cur = p.to_path_buf();
    while !cur.exists() {
        to_create.push(cur.clone());
        match cur.parent() {
            Some(par) => cur = par.to_path_buf(),
            None => break,
        }
    }
    for d in to_create.iter().rev() {
        backup.record_created(d);
        fs::create_dir_all(d)?;
    }
    Ok(())
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------- projections

/// does this registry key belong to a selected path (exact or sub-path)?
fn key_selected(key: &str, keep: Option<&[String]>) -> bool {
    match keep {
        None => true,
        Some(paths) => paths
            .iter()
            .any(|p| key == p || key.starts_with(&format!("{}/", p))),
    }
}

fn project_bytes(proj: Projection, raw: &[u8], keep: Option<&[String]>) -> Result<Option<Vec<u8>>> {
    match proj {
        Projection::ClaudeProjects => {
            let v: serde_json::Value = match serde_json::from_slice(raw) {
                Ok(v) => v,
                Err(_) => return Ok(None),
            };
            let kept: serde_json::Map<String, serde_json::Value> = v
                .get("projects")
                .and_then(|p| p.as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(k, _)| key_selected(k, keep))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default();
            if kept.is_empty() {
                Ok(None)
            } else {
                Ok(Some(serde_json::to_vec_pretty(
                    &serde_json::json!({ "projects": kept }),
                )?))
            }
        }
        Projection::ContinueIdentity => {
            let v: serde_json::Value = match serde_json::from_slice(raw) {
                Ok(v) => v,
                Err(_) => return Ok(None),
            };
            let mut kept = match crate::rewriters::project_identity_fields(&v) {
                Some(k) => k,
                None => return Ok(None),
            };
            if let (Some(paths), Some(obj)) = (keep, kept.as_object_mut()) {
                obj.retain(|_, v| {
                    v.as_str()
                        .map(|s| {
                            paths
                                .iter()
                                .any(|p| s == p || s.starts_with(&format!("{}/", p)))
                        })
                        .unwrap_or(true)
                });
            }
            if kept.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                Ok(None)
            } else {
                Ok(Some(serde_json::to_vec_pretty(&kept)?))
            }
        }
        Projection::CodexProjects => {
            let text = String::from_utf8_lossy(raw);
            let section = codex_projects_section(&text);
            let section = match keep {
                None => section,
                Some(paths) => split_toml_tables(&section)
                    .into_iter()
                    .filter(|b| paths.iter().any(|p| b.contains(p.as_str())))
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            if section.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(section.into_bytes()))
            }
        }
    }
}

/// copy the `[projects]` / `[projects."<path>"]` tables out of a codex
/// config.toml (textual — no toml dependency)
fn codex_projects_section(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == "[projects]" || t.starts_with("[projects.");
            if inside {
                out.push_str(line);
                out.push('\n');
            }
        } else if inside && !t.is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// merge a projection into the live config: archived entries are added,
/// the target's own settings win; everything is journaled
fn merge_projection(
    proj: Projection,
    dst: &Path,
    archived: &[u8],
    home_rel: &str,
    backup: &mut Backup,
) -> Result<()> {
    match proj {
        Projection::ClaudeProjects => {
            let archived: serde_json::Value = serde_json::from_slice(archived)?;
            let mut target: serde_json::Value = if dst.is_file() {
                serde_json::from_slice(&fs::read(dst)?).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            let changed = merge_json_object(
                target
                    .as_object_mut()
                    .context("claude config is not a json object")?,
                archived
                    .get("projects")
                    .and_then(|p| p.as_object())
                    .context("projection without projects")?,
                "projects",
            );
            if changed {
                if dst.is_file() {
                    backup.record_file(dst)?;
                } else {
                    ensure_dir_journaled(dst.parent(), backup)?;
                    backup.record_created(dst);
                }
                crate::rewriters::write_atomic(dst, &serde_json::to_vec_pretty(&target)?)?;
            }
        }
        Projection::ContinueIdentity => {
            let archived: serde_json::Value = serde_json::from_slice(archived)?;
            let mut target: serde_json::Value = if dst.is_file() {
                serde_json::from_slice(&fs::read(dst)?).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            let changed = merge_json_object(
                target
                    .as_object_mut()
                    .context("continue config is not a json object")?,
                archived.as_object().context("projection not an object")?,
                "",
            );
            if changed {
                if dst.is_file() {
                    backup.record_file(dst)?;
                } else {
                    ensure_dir_journaled(dst.parent(), backup)?;
                    backup.record_created(dst);
                }
                crate::rewriters::write_atomic(dst, &serde_json::to_vec_pretty(&target)?)?;
            }
        }
        Projection::CodexProjects => {
            let section = String::from_utf8_lossy(archived).into_owned();
            // a crafted projection may only carry [projects] tables —
            // re-derive the section and require it verbatim
            if codex_projects_section(&section).trim() != section.trim() {
                bail!("{}", t!("archive.err_projection"));
            }
            if dst.is_file() {
                let current = fs::read_to_string(dst)?;
                let mut addition = String::new();
                for block in split_toml_tables(&section) {
                    let header = block.lines().next().unwrap_or("").trim().to_string();
                    if !current.contains(&header) {
                        addition.push_str(&block);
                        addition.push('\n');
                    }
                }
                if !addition.is_empty() {
                    backup.record_file(dst)?;
                    let mut merged = current;
                    if !merged.ends_with('\n') {
                        merged.push('\n');
                    }
                    merged.push_str(&addition);
                    fs::write(dst, merged)?;
                }
            } else {
                ensure_dir_journaled(dst.parent(), backup)?;
                backup.record_created(dst);
                fs::write(dst, section.as_bytes())?;
            }
        }
    }
    let _ = home_rel;
    Ok(())
}

/// insert archived keys the target lacks (target wins per key);
/// `section` nests under a top-level object key when non-empty
fn merge_json_object(
    target: &mut serde_json::Map<String, serde_json::Value>,
    archived: &serde_json::Map<String, serde_json::Value>,
    section: &str,
) -> bool {
    let mut changed = false;
    if section.is_empty() {
        for (k, v) in archived {
            if !target.contains_key(k) {
                target.insert(k.clone(), v.clone());
                changed = true;
            }
        }
        return changed;
    }
    let slot = target
        .entry(section.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let Some(map) = slot.as_object_mut() {
        for (k, v) in archived {
            if !map.contains_key(k) {
                map.insert(k.clone(), v.clone());
                changed = true;
            }
        }
    }
    changed
}

/// split a toml fragment into `[table]` blocks (header line + body)
fn split_toml_tables(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if line.trim().starts_with('[') {
            if !cur.trim().is_empty() {
                blocks.push(cur.clone());
            }
            cur.clear();
        }
        if !line.trim().is_empty() {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    if !cur.trim().is_empty() {
        blocks.push(cur);
    }
    blocks
}

// validate rules from raw CLI strings (see spec::prepare_rules)
pub fn prepare_rules(raw: &[String]) -> Result<Vec<(String, String)>> {
    spec::prepare_rules(raw)
}
