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

Rust implementation, no runtime dependencies, runs on Linux / macOS /
Windows. Prebuilt packages are attached to every
[release](https://github.com/fly88oj/movara/releases) — the file names
below use `1.2.0`; substitute the version you downloaded.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**Other Linux (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg or tar.gz)**

```bash
# open the dmg and copy bin/movara to /usr/local/bin, or:
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Intel Macs run the arm64 build under Rosetta 2 or build from source.

**Windows (zip)**

Extract `movara-1.2.0-x86_64-pc-windows-msvc.zip` and put `movara.exe`
on your `PATH`.

**From source**

```bash
cargo install --path .        # provides the `movara` binary
cargo install movara          # once published to crates.io
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

`movara mv` behaviour: scan and show which agent state references the old
path → confirm → `mv` the directory (cross-filesystem falls back to
copy+remove) → migrate every agent → print a report with the undo id. If
anything goes wrong, a single `movara undo` restores both the directory
location and all agent state. Only directories are accepted (files carry no
agent history — use plain `mv`); existing targets, missing target parents
and src == dst are refused.

### Commands

| command | purpose |
|---|---|
| `movara mv <SRC> <DST>` | move the directory (existing `DST` = move into it) and migrate every agent in one step — the everyday `mv` replacement |
| `movara scan --from <OLD> [--to <NEW>]` | read-only report of every agent state location referencing a path; `--to` only adds rename-target previews |
| `movara migrate --from <OLD> --to <NEW>` | rekey agent state after the directory was already moved by other means; `--move-project` moves the directory first |
| `movara undo --id <ID>` | fully reverse one migration (see below) |
| `movara backups` | list migration journals (see below) |
| `movara agents` | list supported agents and whether each is installed |
| `movara export [--path PATH]... [--agents LIST]` | write a portable `.tar.gz` archive of agent state — whole host, or filtered by project path / agents |
| `movara import <ARCHIVE> [--rebase OLD:NEW]...` | restore an archive on this host, rebasing paths; journaled like a migration, so `movara undo` reverses it |
| `movara move <SRC> [user@]host:<DST>` | move a project AND its agent state to another host over ssh in one command — memory rides along, cleanup optional (no IPv6 host literals) |
| `movara receive --dst <DST>` | [target side] import the streamed archive from stdin (spawned by `move`) |

### Options

| option | applies to | meaning |
|---|---|---|
| `--lang CODE` | global | override the display language (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LIST` | scan, migrate, mv | comma list; limit to specific agents (default: all installed) |
| `--extra-root PATH` | scan, migrate, mv | also rewrite an arbitrary tree (dotfiles, IDE configs); repeatable |
| `--deep` | migrate, mv | also rewrite path mentions inside chat content / logs (default: identity fields only — cwd, directory, project, …) |
| `--backup-dir DIR` | migrate, mv, undo, backups | backup journal root (default `~/.movara/backups`) |
| `--dry-run` | migrate, mv | report only, change nothing |
| `--yes` | migrate, mv | skip the confirmation prompt |
| `--move-project` | migrate | move the project directory itself before rekeying |
| `--json` | agents, scan, migrate, mv, backups | emit a single JSON document on stdout (machine-readable) |
| `--out FILE` | export | archive path (default `movara-export-<timestamp>.tar.gz`) |
| `--path PATH` | export | only state referencing this project path (repeatable; intersects `--agents`) |
| `--rebase OLD:NEW` | import | path mapping, repeatable; overlapping and chained rules are refused |
| `--dst <DST>` | receive | destination directory for the project on this host |
| `--plan-only` | receive | preflight the destination, then exit |
| `--yes` | receive | non-interactive (required when streaming) |
| `--on-conflict POLICY` | import | `skip` (default) or `replace` existing local state |
| `--allow-missing-path` | import | proceed when archive paths resolve to nothing locally |
| `--state-only` | move | carry agent state + project memory, not the code |
| `--cleanup` | move | remove the moved state set on this host after verified success (shared dbs/configs stay) |

### Undo & backups

Every `mv` / `migrate` writes a journal under `~/.movara/backups/<id>/`
before changing anything: copies of each file about to change, whole
SQLite databases (after a `wal_checkpoint`), and a ledger of renames
including the moved project directory.

- **`movara backups`** lists journals — id, date, old → new path, agents
  touched, file/database/rename counts (`--json` for scripts). The
  journal is the unit of undo: filtering by path or agent is not
  supported today; delete old journals by hand to reclaim disk space.
- **`movara undo --id <ID>`** replays one journal backwards — file
  contents return, databases swap back, renames reverse, and the moved
  directory goes home. Use it when a migration targeted the wrong path,
  an agent was still running during the move, or you simply want the
  old layout back. Undo is all-or-nothing per migration: one id reverts
  that entire migration, not a single agent or file, and it should run
  before a new migration of the same paths.

Exchanges via `export` / `import` are journaled the same way: `undo` reverses an import completely, including state it created.
A cross-host `move` adds the project tree (and `.git`) to the exchange: `--state-only` drops the code but keeps in-project memory files (CLAUDE.md, AGENTS.md, rules); `--cleanup` deletes only the moved state set on the source — never shared databases or configs — and is itself reversible.

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
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | `wd_<basename>_<sha256[:12]>` bucket dirs+files + workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + legacy `*.jsonl`, `~/.config/goose/` | `sessions.working_dir` + `working_dir` metadata + permission keys |
| Cline / Roo Code / Kilo Code | `~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code}` | task `path` fields + `workspace`/`cwdOnTaskInitialization` + checkpoints `core.worktree` + cwdHash/sha256 buckets |
| OpenHands | `~/.openhands/` | `working_dir` + `projects/<sha256(realpath)>/` |
| Codebuff / Freebuff | `~/.config/manicode/projects/<basename>/` | bare-basename bucket + run-state `cwd` |
| gptme | `~/.local/share/gptme/logs/<date>-<name>/` | `config.toml [chat] workspace` (tilde form too) + `workspace` symlink + `files` lists |

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
> **Cross-host sync is under development** for the next release —
> see the Unreleased section of [CHANGELOG.md](CHANGELOG.md).

conventions and setup;
> **Cross-host sync is under development** for the next release —
> see the Unreleased section of [CHANGELOG.md](CHANGELOG.md).
 [CHANGELOG.md](CHANGELOG.md) for release history;
[SECURITY.md](SECURITY.md) for reporting security issues;
[docs/research.md](docs/research.md) for per-agent storage formats,
encodings and sources.

## License

Copyright (C) 2026 Movara contributors.

Licensed under either of Apache License, Version 2.0 or MIT license, at
your option. See [LICENSE-APACHE](LICENSE-APACHE) and
[LICENSE-MIT](LICENSE-MIT).
