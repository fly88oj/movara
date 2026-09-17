# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

List items must each be a single source line (no hard wrapping): the
release workflow extracts this file verbatim as the GitHub Release body,
and the release page renders every newline as a forced break.

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
