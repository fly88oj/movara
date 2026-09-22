# OpenAI Codex CLI verification container: npm-installs the real CLI,
# then runs a scan/migrate/undo round trip (rollout session_meta cwd +
# config.toml [projects] trust keys).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @openai/codex && codex --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
S=$T/home/.codex/sessions/2026/09/03
mkdir -p $S $T/proj/abc
printf '{"type":"session_meta","payload":{"id":"u1","cwd":"%s/proj/abc"}}\n{"type":"response_item","payload":{"type":"message","content":"hi"}}\n' "$T" > $S/rollout-x.jsonl
printf '[projects."%s/proj/abc"]\ntrust_level = "trusted"\n' "$T" > $T/home/.codex/config.toml
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents codex --yes
head -1 $S/rollout-x.jsonl | grep -q "$T/proj/cba"
grep -q "$T/proj/cba" $T/home/.codex/config.toml && ! grep -q "$T/proj/abc" $T/home/.codex/config.toml
echo "codex: session_meta cwd + trust key rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
head -1 $S/rollout-x.jsonl | grep -q "$T/proj/abc"
echo "codex: undo restored OK"
EOF

CMD ["echo", "codex adapter verification passed"]
