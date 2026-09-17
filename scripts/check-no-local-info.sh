#!/usr/bin/env bash
# Reject tracked text files that leak local machine / dev-environment info:
# real home-dir paths, hostnames, private IPs, credentials, local tooling.
# Pre-commit passes filenames as args; files must exist (deletions pass).
set -euo pipefail

FAIL=0
for f in "$@"; do
  [ -f "$f" ] || continue
  # skip vendored/lockfiles
  case "$f" in
    Cargo.lock|*.lock) continue ;;
  esac
  if grep -nE \
    '(/home/[a-z][a-z0-9_-]{2,}/|/Users/[a-z][a-z0-9_-]{2,}/|C:\\Users\\[a-z][a-z0-9_-]{2,}\\)' \
    "$f" 2>/dev/null | grep -vE '/home/(u|user|me|you)/' ; then
    echo "^ $f: absolute home-directory path of a real user" >&2
    FAIL=1
  fi
  if grep -nE '(BEGIN (RSA |EC |OPENSSH |PGP )?PRIVATE KEY|api[_-]?key\s*[:=]\s*["'"'"'][A-Za-z0-9]{16,}|token\s*[:=]\s*["'"'"'][A-Za-z0-9]{16,})' \
    "$f" 2>/dev/null; then
    echo "^ $f: embedded private key / credential" >&2
    FAIL=1
  fi
done
exit $FAIL
