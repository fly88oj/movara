# Security Policy

## Supported versions

Only the latest release line is supported.

## Reporting a vulnerability

Do **not** open a public issue for security problems. Please contact the
maintainers privately (see the repository's security advisory page or the
contact address in the commit metadata) and include:

- the component involved (adapters, engine, CLI),
- a minimal reproduction,
- the impact you envision.

You will get an acknowledgement within a few days. Coordinated disclosure
(CVE + advisory + patch release together) is preferred.

## Trust model

movara deliberately rewrites files it does not own (agent state
directories, SQLite databases, protobuf blobs). The mitigations are:

- boundary-aware replacement (no accidental neighbor-path corruption),
- a full backup journal before any modification and a first-class `undo`,
- refusals for unsafe inputs (`--from /`, existing rename targets,
  non-directory sources for `movara`),
- SQLite writes go through `wal_checkpoint` and bound parameters only,
- no network access, no telemetry, no credential handling: auth files
  are structurally avoided (adapters only touch session/config paths,
  never `auth*`/`credential*`/`token*` files).

If you find a way around any of these, please report it privately.
