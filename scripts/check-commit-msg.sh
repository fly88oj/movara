#!/usr/bin/env bash
# Enforce Conventional Commits 1.0.0 on commit messages.
# Spec: https://www.conventionalcommits.org/en/v1.0.0/
# Allowed types mirror the project's history conventions (see CONTRIBUTING).
# Usage (commit-msg hook): check-commit-msg.sh <commit-msg-file>
set -euo pipefail

MSG_FILE="${1:?usage: check-commit-msg.sh <commit-msg-file>}"
HDR="$(head -n1 "$MSG_FILE")"

# allow merge/revert/fixup commits untouched
case "$HDR" in
  "Merge "*|"Revert "*|"fixup! "*) exit 0 ;;
esac

TYPES='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
SCOPES='engine|adapters|cli|i18n|tests|packaging|docs|deps|release|ci'

# subject line: type(scope)?: summary (<=72 chars after the colon)
if ! printf '%s' "$HDR" | grep -qE "^(${TYPES})(\((${SCOPES})\))?!?: .+"; then
  echo "commit message must follow Conventional Commits:" >&2
  echo "  <type>(<scope>)?: <summary>" >&2
  echo "  types:  ${TYPES//|/, }" >&2
  echo "  scopes: ${SCOPES//|/, } (optional)" >&2
  echo "  got:    ${HDR}" >&2
  exit 1
fi
SUBJECT="${HDR#*: }"
if [ "${#SUBJECT}" -gt 72 ]; then
  echo "subject exceeds 72 chars (${#SUBJECT}): ${SUBJECT}" >&2
  exit 1
fi
if printf '%s' "$HDR" | tail -c 2 | grep -qE '[.]$'; then
  echo "subject should not end with a period" >&2
  exit 1
fi
# body (if any) wrapped at 100 and separated by a blank line
if [ "$(wc -l <"$MSG_FILE")" -ge 3 ]; then
  if [ -n "$(sed -n '2p' "$MSG_FILE")" ]; then
    echo "second line must be blank (separates subject from body)" >&2
    exit 1
  fi
fi
exit 0
