// SPDX-License-Identifier: MIT OR Apache-2.0
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
pub(crate) fn is_name_byte(b: u8) -> bool {
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
        // cross-separator rules add the FOUR path forms agents actually
        // store: raw, forward-slash, JSON-escaped (serde_json escapes
        // `\` but never `/`), and the msys/Git-Bash form Claude Code's
        // own matcher normalizes to. Same-style pairs keep each form
        // (form-aware replacement: same-style keeps forms, cross-style
        // targets the raw form — see form_variants)
        let (vo, vn) = Self::form_variants(&old, &new);
        for (o, n) in std::iter::zip(vo, vn) {
            pairs.push((o, n));
        }
        pairs.sort_by_key(|(a, _)| std::cmp::Reverse(a.len()));

        let needles: Vec<Vec<u8>> = pairs.iter().map(|(a, _)| a.as_bytes().to_vec()).collect();

        Ok(ReplaceSpec {
            old,
            new,
            pairs,
            needles,
        })
    }

    /// an empty spec that replaces nothing (import without `--rebase`)
    pub fn identity() -> Self {
        ReplaceSpec {
            old: String::new(),
            new: String::new(),
            pairs: Vec::new(),
            needles: Vec::new(),
        }
    }

    /// the raw old->new rules this spec was built from (journal metadata)
    pub fn rule(&self) -> (String, String) {
        (self.old.clone(), self.new.clone())
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
                    // component boundaries on BOTH sides: a token is a
                    // whole path/bucket/hash, never a mid-fragment of a
                    // longer name (so /a/abc never matches /tmp/a/abc,
                    // and a short rebase rule cannot fire inside a longer
                    // unrelated path)
                    if end <= bytes.len()
                        && &bytes[i..end] == needle.as_slice()
                        && (i == 0 || !is_name_byte(bytes[i - 1]))
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

    /// the FOUR path-form variants agents actually store, as needle→needle
    /// pairs. Only emitted when either endpoint has a drive-letter prefix
    /// (any separator form): on POSIX-only rules the variants would be
    /// no-ops or, worse, match a literal `/c/...` directory that is not a
    /// Windows alias. Same-style (Windows↔Windows) rules map each form to
    /// its OWN form; cross-style maps every form into the target's raw
    /// form. Derived encodings (dash/sha256/…) hash the RAW path only —
    /// they are identity, not needles, and stay outside this table.
    fn form_variants(old: &str, new: &str) -> (Vec<String>, Vec<String>) {
        let drive = |p: &str| {
            let b = p.as_bytes();
            b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
        };
        if !drive(old) && !drive(new) {
            return (vec![], vec![]);
        }
        let fwd = |p: &str| p.replace('\\', "/");
        let esc = |p: &str| p.replace('\\', "\\\\");
        let msys = |p: &str| {
            let f = fwd(p);
            // C:/x -> /c/x (drive letter lowercased, colon dropped)
            let b = f.as_bytes();
            if b.len() >= 2 && b[1] == b':' {
                format!("/{}{}", (b[0] as char).to_ascii_lowercase(), &f[2..])
            } else {
                f
            }
        };
        let same_style = drive(old) && drive(new);
        let mut olds = vec![fwd(old), esc(old), msys(old)];
        let mut news = if same_style {
            vec![fwd(new), esc(new), msys(new)]
        } else {
            vec![new.to_string(); 3]
        };
        olds.retain(|o| o != old);
        news.truncate(olds.len());
        news.resize(olds.len(), new.to_string());
        (olds, news)
    }

    /// like_pattern() for SQLite pre-filters (single pair)
    pub fn like_pattern(&self) -> String {
        format!("%{}%", self.old)
    }

    /// LIKE pre-filter patterns matching BOTH the raw and the
    /// JSON-escaped form (Windows paths stored in JSON columns carry
    /// doubled backslashes; the single-pattern form misses them). Use
    /// with `LIKE ? OR LIKE ?` call sites.
    pub fn like_patterns(&self) -> Vec<String> {
        let raw = format!("%{}%", self.old);
        if self.old.contains('\\') {
            vec![raw, format!("%{}%", self.old.replace('\\', "\\\\"))]
        } else {
            vec![raw]
        }
    }
}

/// Parse and validate `--rebase OLD:NEW` rules. Refused: `/` sources,
/// identity pairs, duplicate sources, and chained rules whose source
/// lies under another rule's target. Returned sorted longest-source-
/// first so that sequential single-pair passes (import) resolve prefix
/// overlaps correctly.
fn split_rule(r: &str) -> Result<(&str, &str)> {
    let bytes = r.as_bytes();
    let mut fallback: Option<(&str, &str)> = None;
    for (i, b) in bytes.iter().enumerate() {
        if *b != b':' {
            continue;
        }
        let right = &r[i + 1..];
        if right.is_empty() {
            continue;
        }
        let root_start = right.starts_with('/')
            || (right.len() >= 3
                && right.as_bytes()[1] == b':'
                && right.as_bytes()[0].is_ascii_alphabetic()
                && (right.as_bytes()[2] == b'/' || right.as_bytes()[2] == b'\\'));
        if root_start && !r[..i].is_empty() {
            return Ok((&r[..i], right));
        }
        fallback = Some((&r[..i], right));
    }
    fallback.ok_or_else(|| anyhow::anyhow!("{}", t!("spec.err_rule", rule = r)))
}

pub fn prepare_rules(raw: &[String]) -> Result<Vec<(String, String)>> {
    let mut rules: Vec<(String, String)> = Vec::new();
    for r in raw {
        let (o, n) = split_rule(r)?;
        let op = absolutish(Path::new(o));
        let np = absolutish(Path::new(n));
        if op == Path::new("/") {
            anyhow::bail!("{}", t!("spec.err_root"));
        }
        let (os, ns) = (crate::ctx::path_str(&op), crate::ctx::path_str(&np));
        if os == ns {
            anyhow::bail!("{}", t!("spec.err_rule_identity", rule = r.as_str()));
        }
        if rules.iter().any(|(ro, _)| *ro == os) {
            anyhow::bail!("{}", t!("spec.err_rule_dup", rule = os.as_str()));
        }
        rules.push((os, ns));
    }
    // no rule's source may lie under another rule's target AND no rule's
    // target may land under another rule's source — both directions make
    // the sequential (longest-first) application order-dependent
    for (o, n) in &rules {
        for (o2, n2) in &rules {
            let chained = (o == n2 || o.starts_with(&format!("{}/", n2)))
                || (n == o2 || n.starts_with(&format!("{}/", o2)));
            if chained {
                anyhow::bail!(
                    "{}",
                    t!("spec.err_rule_chain", old = o.as_str(), new = n.as_str())
                );
            }
        }
    }
    rules.sort_by_key(|(o, _)| std::cmp::Reverse(o.len()));
    Ok(rules)
}

#[inline]
pub(crate) fn utf8_char_len(first_byte: u8) -> usize {
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
    // a drive-letter prefix (C:\ or C:/) is absolute on Windows even
    // when parsed on a POSIX host, and a leading-/ POSIX path stays
    // itself even when parsed on Windows (the engines compare path
    // STRINGS; cwd-joining either corrupts the rule — the first
    // Windows CI run exposed the POSIX half)
    let s_opt = expanded.to_str();
    let drive_abs = s_opt.is_some_and(|s| {
        s.len() >= 2 && s.as_bytes()[1] == b':' && s.as_bytes()[0].is_ascii_alphabetic()
    });
    let posix_abs = s_opt.is_some_and(|s| s.starts_with('/'));
    if posix_abs {
        // keep the POSIX string verbatim: normalize() round-trips
        // through PathBuf, which flips separators to \\ on Windows and
        // the needle would no longer match the POSIX-stored path
        expanded
    } else if expanded.is_absolute() || drive_abs {
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
        // left boundary: a token never matches when a name byte runs
        // directly into it — "/tmp/p/abc" and "xp/abc" are different,
        // longer paths whose shared suffix must stay untouched
        let s = spec.replace("/tmp/p/abc");
        assert_eq!(s, "/tmp/p/abc", "token must not match mid-name");
        let s = spec.replace("xp/abc");
        assert_eq!(s, "xp/abc", "name byte before token blocks the match");
        // empty / no-match strings unchanged
        assert_eq!(spec.replace(""), "");
        assert_eq!(spec.replace("no tokens here"), "no tokens here");
    }

    #[test]
    fn replace_is_linear_on_large_input() {
        // a pathological input that would blow a backtracking regex
        let spec = ReplaceSpec::new("/p/abc", "/p/cba").unwrap();
        // the repeated string is ONE long path: only the leading token
        // has a left boundary, every inner "/p/abc" is preceded by the
        // previous component's 'c' and must stay (inner components are
        // different directories that did not move)
        let line = "/p/abc".repeat(50_000);
        let out = spec.replace(&line);
        assert_eq!(out, format!("{}{}", "/p/cba", "/p/abc".repeat(49_999)));
        let near = format!("{}2", "/p/abc".repeat(50_000));
        assert_eq!(
            spec.replace(&near),
            format!("{}{}2", "/p/cba", "/p/abc".repeat(49_999))
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
