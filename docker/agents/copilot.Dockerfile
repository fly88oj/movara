# GitHub Copilot CLI verification container: fetches the real binary
# (best effort — distribution channel has changed across GA), then
# runs a scan/migrate/undo round trip against synthetic state shaped
# like the real layout (agents/hooks/skills definitions).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (npm @github/copilot — the GA native binary's brew/GitHub
# channels are authenticated; the npm package ships the same CLI)
RUN npm install -g @github/copilot && copilot --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
AG=$T/home/.copilot/agents
mkdir -p $AG $T/proj/abc
printf '{"name": "review", "cwd": "%s/proj/abc", "tools": ["bash"]}' "$T" > $AG/review.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents copilot --yes
grep -q "$T/proj/cba" $AG/review.json
echo "copilot: definitions rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $AG/review.json
echo "copilot: undo restored OK"
EOF

CMD ["echo", "copilot adapter verification passed"]
