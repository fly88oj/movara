# Research: how AI agents key local session state by project path

Research date: 2026-09-03. Method: hands-on inspection of a Linux workstation
(Linux; Claude Code 2.1.x, Codex 0.122, qwen-code 0.21, iflow 0.5.x,
opencode 1.1.36, omp 18, Cursor, Windsurf, Antigravity, zed, Factory agent,
pi 0.80, plus crush config) cross-checked against GitHub sources and
official docs (sources per section). Unless marked "unverified", every
encoding/hash algorithm below was verified bidirectionally against real
local data. A Chinese version is available at `docs/research.zh-CN.md`.

## 1. Agents keyed by encoded directory names

### Claude Code
- Sessions: `~/.claude/projects/<encoded>/<sessionId>.jsonl`; every
  message record carries `cwd`.
- Encoding: **every non-alphanumeric character of the path becomes `-`**
  (`/`, `.`, `_` all count) — lossy and irreversible;
  `.paperclip` → `--paperclip`.
- `~/.claude.json` top-level `projects` map uses the **raw absolute path**
  as the key (the authoritative mapping).
- `~/.claude/history.jsonl` has a `project` field per line.
- Migration: rename the projects subdir (mind the `-` prefix ambiguity),
  rewrite the `projects` keys, `history.project`, and the `cwd` fields
  inside the jsonl (content mentions need `--deep`).
- Sources: code.claude.com/docs/en/claude-directory; issues #1516
  (official workaround is literally renaming the encoded dir), #18829
  (encoding ambiguity); harnez.ai/posts/fix-broken-project-paths.

### omp (Oh My Pi) — a pi fork
- Session bucket: `~/.omp/agent/sessions/<encoded-cwd>/`. Encoding
  (official docs/session.md): the **canonicalized (realpath) cwd**;
  paths under home encode relative to home, everything else uses the full
  path; `/`→`-`. Example: `-works-foo` (not `-home-u-works-foo`).
- Header `{"type":"session","cwd":...}`; `history.db` has a
  `history.cwd` column.
- Sources: github.com/can1357/oh-my-pi/blob/main/docs/session.md,
  packages/coding-agent/src/session/history-storage.ts.

### pi coding agent
- Session bucket: `~/.pi/agent/sessions/--<encoded>--/` with `/ \ :`→`-`,
  wrapped in double `--`.
- `~/.pi/agent/projects-memory/<basename>/` comes from third-party memory
  extensions (pi-hermes-memory et al.), not the core.
- Sources: github.com/earendil-works/pi
  packages/coding-agent/src/core/session-manager.ts.

### Factory Droid (agent)
- Session bucket: `~/.factory/sessions/<encoded>/`. Encoding (binary
  reverse-engineering, v0.211): expand `~` → resolve → **realpath when the
  path exists** → strip leading/trailing `/` → collapse `/` runs into one
  `-` → prepend `-`. **Only slashes are replaced; `.`/`_` survive**
  (unlike Claude).
- Sources: docs.factory.ai/droid-cli/settings; embedded binary docs.

### Qwen Code
- Sessions: `~/.qwen/projects/<sanitize(cwd)>/chats/*.jsonl`;
  `sanitize = cwd.replace(/[^a-zA-Z0-9]/g,'-')` (lowercased first on
  Windows).
- Temp/checkpoints: `~/.qwen/tmp/<sha256(cwd)>/`.
- Ownership filter: the first record's `cwd` is read and the session only
  shows when `sha256(recordCwd)===sha256(currentCwd)` — **renaming the
  bucket alone hides the session; both must change**.
- Sources: github.com/QwenLM/qwen-code packages/core/src/utils/paths.ts,
  services/sessionService.ts.

### iFlow CLI
- Sessions: `~/.iflow/projects/<fromPath(cwd)>/session-<uuid>.jsonl`;
  `fromPath`: strip leading `/`, map `\ / : \s`→`-`, non-`[\w\-_.]`→`-`,
  prepend `-` when missing, **collapse consecutive dashes**.
- `tmp/ history/ cache/ snapshots/` are keyed by `sha256(projectRoot)`.
- Note: 0.5.x `-p` non-interactive mode persists no session (verified
  locally).
- Sources: npm @iflow-ai/iflow-cli bundle reverse-engineering; official
  checkpointing docs.

### Cursor CLI
- `~/.cursor/projects/<encoded>/`: strip the leading `/`, then `/`→`-`,
  **no leading dash** (`home-user-...` — unlike Claude's
  `-home-...`).
- Newer CLIs add `~/.cursor/chats/*/*/store.db` (not present on this
  machine's version).
- Sources: agentgrep.org/backends/cursor-cli; local verification.

## 2. Agents keyed by hashes

| agent | hash | used in | verified |
|---|---|---|---|
| Gemini CLI | `sha256(cwd)` hex | `projectHash` field in chats json | ✅ byte-compared |
| Qwen / iFlow | `sha256(cwd)` | `tmp/` (iFlow: history/cache/snapshots too) | ✅ |
| zcode | `sha256(cwd)[:16]` | `~/.zcode/cli/memories/projects/<basename>-<hash16>` | ✅ |
| Windsurf | `md5(path without file:// scheme)` | `~/.codeium/windsurf/context_state|database/<32hex>`, `cachedWorkspaceInfosResponse:<hash>` keys in state.vscdb | ✅ (multi-root workspaces hash the workspace.json path — unsupported) |
| cc-connect | first 4 bytes of `sha256(workDir)` = 8 hex | `sessions/<project>_<hash>.json` filenames | source-verified |
| OpenCode | `sha1("git-remote:"+normalizedURL)` / root commit / `"global"` | `project.id` (**path-independent**; id survives a pure rename) | ✅ local ids match neither sha1 nor sha256 of the path |

## 3. Agents keyed by database columns

- **Codex**: first line of `sessions/**/rollout-*.jsonl` is
  `session_meta.payload.cwd` (turn_context/world_state embed cwd in XML —
  content layer); 0.147+ `state_*.sqlite` `threads.cwd` (the resume
  picker filters on the current cwd; `--all` disables); config.toml
  `[projects."<path>"]` trust entries.
- **OpenCode**: opencode.db `project.worktree`, `session.directory/path`,
  `workspace.directory`, `project_directory`; `event.data`/`message.data`
  JSON blobs embed the directory; startup refresh deletes
  `project_directory` rows whose dir is missing (so UPDATE is required,
  not self-healing).
- **zcode**: db.sqlite `session.directory/path`, `workflow_run.cwd`;
  agents/exec/artifacts metadata.json carries the workspace.
- **omp**: history.db `history.cwd`.
- **Zed**: `~/.local/share/zed/threads/threads.db`
  `threads.folder_paths` (newline-joined sorted absolute paths);
  `db/0-stable/db.sqlite` `sidebar_threads.folder_paths/
  main_worktree_paths` (JSON arrays), `trusted_worktrees.absolute_path`;
  message bodies live in zstd-compressed blobs (left alone).
- **Continue**: sessions/*.json top-level `workspaceDirectory`
  (`file://` URI); index.sqlite `tag_catalog.dir`.
- **Cursor IDE**: `~/.config/Cursor/User/globalStorage/state.vscdb`
  `ItemTable.value` (JSON strings) + `cursorDiskKV.value`
  (`composerData:`/`bubbleId:` rows carry `fsPath` and `file://` URIs);
  `workspaceStorage/<id>/workspace.json` `folder`. The workspaceStorage
  directory name is an internal VS Code id (md5/sha1/sha256 × 7 variants
  tested, none match) → rewrite contents only; chat history lives in
  globalStorage and re-associates via the URI rewrite.
- **Windsurf IDE**: the ItemTable key `codeium.windsurf` holds
  `windsurf.workspaceCascadeMap:{"file:///<path>":"<cascade-uuid>"}`
  — the session↔workspace map (the migration-critical bit);
  `cascade/*.pb` are encrypted in current versions (high entropy, no
  plaintext paths — verified with strings), so community tools for older
  versions no longer apply.

## 4. No path key / moves with the project

- **Crush**: session db at `<project>/.crush/crush.db` (sessions table has
  no cwd column; ownership is physical location); only the global
  `~/.local/share/crush/projects.json` (`path`/`data_dir`) needs editing.
- **Aider**: `.aider*` history files live in the project root and move
  with it; only `~/.aider.conf.yml` may hold absolute paths.
- **claude-code-router**: no session storage, no path-keyed state
  (source-verified).
- **GitHub Copilot CLI**: local `~/.copilot/session-store.db` schema is
  unpublished and the cloud is authoritative → resync instead of adapting.
- **Amp**: threads live server-side (ampcode.com/feed);
  `~/.local/share/amp/threads/T-*.json` are mirrors → not adapted.

## 5. Migration algorithm (as implemented)

1. **Directory renames**: per-vendor encodings (dash / dash-no-lead / omp
   bucket / `--enc--` / droid slashes-only / iflow dash-collapsing /
   basename-slug) + hash directories (full sha256, sha256[:16], md5,
   sha256[:8] filename suffixes). Bucket names match by prefix so
   sub-project sessions (`/old/sub` buckets prefixed by enc(old)) are
   renamed too.
2. **Identity-field rewrite**: JSON/JSONL recursion touches only identity
   keys (cwd, directory, project, workspaceDirectory, workspace_roots
   arrays, …) and path-shaped dict keys (claude.json `projects`, gemini
   projects.json).
3. **Derived-token replacement**: sha256(old)→sha256(new) etc., under the
   same boundary rule.
4. **SQLite**: per-table literal SQL with bound parameters;
   `wal_checkpoint` + whole-file backup before; `VACUUM` after (opencode)
   to purge stale bytes from free pages.
5. **Protobuf**: generic wire-format walk rewriting length-delimited
   fields that contain the old path, with varint length fixups
   (windsurf/antigravity).
6. **`--deep`**: also replaces old-path mentions in logs / chat content /
   environment-context XML.
7. **undo**: renames reversed first (journaled paths mapped back to their
   original locations), then file and whole-database restoration.

## 6. Unverified / known limitations

- VS Code-family `workspaceStorage/<id>` directory-name algorithm
  (unsolved; content rewrite only).
- Windsurf multi-root workspace md5 input (the workspace.json path —
  unsupported).
- Cursor CLI `~/.cursor/chats` store.db (the tested version does not
  produce it).
- Codex 0.147 `state_5.sqlite` (tested against 0.122; adapted from
  source).
- iflow `-p` persists no session (0.5.x, verified) — no functional resume
  verification possible.
- zcode/zed desktop GUIs were not launched end-to-end (database-level
  verification against their real schemas instead).

## Primary sources

- Claude Code: code.claude.com/docs/en/claude-directory;
  anthropics/claude-code #1516 #18829 #21085;
  harnez.ai/posts/fix-broken-project-paths
- Codex: openai/codex #22037 #31317;
  zread.ai/openai/codex/10-rollout-and-state-persistence
- Gemini: google-gemini/gemini-cli
  packages/core/src/config/projectRegistry.ts, utils/paths.ts
- Qwen: QwenLM/qwen-code
  packages/core/src/{utils/paths.ts, config/storage.ts, services/sessionService.ts}
- iFlow: @iflow-ai/iflow-cli bundle; docs_en/features/checkpointing.md
- omp: can1357/oh-my-pi docs/session.md,
  src/session/history-storage.ts, #8323
- pi: earendil-works/pi
  packages/coding-agent/src/core/session-manager.ts,
  docs/session-format.md
- OpenCode: sst/opencode
  packages/core/src/{project.ts, project/sql.ts, session/sql.ts,
  util/hash.ts, database/database.ts},
  packages/opencode/src/session/session.ts
- Crush: charmbracelet/crush
  internal/{config/load.go, config/config.go, db/connect.go,
  projects/projects.go, db/migrations/*}
- Factory: docs.factory.ai/droid-cli/settings; @factory/cli binary
  strings
- cc-connect: chenhg5/cc-connect core/dir_history.go,
  cmd/cc-connect/main.go
- CCR: musistudio/claude-code-router
  packages/core/src/runtime/app-paths.ts et al.
- Cursor: agentgrep.org/backends/{cursor-ide,cursor-cli};
  vibe-replay.com/blog/cursor-local-storage; forum.cursor.com #143475
  #152450 #165486; github.com/S2thend/cursor-history
- Windsurf: Exafunction/codeium #127 #136; agent-steward; local
  verification (md5 algorithm, encrypted .pb)
- Continue: continuedev/continue core/util/paths.ts;
  docs.continue.dev
- Zed: zed-industries/zed
  crates/{agent/src/db.rs, util/src/path_list.rs, paths/src/paths.ts};
  discussions #32335
- Copilot: docs.github.com copilot-cli chronicle / overview;
  jonmagic.com posts
- Amp: ampcode.com/security; docs.rs/ampcode
