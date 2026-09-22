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
| goose | official install script (curl) | real CLI + round trip |
| gptme | PyPI (`pip install gptme`) | real CLI + round trip |
| codebuff | npm (`codebuff`, fallback `freebuff`) | real CLI + round trip |
| openhands | git+PyPI (no stable public channel — falls back with a printed notice) | round trip |
| copilot | GitHub release binary (best effort) | round trip |
| openinterpreter | GitHub release `open-interpreter-package-x86_64-…` | real CLI + round trip |
| warp | warp.dev .deb → `dpkg-deb -x` (no install, no GUI) | unpacked build + round trip |
| cline | open-vsx .vsix (saoudrizwan.claude-dev) | shipped-build constants (checkpoints, `cwdOnTaskInitialization`/`shadowGitConfigWorkTree`) + round trip incl. state.vscdb |
| roo | open-vsx .vsix (RooVeterinaryInc.roo-cline — the archive's last release) | shipped-build constants + round trip incl. index-cache invalidation |
| kilo | open-vsx .vsix (kilocode.kilo-code) | shipped-build constants + round trip incl. sha256 bucket renames |
| qoder | qoder.com .deb (best effort) | round trip incl. memories buckets + IDE state.vscdb |
| trae | trae .deb (best effort) | round trip incl. IDE state.vscdb + ~/.trae |

Best-effort channels (openhands pip, copilot binary, qoder/trae .deb)
print an explicit notice when unreachable and proceed with the
synthetic round trip — the adapter verification itself is always the
hard gate.
