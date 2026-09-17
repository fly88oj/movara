// SPDX-License-Identifier: GPL-3.0-or-later
//! Path-name encodings used by various AI agents to key state by project
//! cwd (verified against real data; see docs/research.md):
//!
//! - dash encoding (Claude Code, qwen-code): every char outside [A-Za-z0-9]
//!   becomes '-'. "/home/u/abc" -> "-home-u-abc".
//! - cursor CLI variant: same but without the leading dash.
//! - full sha256 hex (qwen/iflow tmp dirs, gemini chat "projectHash").
//! - sha256[:16] (zcode memory keys "<basename>-<hash16>").
//! - omp bucket: canonicalized cwd, home-relative when under home.
//! - pi bucket: "--<encoded>--" ('/' '\' ':' -> '-').
//! - droid bucket: realpath with only slashes turned into dashes.
//! - iflow bucket: dash-collapsing ProjectNameGenerator::fromPath().
//!
//! The md5 helper replicates third-party directory/key naming schemes
//! (windsurf context_state/database dirs, keyed by md5 of the path) —
//! interop only, not a security primitive.

use sha2::{Digest as _, Sha256};

pub fn dash_encode(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub fn dash_encode_nolead(path: &str) -> String {
    dash_encode(path).trim_start_matches('-').to_string()
}

pub fn sha256_hex(path: &str) -> String {
    let mut h = Sha256::new();
    h.update(path.as_bytes());
    hex(&h.finalize())
}

pub fn sha256_16(path: &str) -> String {
    sha256_hex(path)[..16].to_string()
}

/// cc-connect session file suffix: first 4 bytes of sha256(workDir)
pub fn sha256_8(path: &str) -> String {
    sha256_hex(path)[..8].to_string()
}

pub fn md5_hex(path: &str) -> String {
    use md5::{Digest as _, Md5};
    let mut h = Md5::new();
    h.update(path.as_bytes());
    hex(&h.finalize())
}

pub fn basename(path: &str) -> String {
    let p = path.trim_end_matches('/');
    let name = p.rsplit('/').next().unwrap_or(p);
    if name.is_empty() {
        "/".to_string()
    } else {
        name.to_string()
    }
}

pub fn zcode_memory_key(path: &str) -> String {
    format!("{}-{}", basename(path), sha256_16(path))
}

/// omp sessions bucket: canonicalized cwd, encoded relative to home when
/// underneath, otherwise the full path; '/' (and any non-alnum) -> '-'.
pub fn omp_bucket(path: &str, home: &str) -> String {
    let p = canonical_str(path);
    let h = canonical_str(home);
    let rel = if p == h {
        return "-home".to_string();
    } else if p.starts_with(&format!("{}/", h)) {
        p[h.len() + 1..].to_string()
    } else {
        p.trim_start_matches('/').to_string()
    };
    format!("-{}", dash_encode(&rel))
}

/// pi coding agent: sessions/--<encoded-cwd>-- ('/' '\' ':' -> '-')
pub fn pi_bucket(path: &str) -> String {
    let p = path.trim_start_matches('/');
    let enc: String = p
        .chars()
        .map(|c| matches!(c, '/' | '\\' | ':').then(|| '-').unwrap_or(c))
        .collect();
    format!("--{}--", enc)
}

/// Factory droid: realpath, only slashes become dashes (dots/underscores
/// survive), leading '-' prepended.
pub fn droid_bucket(path: &str) -> String {
    let p = canonical_str(path);
    let trimmed = p.trim_matches('/');
    let enc: String = trimmed
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    format!("-{}", enc)
}

/// iflow ProjectNameGenerator::fromPath(): strip leading '/', map
/// separators/whitespace to '-', non-[word-_.] to '-', prepend '-' when
/// missing, collapse consecutive dashes.
pub fn iflow_bucket(path: &str) -> String {
    let s = path.trim_start_matches('/');
    let mut out = String::new();
    for c in s.chars() {
        if c == '/' || c == '\\' || c == ':' || c.is_whitespace() {
            out.push('-');
        } else if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    if !out.starts_with('-') {
        out.insert(0, '-');
    }
    // collapse runs of '-'
    let mut collapsed = String::new();
    let mut prev_dash = false;
    for c in out.chars() {
        if c == '-' {
            if !prev_dash {
                collapsed.push(c);
            }
            prev_dash = true;
        } else {
            collapsed.push(c);
            prev_dash = false;
        }
    }
    if collapsed.is_empty() {
        "-unnamed-project".to_string()
    } else {
        collapsed
    }
}

fn canonical_str(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// All derived old->new token pairs for a path rename, besides the raw
/// path itself: encoded directory names and path hashes that agents store
/// as values or directory names (windsurf keys its context_state/database
/// dirs by md5 of the path).
pub fn derived_tokens(old: &str, new: &str) -> Vec<(String, String)> {
    let candidates = vec![
        (dash_encode(old), dash_encode(new)),
        (dash_encode_nolead(old), dash_encode_nolead(new)),
        (sha256_hex(old), sha256_hex(new)),
        (sha256_16(old), sha256_16(new)),
        (md5_hex(old), md5_hex(new)),
        (zcode_memory_key(old), zcode_memory_key(new)),
    ];
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (a, b) in candidates {
        if !a.is_empty() && a != b && seen.insert(a.clone()) {
            out.push((a, b));
        }
    }
    out
}
