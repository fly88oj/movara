# Movara

**[English](README.md)** | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

Portable workspace state for AI coding agents.

```
~/abc  --renamed-->  ~/cba
  └─ every agent's records of /home/me/abc  --movara-->  /home/me/cba
```

## Why

Most AI coding agents (Claude Code, Codex, Gemini CLI family, OpenCode, omp,
Cursor, Windsurf, …) key their session history by **project path**: directory
names are some encoding of the path (dashes / sha256 / md5), and `cwd` values
live inside JSON, JSONL, SQLite and protobuf files. Move or rename the
directory and the old sessions "disappear" — they are still on disk, just
keyed to a path that no longer exists. `movara` moves the directory and
re-keys all of those references to the new path in one shot, with full undo.

## Install

Rust implementation, no runtime dependencies, runs on Linux /
macOS / Windows:

```bash
# prebuilt binaries (tar.gz / zip) and native packages (.deb / .rpm / .dmg)
# are attached to every tagged release by GitHub Actions
#   https://github.com/fly88oj/movara/releases

# from source
cargo install --path .        # provides the `movara` binary
cargo install movara        # once published to crates.io
```

The UI language follows the system locale automatically (English, 简体中文,
日本語, 한국어, Español, Français, Deutsch, Português); override with
`--lang` or `MOVARA_LANG`.

## Usage

```bash
# the everyday case: use it instead of mv — move the directory AND
# migrate all agent history in one command
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # dst is an existing dir: move into it (mv semantics)
movara mv --dry-run ~/works/abc ~/works/cba

# optional shorthand for muscle memory (add to your shell rc):
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# see which agents reference a path
movara scan --from ~/works/abc

# migrate only (directory already moved)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# undo everything
movara backups
movara undo --id 20260903-131427-644777
```

`movara` behaviour: scan and show which agent state references the old
path → confirm → `mv` the directory (cross-filesystem falls back to
copy+remove) → migrate every agent → print a report with the undo id. If
anything goes wrong, a single `movara undo` restores both the directory
location and all agent state. Only directories are accepted (files carry no
agent history — use plain `mv`); existing targets, missing target parents
and src == dst are refused.

| option | meaning |
|---|---|
| `--agents claude,codex` | limit to specific agents (default: all installed) |
| `--extra-root PATH` | also rewrite an arbitrary tree (dotfiles, IDE configs); repeatable |
| `--deep` | also rewrite path mentions inside chat content / logs (default: identity fields only — cwd, directory, project, …) |
| `--backup-dir DIR` | backup directory (default `~/.movara/backups`) |
| `--dry-run` | report only |
| `--yes` | skip the confirmation prompt (mv, migrate) |
| `--json` | emit a single JSON document on stdout (machine-readable) |

## Safety

- **Boundary-aware replacement**: `/a/abc` never matches `/a/abc2` or
  `/a/abc-def`; `file://` URIs, JSON escaping and sub-paths
  (`/a/abc/sub`) are all handled.
- **Derived tokens are replaced too**: full sha256 (gemini `projectHash`,
  qwen/iflow tmp dirs), sha256[:16] (zcode memory keys), md5 (windsurf
  context_state/database dirs), and every vendor's dash-encoded directory
  name.
- Full backup before each migration: changed files and SQLite databases are
  copied (after a `wal_checkpoint`), renames are journaled, and `undo`
  restores everything; directory moves made by `movara` are undone as well.
- Renames are skipped when the target exists; `--from /` is refused.
- Close the agents you are migrating (WAL databases get a warning but are
  not corrupted).

## Supported agents

| agent | state location | path key |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | dash dir + `projects` keys + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | dash dir + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | own encoding + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | session/project/workspace directory columns |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | home-relative dash bucket + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/file:// URIs + composerData |
| Windsurf | `~/.codeium/windsurf/` + IDE state.vscdb | md5(path) + file:// URIs |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | same as VS Code forks |
| Crush | `<project>/.crush/crush.db` + global projects.json | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, slashes only |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | file:// URI + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | `--encoded--` bucket + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | absolute paths in config |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | dir MRU + filename hash |

Not supported (by design):

- **GitHub Copilot CLI** — local schema unpublished, cloud is authoritative.
- **Amp** — threads live server-side.
- **claude-code-router** — no path-keyed state (verified from source).

## Contributing

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # 32 tests must pass
cargo clippy --all-targets -- -D warnings # no warnings
cargo fmt --all -- --check
pre-commit install                        # local hooks (same as CI)
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the adapter guide, commit
conventions and setup; [CHANGELOG.md](CHANGELOG.md) for release history;
[SECURITY.md](SECURITY.md) for reporting security issues;
[docs/research.md](docs/research.md) for per-agent storage formats,
encodings and sources.

## License

Copyright (C) 2026 Movara contributors.

This program is free software: you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option)
any later version. See [LICENSE](LICENSE).
