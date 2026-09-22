# Aider verification container: pip-installs the real CLI, then runs
# a scan/migrate/undo round trip over .aider.conf.yml absolute paths.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --break-system-packages aider-chat && aider --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
mkdir -p $T/home $T/proj/abc
printf 'read: %s/proj/abc/notes.md\n' "$T" > $T/home/.aider.conf.yml
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents aider --yes
grep -q "$T/proj/cba/notes.md" $T/home/.aider.conf.yml && ! grep -q "$T/proj/abc" $T/home/.aider.conf.yml
echo "aider: config read paths rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc/notes.md" $T/home/.aider.conf.yml
echo "aider: undo restored OK"
EOF

CMD ["echo", "aider adapter verification passed"]
