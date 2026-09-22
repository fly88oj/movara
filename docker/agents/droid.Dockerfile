# Factory Droid verification container: npm-installs the real CLI
# (best effort), then runs a scan/migrate/undo round trip (sessions
# cwd + background-processes).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (factory's install script drops it in ~/.local/bin)
RUN curl -fsSL https://app.factory.ai/cli | sh && /root/.local/bin/droid --version
ENV PATH="/root/.local/bin:${PATH}"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
F=$T/home/.factory/sessions
mkdir -p $F $T/home/.factory $T/proj/abc
printf '{"sessionId":"s1","cwd":"%s/proj/abc"}' "$T" > $F/s1.json
printf '{"procs":[{"cwd":"%s/proj/abc"}]}' "$T" > $T/home/.factory/background-processes.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents droid --yes
grep -q "$T/proj/cba" $F/s1.json
grep -q "$T/proj/cba" $T/home/.factory/background-processes.json && ! grep -q "$T/proj/abc" $T/home/.factory/background-processes.json
echo "droid: session cwd + background processes rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $F/s1.json
echo "droid: undo restored OK"
EOF

CMD ["echo", "droid adapter verification passed"]
