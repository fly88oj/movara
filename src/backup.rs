// SPDX-License-Identifier: GPL-3.0-or-later
//! Change journal and undo: modified files and sqlite databases are copied
//! into the backup tree before being touched; renames and moved project
//! directories are journaled so `undo` can restore everything.

use anyhow::{Context as _, Result};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub created: String,
    pub from: String,
    pub to: String,
    pub agents: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub dbs: Vec<FileEntry>,
    #[serde(default)]
    pub renames: Vec<(String, String)>,
    #[serde(default)]
    pub moved_project: Option<(String, String)>,
    /// rebase rules an import applied (empty for plain migrations)
    #[serde(default)]
    pub rules: Vec<(String, String)>,
    /// paths the change CREATED (absent before); undo removes them and
    /// prunes directories that only exist because of the change
    #[serde(default)]
    pub created_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub backup_rel: String,
    #[serde(default)]
    pub size: u64,
}

pub struct Backup {
    pub dir: PathBuf,
    pub data_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
    pub dry_run: bool,
    dbs_done: std::collections::HashSet<PathBuf>,
    files_done: std::collections::HashSet<PathBuf>,
}

impl Backup {
    pub fn new(
        backup_dir: &Path,
        spec: &crate::spec::ReplaceSpec,
        agents: Vec<String>,
        dry_run: bool,
    ) -> Self {
        let id = chrono::Local::now().format("%Y%m%d-%H%M%S-%6f").to_string();
        let dir = backup_dir.join(&id);
        let data_dir = dir.join("data");
        Backup {
            manifest_path: dir.join("manifest.json"),
            dir,
            data_dir,
            manifest: Manifest {
                id,
                created: chrono::Local::now().to_rfc3339(),
                from: spec.old.clone(),
                to: spec.new.clone(),
                agents,
                files: Vec::new(),
                dbs: Vec::new(),
                renames: Vec::new(),
                moved_project: None,
                rules: Vec::new(),
                created_paths: Vec::new(),
            },
            dry_run,
            dbs_done: std::collections::HashSet::new(),
            files_done: std::collections::HashSet::new(),
        }
    }

    /// Copy a file into the backup tree before it is rewritten (each file
    /// is journaled at most once per backup).
    pub fn record_file(&mut self, path: &Path) -> Result<bool> {
        if self.dry_run {
            return Ok(false);
        }
        let path = absolute(path);
        if !self.files_done.insert(path.clone()) {
            return Ok(false);
        }
        let rel = backup_rel(&path)?;
        let dst = self.data_dir.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("{}: {}", t!("backup.err_create_dir"), parent.display())
            })?;
        }
        fs::copy(&path, &dst)
            .with_context(|| format!("{}: {}", t!("backup.err_copy"), path.display()))?;
        let meta = fs::metadata(&dst)?;
        self.manifest.files.push(FileEntry {
            path: crate::ctx::path_str(&path),
            backup_rel: rel,
            size: meta.len(),
        });
        Ok(true)
    }

    /// Checkpoint WAL and copy a whole sqlite database before modifying it.
    pub fn record_db(&mut self, db: &Path) -> Result<bool> {
        if self.dry_run {
            return Ok(false);
        }
        let db = absolute(db);
        if self.dbs_done.contains(&db) {
            return Ok(false);
        }
        self.dbs_done.insert(db.clone());
        let _ = crate::sqlite::checkpoint(&db);
        let rel = backup_rel(&db)?;
        let dst = self.data_dir.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("{}: {}", t!("backup.err_create_dir"), parent.display())
            })?;
        }
        fs::copy(&db, &dst)
            .with_context(|| format!("{}: {}", t!("backup.err_copy_db"), db.display()))?;
        self.manifest.dbs.push(FileEntry {
            path: crate::ctx::path_str(&db),
            backup_rel: rel,
            size: fs::metadata(&dst)?.len(),
        });
        Ok(true)
    }

    pub fn record_rename(&mut self, old: &Path, new: &Path) {
        self.manifest.renames.push((
            crate::ctx::path_str(&absolute(old)),
            crate::ctx::path_str(&absolute(new)),
        ));
    }

    /// Journal a path this change creates (file, database or directory);
    /// undo deletes it again and prunes parent directories the change
    /// left behind. Call BEFORE creating the path.
    pub fn record_created(&mut self, path: &Path) {
        if self.dry_run {
            return;
        }
        let p = crate::ctx::path_str(&absolute(path));
        if !self.manifest.created_paths.contains(&p) {
            self.manifest.created_paths.push(p);
        }
    }

    pub fn save(&self) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }
        fs::create_dir_all(&self.dir)?;
        fs::write(
            &self.manifest_path,
            serde_json::to_string_pretty(&self.manifest)?,
        )?;
        Ok(())
    }
}

fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    }
}

fn backup_rel(path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(std::path::Component::RootDir.as_os_str())
        .unwrap_or(path);
    let s = rel.to_string_lossy().into_owned();
    if Path::new(&s)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        anyhow::bail!(
            "{}",
            t!(
                "backup.err_escape",
                path = path.display().to_string().as_str()
            )
        );
    }
    Ok(s)
}

/// Revert a migration: reverse renames first (mapping journaled paths back
/// to their original locations), then restore databases and files, then the
/// moved project directory.
pub fn undo(backup_dir: &Path, backup_id: &str) -> Result<bool> {
    let mpath = backup_dir.join(backup_id).join("manifest.json");
    let manifest: Manifest = serde_json::from_str(&fs::read_to_string(&mpath)?)
        .with_context(|| format!("{}: {}", t!("backup.err_read_manifest"), mpath.display()))?;
    let bdir = mpath.parent().unwrap().to_path_buf();
    let data_dir = bdir.join("data");
    let mut ok = true;

    // 1. reverse renames (reverse order; directories and files alike).
    //    A failing rename must not abort the whole undo: later renames and
    //    file restores still apply, so record the error and continue.
    for (old, new) in manifest.renames.iter().rev() {
        let (old, new) = (PathBuf::from(old), PathBuf::from(new));
        if new.exists() {
            if old.exists() {
                eprintln!(
                    "{}",
                    t!(
                        "backup.cannot_reverse",
                        new = new.display().to_string().as_str(),
                        old = old.display().to_string().as_str()
                    )
                );
                ok = false;
                continue;
            }
            if let Err(e) = fs::rename(&new, &old) {
                eprintln!(
                    "{}",
                    t!(
                        "backup.err_reverse",
                        new = new.display().to_string().as_str(),
                        old = old.display().to_string().as_str(),
                        error = e.to_string().as_str()
                    )
                );
                ok = false;
            }
        }
    }
    let to_original = |p: &str| -> String {
        let mut best: Option<(&String, &String)> = None;
        for (old, new) in &manifest.renames {
            if (p == new || p.starts_with(&format!("{}{}", new, std::path::MAIN_SEPARATOR)))
                && best
                    .as_ref()
                    .map(|(_, bn)| new.len() > bn.len())
                    .unwrap_or(true)
            {
                best = Some((old, new));
            }
        }
        match best {
            Some((old, new)) => format!("{}{}", old, &p[new.len()..]),
            None => p.to_string(),
        }
    };

    // 2. restore databases (whole-file copies; drop stale wal/shm)
    for entry in &manifest.dbs {
        if entry.backup_rel.contains("..") {
            continue;
        }
        let src = data_dir.join(&entry.backup_rel);
        let dst = to_original(&entry.path);
        if src.is_file() {
            let dst = PathBuf::from(&dst);
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::copy(&src, &dst)?;
            let _ = fs::remove_file(format!("{}-wal", dst.display()));
            let _ = fs::remove_file(format!("{}-shm", dst.display()));
        } else {
            eprintln!("{}", t!("backup.missing_db", path = entry.path.as_str()));
            ok = false;
        }
    }

    // 3. restore files
    for entry in &manifest.files {
        if entry.backup_rel.contains("..") {
            continue;
        }
        let src = data_dir.join(&entry.backup_rel);
        let dst = PathBuf::from(to_original(&entry.path));
        if src.is_file() {
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::copy(&src, &dst)?;
        } else {
            eprintln!("{}", t!("backup.missing_file", path = entry.path.as_str()));
        }
    }

    // 4. restore moved project dir (a conflict means something was created
    //    at the old location since the migration — warn, don't clobber)
    if let Some((old, new)) = &manifest.moved_project {
        let (old, new) = (PathBuf::from(old), PathBuf::from(new));
        if new.is_dir() {
            if old.exists() {
                eprintln!(
                    "{}",
                    t!(
                        "backup.warn_moved_conflict",
                        old = old.display().to_string().as_str(),
                        new = new.display().to_string().as_str()
                    )
                );
                ok = false;
            } else if let Err(e) = fs::rename(&new, &old) {
                eprintln!(
                    "{}",
                    t!(
                        "backup.err_reverse",
                        new = new.display().to_string().as_str(),
                        old = old.display().to_string().as_str(),
                        error = e.to_string().as_str()
                    )
                );
                ok = false;
            }
        }
    }

    // 5. remove paths the change created (files and databases first,
    //    then directories deepest-first, pruning only empties) — this is
    //    what makes `undo` reverse an import completely
    let mut created_dirs: Vec<PathBuf> = Vec::new();
    for p in &manifest.created_paths {
        if p.contains("..") {
            continue;
        }
        let path = PathBuf::from(p);
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            created_dirs.push(path);
        } else if let Err(e) = fs::remove_file(&path) {
            eprintln!(
                "{}",
                t!(
                    "backup.err_remove_created",
                    path = p.as_str(),
                    error = e.to_string().as_str()
                )
            );
            ok = false;
        }
    }
    created_dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in created_dirs {
        if d.read_dir()
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
        {
            if let Err(e) = fs::remove_dir(&d) {
                eprintln!(
                    "{}",
                    t!(
                        "backup.err_remove_created",
                        path = d.display().to_string().as_str(),
                        error = e.to_string().as_str()
                    )
                );
                ok = false;
            }
        }
    }

    Ok(ok)
}

/// list backup ids (directories containing a manifest)
pub fn list_backups(backup_dir: &Path) -> Result<Vec<Manifest>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(backup_dir) {
        Ok(e) => e,
        Err(_) => return Ok(out),
    };
    let mut ids: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    ids.sort();
    for p in ids.into_iter().rev() {
        if let Ok(m) =
            serde_json::from_str(&fs::read_to_string(p.join("manifest.json")).unwrap_or_default())
        {
            out.push(m);
        }
    }
    Ok(out)
}

/// copy helper preserving permissions (best effort); symlinks are
/// recreated verbatim on unix and resolved-copy'd elsewhere
pub fn copy_entry(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_entry(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else if src.is_symlink() {
        let target = fs::read_link(src)?;
        let _ = fs::remove_file(dst);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, dst)
        }
        #[cfg(not(unix))]
        {
            // windows symlinks need privileges; copy the target instead
            if target.is_absolute() || target.exists() {
                fs::copy(&target, dst).map(|_| ())
            } else {
                fs::copy(src, dst).map(|_| ())
            }
        }
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}
