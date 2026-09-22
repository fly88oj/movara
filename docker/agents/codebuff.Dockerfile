# Codebuff/Freebuff verification container: npm-installs the real CLI,
# then runs a scan/migrate/undo round trip against synthetic state
# shaped like the real layout (projects/<basename>/chats/<ts>/).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (npm; freebuff is the rebranded name if codebuff is gone)
RUN npm install -g codebuff || npm install -g freebuff || echo "npm package unreachable — synthetic verification proceeds"
RUN codebuff --version || freebuff --version || true

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
CHAT=$T/home/.config/manicode/projects/abc/chats/2026-09-01T00-00-00-000Z
mkdir -p $CHAT $T/proj/abc
printf '{"sessionState": {"cwd": "%s/proj/abc", "note": "x"}}' "$T" > $CHAT/run-state.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents codebuff --yes
[ -d $T/home/.config/manicode/projects/cba/chats/2026-09-01T00-00-00-000Z ]
[ ! -d $T/home/.config/manicode/projects/abc ]
grep -q "$T/proj/cba" $T/home/.config/manicode/projects/cba/chats/2026-09-01T00-00-00-000Z/run-state.json
echo "codebuff: basename bucket + run-state cwd rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $T/home/.config/manicode/projects/abc/chats/2026-09-01T00-00-00-000Z ]
echo "codebuff: undo restored OK"
EOF

CMD ["echo", "codebuff adapter verification passed"]
