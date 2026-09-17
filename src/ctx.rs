// SPDX-License-Identifier: GPL-3.0-or-later
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
                data_home: data,
            };
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_home = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let data_home = dirs::data_dir().unwrap_or_else(|| home.join(".local").join("share"));
        Ctx {
            home,
            config_home,
            data_home,
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

    /// default backup journal root (`<home>/.movara/backups`)
    pub fn default_backup_dir(&self) -> PathBuf {
        self.home.join(".movara").join("backups")
    }
}

/// the one lossy Path->String conversion used across the crate
pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
