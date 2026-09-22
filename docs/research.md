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

## 2026-09-22 market census additions (11 new agents)

The full census (community/awesome lists, orchestrator support matrices
like Vibe Kanban and claude-code-router, GitHub topics) produced ~30
candidates beyond the 19 supported agents; the twelve below carry
verifiable local session/history state and gained adapters. Storage
facts were verified against each project's open-source repo and, for
Qoder/Trae/Copilot/Kimi, against live on-disk state.

### Goose (Block) — 54.5k★
- `~/.local/share/goose/sessions/sessions.db` (SQLite, WAL, schema v9):
  `sessions.working_dir` TEXT NOT NULL. Pre-db releases wrote flat
  `sessions/*.jsonl` whose first line is session metadata carrying
  `working_dir`.
- Config `~/.config/goose/permissions/tool_permissions.json` keys
  projects by absolute path.
- Per-OS roots follow goose's etcetera strategy (macOS
  `~/Library/Application Support/Block.block.goose` + Preferences
  bundle dir; Windows `%APPDATA%\Block\goose`). No path-derived
  bucket names (session ids are `YYYYMMDD_N`). `messages.content_json`
  is chat content (--deep).

### Cline / Roo Code / Kilo Code (VS Code extension family)
- globalStorage per marketplace id (`saoudrizwan.claude-dev`,
  `rooveterinaryinc.roo-cline`, `kilocode.kilo-code`) under every IDE
  they run in (Code, Cursor, Windsurf, VSCodium, ~/.vscode-server).
- Task history: Roo `tasks/_index.json` + `history_item.json`
  (`workspace` field); Cline 4.x `state/taskHistory.json`
  (`cwdOnTaskInitialization`, legacy `shadowGitConfigWorkTree`); Cline
  ≤3.x and classic Kilo keep the array in the IDE's state.vscdb
  ItemTable under the extension key (rewritten row-scoped).
- Checkpoints are shadow git repos whose `.git/config core.worktree`
  is the absolute workspace path — a stale value makes the extension
  refuse to resume. Cline ≤3.x keys them `checkpoints/<cwdHash>/`
  where cwdHash is a polynomial hash (×31, u32, UTF-16 code units,
  decimal — handled as an exact-name rename, deliberately NOT a text
  needle); Roo/Kilo key them per-task; classic Kilo also has legacy
  `checkpoints/<sha256(cwd)[:8]>/` and `sessions/<sha256[:16]>/`.
- Roo's `roo-index-cache-<sha256>.json` files are regenerable and are
  removed on migration (derived-store invalidation). Roo Code was
  archived 2026-05; its state is found abandoned on disk.

### OpenHands
- `~/.openhands`: `conversations/<uuid>/events/event-*.json` (ids are
  UUIDs), `agent_settings.json` `working_dir`, and
  `projects/<sha256(realpath(cwd))>/prompt_history.json`. Cloud
  conversation store is a stub. Three env vars can relocate the root
  (not tracked).

### Codebuff / Freebuff
- `~/.config/manicode/projects/<basename>/chats/<timestamp>/`
  (the legacy config name survives the rebrand). The project key is
  the bare basename — same-basename projects share storage upstream
  (documented quirk); collisions at rename are refused loudly.
  run-state.json sessionState embeds the cwd.

### gptme
- `~/.local/share/gptme/logs/<YYYY-MM-DD>-<name>/` — flat; names derive
  from date + random/user/LLM naming, never the path. The project link
  is `config.toml [chat] workspace`, saved TILDE-ABBREVIATED under
  home (both forms rewritten; the spec machinery would cwd-join a
  "~"-path, so the tilde form uses a dedicated boundary replacement).
  `workspace` is a symlink to the project (retargeted, never
  followed). Message `files` lists carry attachment paths.

### Qoder / Tongyi Lingma (CN)
- IDE is a VS Code fork (`~/.config/Qoder` state.vscdb + workspace
  storage). Home root `~/.qoder`:
  `memories/<account-hash>/projects/<dash-encoded-path>/**` — the
  project level is the classic dash encoding; the 8-hex bucket above
  it is account-keyed, not path-derived. Tongyi Lingma migrated its
  CN variant to `~/.lingma/qoder-cn` with the same memories layout.
  Lingma's own `index/` is a binary regenerable store (left alone).

### Trae (ByteDance)
- VS Code fork state under `~/.config/Trae CN` — the CN build's app
  dir carries a literal space (international build: plain `Trae`) —
  plus the thin `~/.trae` definition root (agents/skills/mcp.json).

### GitHub Copilot CLI
- `~/.copilot` agents/hooks/skills definitions. Observed GA installs
  keep no session transcripts under this root; the definition layer is
  the local surface.

### Warp
- `~/.local/share/warp/warp.db` (macOS `~/.warp`). Closed, undocumented
  schema: the adapter discovers every table's TEXT columns at runtime
  (PRAGMA table_info) and rewrites rows generically under the
  boundary-aware patterns; journaled whole-file for undo. Verified
  against a synthetic db only — no live sample was available.

### Kimi Code (Moonshot) — verified live end to end
- `~/.kimi-code`: buckets `wd_<basename(root)>_<sha256(root)[:12]>`
  (34/34 live workspaces matched) naming a sessions/ directory and
  file-history/ + workspace-trust/ FILES; workspaces.json (bucket
  keys + roots + display name); session_index.jsonl
  (sessionDir/workDir); per-session state.json (workDir + homedir) and
  wire.jsonl (`runtime.set_binding.workspaceId` binds the session to
  its workspace); the server event stream
  (`event.workspace.updated` payload ids under generic keys).
- **Derived stores hold pre-migration metadata and MUST be
  invalidated**: `cache/query-store` (a sharded WAL+generations
  materialized view the server answers session/workspace queries
  from — it never re-reads the authoritative files until invalidated,
  invisible to grep; found via strace), `sessions/.index-cache`,
  `search-index`. All three are removed on migration and rebuild on
  next launch. A stale store resurrected the old workspace in the
  live web UI even with every authoritative layer clean — the
  acceptance test is the agent's own view (`kimi session list` in the
  new path + the API/UI), never disk greps alone.

## Census exclusions (documented out of scope)

- **Cloud-only / server-side sessions**: Google Jules (CLI drives
  cloud sessions), Devin, Replit Agent, Lovable, Bolt.new, v0,
  Firebase Studio, Roomote (Roo's post-shutdown cloud product),
  Sweep (shut down 2026-04), MiniMax Agent (web), CodeGeeX web chat.
- **Hybrid, local config only**: Amp (threads server-side at
  ampcode.com/feed; local `~/.local/share/amp` holds config and a
  thread mirror whose internals are undocumented and version-
  unstable — nothing load-bearing to rekey).
- **Covered by existing adapters**: MiniMax Code reuses
  `~/.local/share/opencode` (OpenCode-derived; the opencode adapter
  covers it), VS Code Copilot chat sessions live in the IDE's
  workspaceStorage (covered by the fork machinery where the IDE is
  supported).
- **Adjacent tooling, not chat-session state**: Backlog.md (project
  task markdown), orchestrators (Vibe Kanban, claude-squad, crystal/
  Nimbalyst, Conductor, claude-code-router) whose configs may embed
  paths but hold no session transcripts.
- **Weak/unverifiable local signals**: Junie (JetBrains; `~/.junie`
  trust markers, conversation subdir unconfirmed), Cody (server-side
  chat sync since v1.20; local transcripts secondary), Refact.ai
  (self-host Docker volumes), Tongyi Lingma's own pre-migration
  layout (superseded by qoder-cn, covered).

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

## Memory inventory (v1.2 move carriage)

Project-scoped memory rides a `movara move` through two mechanisms:
home-side stores keyed by an encoding of the project path (selected via
their directory keys — the prose inside carries no path string), and
in-project memory files (carried with the project tree, or via the
`project-memory/` manifest on `--state-only`).

| agent | project memory | keyed by | selected via | global memory (roadmap) |
|---|---|---|---|---|
| claude | `~/.claude/projects/<dash>/` (all per-project data) | dash(cwd) | dir key | `~/.claude/CLAUDE.md` |
| codex | `AGENTS.md` in project | file in project | project tree | `~/.codex/AGENTS.md` |
| gemini | `GEMINI.md` in project | file in project | project tree | `~/.gemini/GEMINI.md` |
| qwen / iflow | same family as gemini | file in project | project tree | user-level md |
| opencode | storage db rows + memory under `~/.local/share/opencode/` | db | content match | global memory dir |
| omp / pi | `~{.pi,.omp}/agent/projects-memory/<basename>/` | basename | dir key (depth ≤ 2) | — |
| zcode | `~/.zcode/cli/memories/projects/<basename>-<sha256[:16]>/` | memory key | dir key (any depth) | — |
| cursor | `.cursor/rules/` in project | dir in project | project tree | global rules store |
| windsurf | `.windsurf/rules/` in project | dir in project | project tree | `~/.codeium/windsurf/memories` |
| antigravity | rules in project | dir in project | project tree | — |
| crush | `<project>/.crush/` | in project | rides the user's own project move | global `projects.json` |
| droid | `AGENTS.md` in project | file in project | project tree | — |
| continue | config + index | db | content match | — |
| aider | `CONVENTIONS.md` in project | file in project | project tree | chat history |
| zed | thread data | db | content match | — |
| cc-connect | none | — | — | — |

Global memory deliberately does NOT move in v1.2: both hosts almost
always have their own, so carriage is a merge problem that waits for
the additive/merge machinery.
