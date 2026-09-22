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
    "homedir",
    "sessionDir",
    "workspaceId",
    "working_dir",
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

/// keep only the identity-ish top-level fields of a JSON value (archive
/// projection for dual-purpose configs); None when the value is not an
/// object
pub fn project_identity_fields(v: &Value) -> Option<Value> {
    let map = v.as_object()?;
    let mut kept = serde_json::Map::new();
    for (k, val) in map {
        let keep = (val.is_string() && JSON_FIELD_KEYS.contains(&k.as_str()))
            || (val.is_array() && JSON_LIST_FIELD_KEYS.contains(&k.as_str()));
        if keep {
            kept.insert(k.clone(), val.clone());
        }
    }
    Some(Value::Object(kept))
}

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
/// strip a leading UTF-8 BOM (serde_json rejects it outright; Windows
/// editors prepend it — today the rewriters silently skip such files,
/// which is a behavior bug this fixes)
pub(crate) fn strip_bom(raw: &[u8]) -> &[u8] {
    raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw)
}

/// In-memory JSON core: parse (BOM-tolerant), rewrite identity fields,
/// pretty-print back. None = nothing referenced the spec (or the bytes
/// are not JSON — the caller decides whether to fall back to text).
pub fn json_core(raw: &[u8], spec: &ReplaceSpec) -> Result<Option<Vec<u8>>> {
    if !spec.maybe_contains(raw) {
        return Ok(None);
    }
    let mut obj: Value = match serde_json::from_slice(strip_bom(raw)) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let before = serde_json::to_string(&obj)?;
    rewrite_json_value(&mut obj, spec);
    let after = serde_json::to_string(&obj)?;
    if before == after {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string_pretty(&obj)?.into_bytes()))
}

/// In-memory line-wise JSONL core; with `deep` also whole-line boundary
/// replace for non-JSON lines. None = unchanged (or invalid UTF-8, which
/// the file wrapper leaves untouched as before).
pub fn jsonl_core(raw: &[u8], spec: &ReplaceSpec, deep: bool) -> Result<Option<String>> {
    if !spec.maybe_contains(raw) {
        return Ok(None);
    }
    let text = match String::from_utf8(raw.to_vec()) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
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
            if let Ok(mut obj) =
                serde_json::from_str::<Value>(body.strip_prefix('\u{feff}').unwrap_or(body))
            {
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
    Ok(changed.then_some(out))
}

/// In-memory plain-text core: boundary-aware replace; None = unchanged,
/// binary or invalid UTF-8.
pub fn text_core(raw: &[u8], spec: &ReplaceSpec) -> Option<String> {
    if !spec.maybe_contains(raw) || raw.contains(&0u8) {
        return None;
    }
    let text = String::from_utf8(raw.to_vec()).ok()?;
    let new_text = spec.replace(&text);
    (new_text != text).then_some(new_text)
}

pub fn rewrite_json_file(path: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> Result<bool> {
    let new = json_core(&fs::read(path)?, spec)?;
    let Some(new) = new else {
        return Ok(false);
    };
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    write_atomic(path, &new)?;
    Ok(true)
}

pub fn rewrite_jsonl_file(
    path: &Path,
    spec: &ReplaceSpec,
    backup: &mut Backup,
    deep: bool,
) -> Result<bool> {
    let raw = fs::read(path)?;
    let new = jsonl_core(&raw, spec, deep)?;
    let Some(new) = new else {
        return Ok(false);
    };
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    write_atomic(path, new.as_bytes())?;
    Ok(true)
}

/// Boundary-aware replace inside a plain text file.
pub fn rewrite_text_file(path: &Path, spec: &ReplaceSpec, backup: &mut Backup) -> Result<bool> {
    let raw = fs::read(path)?;
    let new = text_core(&raw, spec);
    let Some(new) = new else {
        return Ok(false);
    };
    backup.record_file(path)?;
    if backup.dry_run {
        return Ok(true);
    }
    write_atomic(path, new.as_bytes())?;
    Ok(true)
}

pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
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
