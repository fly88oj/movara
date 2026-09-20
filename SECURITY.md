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

## Cross-host moves (v1.2)

`movara move` shells out to `ssh -T` and streams the archive to a
`movara receive` on the target — the transport is exactly as
trustworthy as your ssh channel. `receive` places files only: state
under the validated agent roots, the project tree under the
destination you name, with per-member revalidation (no `..`, no
symlink members); it never executes anything from the stream, and a
truncated stream places nothing. Its write surface is wider BY DESIGN
(a project lands where you point it), so `receive` is only as
trustworthy as the stream's source — never pipe an archive you did
not produce yourself.

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

## Archives (export / import)

Exports apply a structural exclusion layer: auth/credential/token
files, shell snapshots and secret-carrying configs never leave the
machine, and dual-purpose configs ship as path-keyed projections
only. No rule, however, can see inside conversation transcripts —
anything pasted into a chat may be in an archive. Treat every archive
as sensitive; move it over trusted channels only.
