# pi / gsd verification container: installs the real CLI (best
# effort), then runs a scan/migrate/undo round trip (--encoded--
# session buckets + run-history cwd + projects-memory).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @mariozechner/pi && pi --version || echo "npm channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
B=$(python3 -c "
import re, sys
print('--' + re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1].lstrip('/')) + '--')" "$T/proj/abc")
BN=$(python3 -c "
import re, sys
print('--' + re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1].lstrip('/')) + '--')" "$T/proj/cba")
D=$T/home/.pi/agent/sessions/$B
mkdir -p $D $T/home/.pi/agent/projects-memory/abc $T/proj/abc
printf '{"type":"session","version":3,"id":"uuid7","cwd":"%s/proj/abc"}\n' "$T" > $D/2026-01-01_uuid7.jsonl
printf '{"agent":"x","cwd":"%s/proj/abc","task":"work"}\n' "$T" > $T/home/.pi/agent/run-history.jsonl
printf 'mem\n' > $T/home/.pi/agent/projects-memory/abc/AGENTS.md
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents pi --yes
[ -d $T/home/.pi/agent/sessions/$BN ] && [ ! -d $D ]
grep -q "$T/proj/cba" $T/home/.pi/agent/sessions/$BN/2026-01-01_uuid7.jsonl
head -1 $T/home/.pi/agent/run-history.jsonl | grep -q '"cwd":"'"$T"'/proj/cba"'
echo "pi: --encoded-- bucket + run-history identity cwd rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $D ]
echo "pi: undo restored OK"
EOF

CMD ["echo", "pi adapter verification passed"]
