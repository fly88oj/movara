// SPDX-License-Identifier: GPL-3.0-or-later
//! The compiled old->new replacement specification for a path rename.

use crate::encodings;
use anyhow::Result;
use memchr::memmem;
use rust_i18n::t;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ReplaceSpec {
    pub old: String,
    pub new: String,
    /// old->new pairs sorted longest-first (path first, then derived tokens)
    pairs: Vec<(String, String)>,
    needles: Vec<Vec<u8>>,
}

/// bytes that may extend a path component: a token match is only valid when
/// the next byte is NOT one of these (so /a/abc never matches inside
/// /a/abc2 or /a/abc-def, while /a/abc/sub, "…/abc\"" and file:///a/abc do)
#[inline]
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

impl ReplaceSpec {
    /// `old == new` is allowed (used by `scan` without `--to`: the spec is
    /// then a read-only no-op probe); commands that change state reject
    /// identical paths themselves.
    pub fn new(old: &str, new: &str) -> Result<Self> {
        let old_p = absolutish(Path::new(old));
        let new_p = absolutish(Path::new(new));
        if old_p == Path::new("/") {
            anyhow::bail!("{}", t!("spec.err_root"));
        }
        let old = crate::ctx::path_str(&old_p);
        let new = crate::ctx::path_str(&new_p);

        let mut pairs: Vec<(String, String)> = vec![(old.clone(), new.clone())];
        pairs.extend(encodings::derived_tokens(&old, &new));
        pairs.sort_by_key(|(a, _)| std::cmp::Reverse(a.len()));

        let needles: Vec<Vec<u8>> = pairs.iter().map(|(a, _)| a.as_bytes().to_vec()).collect();

        Ok(ReplaceSpec {
            old,
            new,
            pairs,
            needles,
        })
    }

    /// Boundary-aware replace inside a string.
    ///
    /// Implemented as a single left-to-right scan trying each token
    /// longest-first at every position: linear time, no backtracking (a
    /// regex with a negative lookahead over several long alternatives
    /// exceeds backtracking limits on large real-world chat lines).
    pub fn replace(&self, s: &str) -> String {
        // specs built with old == new (scan probes) replace nothing
        if self.pairs.is_empty() || self.pairs[0].0 == self.pairs[0].1 {
            return s.to_string();
        }
        let bytes = s.as_bytes();
        // tokens start with '/', '-' or a hex digit — skip everything else
        // cheaply before memcmp-ing the (few) needles
        let firsts: Vec<u8> = self
            .needles
            .iter()
            .map(|n| n[0])
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if firsts.contains(&b) {
                let mut matched = None;
                for (idx, needle) in self.needles.iter().enumerate() {
                    let end = i + needle.len();
                    if end <= bytes.len()
                        && &bytes[i..end] == needle.as_slice()
                        && !bytes.get(end).is_some_and(|nb| is_name_byte(*nb))
                    {
                        matched = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = matched {
                    out.push_str(&self.pairs[idx].1);
                    i += self.needles[idx].len();
                    continue;
                }
            }
            // copy one UTF-8 character so multi-byte text stays intact
            let ch_len = utf8_char_len(b);
            let end = (i + ch_len).min(bytes.len());
            out.push_str(&s[i..end]);
            i = end;
        }
        out
    }

    /// Fast pre-filter: does this blob contain any old token at all?
    pub fn maybe_contains(&self, data: &[u8]) -> bool {
        self.needles.iter().any(|n| memmem::find(data, n).is_some())
    }

    pub fn like_pattern(&self) -> String {
        format!("%{}%", self.old)
    }
}

#[inline]
fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // invalid continuation byte in isolation: advance by one
    }
}

/// Expand `~`, make absolute against the CWD and normalize `.`/`..`
/// components without resolving symlinks (agents store the path as
/// invoked); the single home of this logic for CLI args and specs alike.
pub fn absolutish(p: &Path) -> PathBuf {
    let expanded: PathBuf = if let Some(s) = p.to_str() {
        if let Some(rest) = s.strip_prefix("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(rest)
        } else {
            p.to_path_buf()
        }
    } else {
        p.to_path_buf()
    };
    if expanded.is_absolute() {
        normalize(&expanded)
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        normalize(&cwd.join(expanded))
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_handles_multibyte_and_boundaries() {
        let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
        // multibyte text around the token stays intact
        let s = spec.replace("日本語 /p/abc 中文 /p/abc2 /p/abc-def");
        assert_eq!(s, "日本語 /p/cba 中文 /p/abc2 /p/abc-def");
        // token at end of string matches (no boundary byte follows)
        let s = spec.replace("/p/x//p/abc");
        assert_eq!(s, "/p/x//p/cba");
        // empty / no-match strings unchanged
        assert_eq!(spec.replace(""), "");
        assert_eq!(spec.replace("no tokens here"), "no tokens here");
    }

    #[test]
    fn replace_is_linear_on_large_input() {
        // a pathological input that would blow a backtracking regex
        let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
        let line = "/p/abc".repeat(50_000);
        let out = spec.replace(&line);
        assert_eq!(out, "/p/cba".repeat(50_000));
        // a trailing name byte shields the LAST occurrence only: the ones
        // followed by "/" are legitimate sub-path boundaries and DO change
        let near = format!("{}2", "/p/abc".repeat(50_000));
        assert_eq!(
            spec.replace(&near),
            format!("{}{}2", "/p/cba".repeat(49_999), "/p/abc")
        );
        // the shielded token alone stays untouched
        assert_eq!(spec.replace("/p/abc2"), "/p/abc2");
    }

    #[test]
    fn identical_paths_are_a_noop_probe() {
        let spec = ReplaceSpec::new("/p/abc", "/p/abc").unwrap();
        assert_eq!(spec.replace("/p/abc"), "/p/abc");
    }
}
