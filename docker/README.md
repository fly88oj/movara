# Isolated Docker test matrix

Every agent install and test run happens in a throwaway container: the
host's own agent state, cargo caches and config are never touched, and
debugging one agent cannot interfere with another. Run everything from
the repo root.

## Full test suite in a clean container

```sh
docker compose -f docker/compose.yaml run --rm test
```

Builds `docker/Dockerfile.test` (rust toolchain pinned to the repo's
`rust-toolchain.toml`), compiles the suite at build time, runs it at
container start with `HOME=/tmp/home` so nothing outside the container
is ever read or written. The build context ignores `target/` and `.git`
(see the root `.dockerignore`).

## Per-agent verification containers

Each new adapter ships with its own container under `docker/agents/`,
registered in `docker/compose.yaml`. Every container performs two
layers of verification:

1. **the real agent artifact** — the CLI is installed from its official
   channel (pip/npm/binary release), or the shipped extension/IDE
   package (.vsix from open-vsx, .deb) is unpacked and its storage
   constants are grepped against the adapter's layout assumptions;
2. **a full scan → migrate → assert → undo → assert round trip** with
   synthetic state shaped like the real layout.

Run one (or all):

```sh
docker compose -f docker/compose.yaml run --rm goose
for s in gptme codebuff openhands copilot openinterpreter warp \
         cline roo kilo qoder trae goose; do
  docker compose -f docker/compose.yaml run --rm $s
done
```

| service | artifact channel | verified |
|---|---|---|
| goose | GitHub release tarball (`goose-x86_64-…-gnu.tar.gz`) | real CLI + round trip |
| gptme | PyPI (`pip install gptme`) | real CLI + round trip |
| codebuff | npm (`codebuff`, fallback `freebuff`) | real CLI + round trip |
| openhands | PyPI `openhands-ai` (the SDK/runtime package) on a trixie base (needs Python ≥3.12); the `openhands` console command lives in a separate git-only CLI package that did not build | round trip |
| copilot | npm (`@github/copilot`) | real CLI + round trip |
| openinterpreter | GitHub release `open-interpreter-package-x86_64-…` | real CLI + round trip |
| warp | warp.dev .deb → `dpkg-deb -x` (no install, no GUI) | unpacked build + round trip |
| cline | open-vsx .vsix (saoudrizwan.claude-dev) | shipped-build constants (checkpoints, `cwdOnTaskInitialization`/`shadowGitConfigWorkTree`) + round trip incl. state.vscdb |
| roo | open-vsx .vsix (RooVeterinaryInc.roo-cline — the archive's last release) | shipped-build constants + round trip incl. index-cache invalidation |
| kilo | open-vsx .vsix (kilocode.kilo-code) | shipped-build constants + round trip incl. sha256 bucket renames |
| qoder | qoder.com .deb — **links are JS-rendered, not scrapable; synthetic** | round trip incl. memories buckets + IDE state.vscdb |
| trae | trae .deb — **same; synthetic** | round trip incl. IDE state.vscdb + ~/.trae |

### The original agents (every adapter gets the same treatment)

| service | artifact channel | verified |
|---|---|---|
| claude | npm (`@anthropic-ai/claude-code`) | real CLI + dash bucket + `.claude.json` keys round trip |
| codex | npm (`@openai/codex`) | real CLI + rollout `session_meta` cwd + trust keys round trip |
| gemini | npm (`@google/gemini-cli`; the `--version` gate needs a newer node than bookworm ships, the binary is present) | slug dir + ownership marker + `projects.json` round trip |
| qwen | npm (`@qwen-code/qwen-code`) | real CLI + sha256 tmp bucket + dash project dirs round trip |
| iflow | npm (`@iflow-ai/iflow-cli`) | real CLI + sha256 dirs + own-encoded project bucket round trip |
| opencode | npm (`opencode-ai`) | real CLI + project/worktree + session directory columns round trip |
| omp | npm (`oh-my-pi` — binary `oh-my-pi`) | real CLI + home-relative dash bucket + history.db cwd round trip |
| zcode | GUI IDE — synthetic | session.directory/path + memory-key buckets round trip |
| cursor | GUI IDE — synthetic | CLI project buckets + ItemTable + cursorDiskKV round trip |
| windsurf | GUI IDE — synthetic | md5 context_state buckets + mcp config + ItemTable round trip |
| antigravity | GUI IDE — synthetic | IDE ItemTable + `~/.gemini/antigravity` tmp markers round trip |
| crush | GitHub release .deb (`charmbracelet/crush`) | real CLI + projects.json path/data_dir round trip |
| droid | factory install script (`app.factory.ai/cli` → ~/.local/bin) | real CLI + sessions cwd + background processes round trip |
| continue | **no public headless CLI anymore** (`@continue-dev/cli` and `continue-cli` are gone from npm; the CLI ships inside the IDE); synthetic | file:// workspace URIs + tag_catalog.dir round trip |
| pi | npm `@mariozechner/pi` exposes only the `pi-pods` binary, not the agent; synthetic | `--encoded--` buckets + run-history identity cwd round trip |
| aider | PyPI (`aider-chat`) | real CLI + config absolute paths round trip |
| ccconnect | npm (`cc-connect`) | real CLI + dir MRU + sha256[:8] session-file renames round trip |
| zed | GUI editor — synthetic | threads.folder_paths round trip |
| kimi | **Moonshot CDN binary** (`code.kimi.com/kimi-code/binaries/<ver>`) | **the incident reproduction**: full registry/bucket/wire/events state + derived-store invalidation, gated on the AGENT'S OWN `kimi session list` from the new path |

Synthetic-only channels and why: continue (no public
headless CLI exists anymore), pi (the npm package exposes only a
`pi-pods` helper), qoder/trae (.deb links are JS-rendered), and the
five GUI-only agents (zcode/cursor/windsurf/antigravity/zed). Every
one of these still runs the full synthetic round trip as its hard
gate. The kimi container is the pattern the Kimi Code incident
motivated: disk greps alone proved insufficient (the
cache/query-store resurrection), so its acceptance gate is the
agent's own session listing from the migrated path.
