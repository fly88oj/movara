# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

List items must each be a single source line (no hard wrapping): the
release workflow extracts this file verbatim as the GitHub Release body,
and the release page renders every newline as a forced break.

## [Unreleased]

### Added

- **`movara export` / `movara import`** — portable agent-state archives for multi-host migration: a single standard `.tar.gz` built with the pure-Rust `tar` + `flate2` crates (no system toolchain, self-contained binary). Exports scope to the whole host, `--path` project paths (repeatable, boundary-aware selection so sibling paths like `/p/abc2` never leak into a `/p/abc` export) and/or `--agents`. Import rebases paths through repeatable `--rebase OLD:NEW` rules by reusing the migration engine, verifies archive paths against the target host (`--allow-missing-path` to proceed), applies skip/replace conflict policies per file, refuses to place anything outside the selected agents' state roots (crafted archives stay contained), never replaces a shared database on a filtered exchange, and is fully reversible — the backup journal now records created paths, so `movara undo` removes imported state again.
- **Export exclusion layer**: auth/credential/token files, shell snapshots and secret-carrying configs never leave the machine; dual-purpose configs (`~/.claude.json`, `~/.codex/config.toml`, `~/.continue/config.json`) are exported as sanitized path-keyed projections and merged additively on import.

### Fixed

- Boundary-aware replacement now guards both edges of a match: a token can no longer match as a mid-fragment of a longer name, so a short rebase rule or a shared path suffix cannot corrupt longer unrelated paths.

## [1.0.0] - 2026-09-17

### Added

- **movara** — one command that moves a project directory and migrates every AI coding agent's local state with it: sessions, history and context (mv semantics incl. move-into-directory, cross-filesystem fallback for POSIX EXDEV and Windows ERROR_NOT_SAME_DEVICE, preflight confirmation, single-command undo).
- **Full CLI**: `movara mv` (one-shot move + migrate, the everyday `mv` replacement), `scan` (report every agent state location referencing a path), `migrate` (rekey agent state after a directory move), `undo` (full reversal: files, databases, renames, moved project dirs), `backups` (list migrations), `agents` (supported list); `alias mva='movara mv'` gives the shortest everyday form.
- **18 agent adapters**: Claude Code, Codex, Gemini CLI, Qwen Code, iFlow, OpenCode, omp, ZCode, Cursor (IDE + CLI), Windsurf, Antigravity, Crush, Factory Droid, Continue, pi/gsd, Zed, Aider, cc-connect — plus `--extra-root` for arbitrary trees. Each adapter implements the vendor's own path encoding: dash buckets (claude/omp/pi/droid each differ), sha256/md5 hash directories, SQLite directory columns, `file://` URIs, protobuf blobs.
- **Eight UI languages** (English, 简体中文, 日本語, 한국어, Español, Français, Deutsch, Português) with automatic locale detection: `--lang` > `MOVARA_LANG` > `LC_ALL` > `LC_MESSAGES` > `LANG` > system locale > English, with full key parity across all eight catalogs; the README ships in all eight languages too.
- **Safety model**: boundary-aware replacement (`/a/abc` never matches `/a/abc2` or `/a/abc-def`); derived hash tokens (sha256, sha256[:16], md5, vendor bucket keys) rewritten alongside the path; full backup journal before any modification (file copies, SQLite `wal_checkpoint` + whole-database backups, rename ledger); WAL-activity warning when an agent database appears still in use; refusals for existing targets, missing parents, same paths and `/`.
- **Packaging**: cargo-deb (.deb), cargo-generate-rpm (.rpm), hdiutil (.dmg), standalone tar.gz/zip, Homebrew formula template.
- **CI/CD**: GitHub Actions — `ci.yml` (fmt + clippy + tests + pre-commit hooks + Conventional Commits message gate on ubuntu/macos/windows), `release.yml` (tag-triggered; builds three targets — linux x86_64, macOS arm64, Windows x64 — and packages tar.gz/zip/deb/rpm/dmg, publishing the GitHub Release with changelog notes).
- **Governance**: pre-commit hooks (formatting, clippy, Conventional Commits message check, private-key and local-machine-info leak scanning — identical checks run in CI), EditorConfig, issue/PR templates, Contributor Covenant code of conduct, security policy.
- **Test suites**: 32 tests — synthetic fixture HOMEs replicating every agent's real storage layout (bucket renames, SQLite updates, derived hash tokens, sibling-path immunity, undo restoring byte-identically, dry-run changing nothing); robustness (malformed protobuf with overflow varints/truncations/hostile nesting, CRLF/UTF-8 JSONL, binary-level --json purity and --lang usage errors); engine unit tests (multibyte boundaries, 50k-token pathological inputs, linear-scanner edge cases).
- **Research documentation**: per-agent storage formats, encoding algorithms and source links (`docs/research.md`, bilingual en/zh-CN).
