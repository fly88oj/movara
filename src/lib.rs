// SPDX-License-Identifier: MIT OR Apache-2.0
//! movara: migrate AI-agent session/config references when a project
//! directory moves or is renamed.
//!
//! Library layout:
//! - [`ctx`]: cross-platform state-directory resolution (home / XDG config /
//!   XDG data), overridable for tests and sandboxes via `MOVARA_HOME`.
//! - [`encodings`]: per-vendor directory-name encodings and path hashes.
//! - [`spec`]: the compiled old->new replacement specification
//!   (boundary-aware, includes derived hash tokens).
//! - [`backup`]: change journal and full undo.
//! - [`rewriters`]: text / JSON / JSONL file rewriting.
//! - [`sqlite`] / [`protobuf`]: database and protobuf-blob rewriting.
//! - [`adapters`]: one module per agent family.
//! - [`i18n`]: locale detection (en, zh-CN, ja, ko, es, fr, de, pt-BR).
//! - [`cli`]: the `movara` command line.

pub mod adapters;
pub mod archive;
pub mod backup;
pub mod cli;
pub mod ctx;
pub mod encodings;
pub mod i18n;
pub mod protobuf;
pub mod rewriters;
pub mod spec;
pub mod sqlite;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

rust_i18n::i18n!("locales", fallback = "en");
