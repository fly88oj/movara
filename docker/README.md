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

Each supported agent that can be installed headless gets its own
Dockerfile under `docker/agents/`, installed and exercised in
isolation:

- the real agent CLI is installed from its official channel,
- state is generated or laid out under a throwaway `HOME`,
- `movara scan` / `movara migrate` / `movara undo` run against that
  state and the results are asserted.

Agents whose CLIs require interactive login or a GUI (VS Code
extension families) verify the storage layout against the published
extension/CLI artifact instead — the shipped build is the source of
truth, unpacked and inspected inside the container.

Run one:

```sh
docker compose -f docker/compose.yaml run --rm <agent>
```

(services are registered in `docker/compose.yaml` alongside each
adapter).
