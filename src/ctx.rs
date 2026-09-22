// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cross-platform resolution of agent state roots.
//!
//! Linux keeps the classic layout the adapters were verified against
//! (`~/.config`, `~/.local/share`); on macOS/Windows the `dirs` crate
//! equivalents are used (`~/Library/Application Support`,
//! `%APPDATA%`, ...). `MOVARA_HOME` redirects everything for tests and
//! sandboxes without touching the real user home.

use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Ctx {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    /// the Windows local (non-roaming) data root, when pinned; None
    /// means "resolve lazily from the environment" (real-machine
    /// semantics). Tests construct Ctx literally and MUST pin this —
    /// otherwise parallel fixtures on Windows would collide inside the
    /// REAL %LOCALAPPDATA%.
    pub data_local: Option<PathBuf>,
}

/// which state-root family a path belongs to; archives carry this so a
/// member lands under the RIGHT root on a foreign OS (`.config/X` on
/// Linux is `%APPDATA%\X` on Windows — home-relative alone is
/// many-to-one and cannot be inverted)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RootKind {
    Home,
    Config,
    Data,
    #[serde(rename = "datalocal")]
    DataLocal,
}

impl RootKind {
    /// the archive member segment for this kind (format 2)
    pub fn segment(self) -> &'static str {
        match self {
            RootKind::Home => "home",
            RootKind::Config => "config",
            RootKind::Data => "data",
            RootKind::DataLocal => "datalocal",
        }
    }

    pub fn from_segment(s: &str) -> Option<Self> {
        match s {
            "home" => Some(RootKind::Home),
            "config" => Some(RootKind::Config),
            "data" => Some(RootKind::Data),
            "datalocal" => Some(RootKind::DataLocal),
            _ => None,
        }
    }
}

impl Ctx {
    pub fn from_env() -> Self {
        if let Ok(h) = env::var("MOVARA_HOME") {
            let home = PathBuf::from(h);
            let config = home.join(".config");
            let data = home.join(".local").join("share");
            return Ctx {
                home,
                config_home: config,
                data_home: data.clone(),
                data_local: Some(data),
            };
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_home = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let data_home = dirs::data_dir().unwrap_or_else(|| home.join(".local").join("share"));
        Ctx {
            home,
            config_home,
            data_home,
            data_local: None,
        }
    }

    /// path under the agent-state home (`$HOME/...` on Linux)
    pub fn h(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }

    /// path under the XDG-style config root (`~/.config/...` on Linux)
    pub fn c(&self, rel: &str) -> PathBuf {
        self.config_home.join(rel)
    }

    /// path under the XDG-style data root (`~/.local/share/...` on Linux)
    pub fn d(&self, rel: &str) -> PathBuf {
        self.data_home.join(rel)
    }

    /// path under the local (non-roaming) data root; on Linux identical
    /// to `d` — the distinction only exists on Windows
    /// (`%LOCALAPPDATA%` vs `%APPDATA%`). A pinned `data_local` wins
    /// (test isolation), then MOVARA_HOME, then the real machine root.
    pub fn dl(&self, rel: &str) -> PathBuf {
        #[cfg(windows)]
        {
            if let Some(root) = &self.data_local {
                return root.join(rel);
            }
            if let Ok(h) = env::var("MOVARA_HOME") {
                return PathBuf::from(h).join(".local").join("share").join(rel);
            }
            if let Some(d) = dirs::data_local_dir() {
                return d.join(rel);
            }
        }
        self.data_home.join(rel)
    }

    /// the root of the given kind
    pub fn root_of(&self, kind: RootKind) -> PathBuf {
        match kind {
            RootKind::Home => self.home.clone(),
            RootKind::Config => self.config_home.clone(),
            RootKind::Data => self.data_home.clone(),
            RootKind::DataLocal => {
                // dl() needs a rel; use empty to get the bare root
                self.dl("")
            }
        }
    }

    /// default backup journal root (`<home>/.movara/backups`)
    pub fn default_backup_dir(&self) -> PathBuf {
        self.home.join(".movara").join("backups")
    }
}

/// strip the Windows verbatim prefix (`\\\\?\\`) that fs::canonicalize
/// returns: std I/O accepts it, but our string surgery (backup rels,
/// member names) would embed the `?` — invalid in NTFS filenames
pub fn de_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// portable archive-relative form: forward slashes + NFC normalization
/// (APFS stores names as given but compares normalized — without NFC a
/// macOS-created NFD name duplicates instead of matching on Linux)
pub fn to_portable_rel(rel: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let slashed = rel.replace('\\', "/");
    slashed.nfc().collect::<String>()
}

/// read-side shim: accept either separator (pre-1.3 Windows archives
/// carry backslash members); NFC is NOT applied on read — content
/// hashes must stay byte-faithful to what each host wrote
pub fn from_portable_rel(rel: &str) -> String {
    rel.replace('\\', "/")
}

/// the one lossy Path->String conversion used across the crate
pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
