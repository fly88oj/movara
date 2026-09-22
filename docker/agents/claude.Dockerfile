# Claude Code verification container: npm-installs the real CLI, then
# runs a scan/migrate/undo round trip (dash bucket + .claude.json keys).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @anthropic-ai/claude-code && claude --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
ENC=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/abc")
ENCN=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/cba")
P=$T/home/.claude/projects/$ENC
mkdir -p $P $T/proj/abc
printf '{"type":"user","cwd":"%s/proj/abc","sessionId":"s1"}\n' "$T" > $P/sess1.jsonl
printf '{"numStartups":5,"projects":{"%s/proj/abc":{"allowedTools":[]}}}' "$T" > $T/home/.claude.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents claude --yes
[ -d $T/home/.claude/projects/$ENCN ] && [ ! -d $P ]
grep -q "$T/proj/cba" $T/home/.claude/projects/$ENCN/sess1.jsonl
grep -q "$T/proj/cba" $T/home/.claude.json && ! grep -q "$T/proj/abc" $T/home/.claude.json
echo "claude: dash bucket + .claude.json projects key rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $P ] && grep -q "$T/proj/abc" $P/sess1.jsonl
echo "claude: undo restored OK"
EOF

CMD ["echo", "claude adapter verification passed"]
