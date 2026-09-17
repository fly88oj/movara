// SPDX-License-Identifier: MIT OR Apache-2.0
//! File-level rewriters: plain text, JSON identity fields, JSONL lines.

use crate::backup::Backup;
use crate::spec::ReplaceSpec;
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;

/// identity-ish JSON keys that hold a project path / URI; rewriting these
/// is always safe (never chat content)
const JSON_FIELD_KEYS: &[&str] = &[
    "cwd",
    "directory",
    "project",
    "projectHash",
    "projectRoot",
    "projectroot",
    "root",
    "workspaceDirectory",
    "workspacePath",
    "workspace",
    "folder",
    "basePath",
    "projectDir",
    "project_dir",
    "workingDirectory",
    "working_directory",
    "homePath",
    "path",
    "data_dir",
    "workDir",
    "work_dir",
];

/// fields whose value is a LIST of paths (e.g. codex workspace_roots)
pub const JSON_LIST_FIELD_KEYS: &[&str] = &[
    "workspace_roots",
    "workspaceFolders",
    "folders",
    "roots",
    "workspace_folders",
];

pub fn rewrite_json_value(v: &mut Value, spec: &ReplaceSpec) {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, mut val) in map.iter_mut() {
                let key = k.as_str();
                if val.is_string()
                    && JSON_FIELD_KEYS.contains(&key)
                    && spec.maybe_contains(val.as_str().unwrap().as_bytes())
                {
                    let s = val.as_str().unwrap();
                    *val = Value::String(spec.replace(s));
                } else if val.is_array() && JSON_LIST_FIELD_KEYS.contains(&key) {
                    if let Value::Array(items) = &mut val {
                        for item in items.iter_mut() {
                            if item.is_string()
                                && spec.maybe_contains(item.as_str().unwrap().as_bytes())
                            {
                                let s = item.as_str().unwrap();
                                *item = Value::String(spec.replace(s));
                            } else {
                                rewrite_json_value(item, spec);
                            }
                        }
                    }
                } else {
                    rewrite_json_value(val, spec);
                }
                // the key itself may be a path (claude.json "projects" map)
                let new_key = if spec.maybe_contains(k.as_bytes()) {
                    spec.replace(k)
                } else {
                    k.clone()
                };
                out.insert(new_key, val.take());
            }
            *map = out;
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite_json_value(item, spec);
            }
        }
        _ => {}
    }
}

/// Parse JSON, rewrite identity fields + path-like keys, write back.
pub fn rewrite_json_file(path: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> Result<bool> {
    let raw = fs::read(path)?;
    if !spec.maybe_contains(&raw) {
        return Ok(false);
    }
    let mut obj: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let before = serde_json::to_string(&obj)?;
    rewrite_json_value(&mut obj, spec);
    let after = serde_json::to_string(&obj)?;
    if before == after {
        return Ok(false);
    }
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    let text = serde_json::to_string_pretty(&obj)?;
    write_atomic(path, text.as_bytes())?;
    Ok(true)
}

/// Line-wise JSON rewrite; with `deep` also whole-line boundary replace for
/// non-JSON lines. JSON lines get identity-field rewriting always; with
/// `deep` the reserialized line is additionally boundary-replaced (covers
/// path mentions inside message content). A single `\r` before the `\n`
/// (CRLF files) is preserved; invalid UTF-8 files are left untouched.
pub fn rewrite_jsonl_file(
    path: &Path,
    spec: &ReplaceSpec,
    backup: &mut Backup,
    deep: bool,
) -> Result<bool> {
    let raw = fs::read(path)?;
    if !spec.maybe_contains(&raw) {
        return Ok(false);
    }
    let text = match String::from_utf8(raw) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        // strip the newline, then at most ONE carriage return (a CRLF line
        // ending); both are re-appended verbatim after rewriting
        let (body, eol) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        let (body, cr) = match body.strip_suffix('\r') {
            Some(b) => (b, "\r"),
            None => (body, ""),
        };
        let mut new_body = body.to_string();
        if body.starts_with('{') && body.ends_with('}') {
            if let Ok(mut obj) = serde_json::from_str::<Value>(body) {
                let before = serde_json::to_string(&obj)?;
                rewrite_json_value(&mut obj, spec);
                let after = serde_json::to_string(&obj)?;
                if after != before {
                    new_body = after;
                }
            }
        }
        if deep {
            new_body = spec.replace(&new_body);
        }
        if new_body != body {
            changed = true;
        }
        out.push_str(&new_body);
        out.push_str(cr);
        out.push_str(eol);
    }
    if !changed {
        return Ok(false);
    }
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    write_atomic(path, out.as_bytes())?;
    Ok(true)
}

/// Boundary-aware replace inside a plain text file.
pub fn rewrite_text_file(path: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> Result<bool> {
    let raw = fs::read(path)?;
    if !spec.maybe_contains(&raw) {
        return Ok(false);
    }
    if raw.contains(&0u8) {
        return Ok(false); // binary
    }
    let text = match String::from_utf8(raw) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let new_text = spec.replace(&text);
    if new_text == text {
        return Ok(false);
    }
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    write_atomic(path, new_text.as_bytes())?;
    Ok(true)
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!(
        "{}.movara-tmp",
        path.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut f = fs::File::create(&tmp)?;
    f.write_all(data)?;
    // preserve the executable bit on unix
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
