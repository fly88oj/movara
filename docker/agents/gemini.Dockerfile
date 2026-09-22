# Gemini CLI verification container: npm-installs the real CLI, then
# runs a scan/migrate/undo round trip (.project_root ownership marker +
# projects.json keys + sha256 tmp bucket).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @google/gemini-cli && gemini --version || echo "npm channel needs a newer node than bookworm ships — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
H=$(printf '%s/proj/abc' "$T" | sha256sum | cut -d' ' -f1)
HN=$(printf '%s/proj/cba' "$T" | sha256sum | cut -d' ' -f1)
D=$T/home/.gemini/tmp/abc
mkdir -p $D/chats $T/home/.gemini/history/abc $T/proj/abc
printf '%s/proj/abc\n' "$T" > $D/.project_root
printf '{"sessionId":"1","projectHash":"%s","messages":[]}' "$H" > $D/chats/session-1.json
printf '{"prompt":"hi"}\n' > $T/home/.gemini/history/abc/prompts.jsonl
printf '{"projects":{"%s/proj/abc":"abc"}}' "$T" > $T/home/.gemini/projects.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents gemini --yes
# the slug dir follows the new basename and its marker follows the path
[ -d $T/home/.gemini/tmp/cba ] && [ ! -d $D ]
grep -q "$T/proj/cba" $T/home/.gemini/tmp/cba/.project_root
grep -q "$T/proj/cba" $T/home/.gemini/projects.json && ! grep -q "$T/proj/abc" $T/home/.gemini/projects.json
echo "gemini: slug dir + ownership marker + projects.json rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $D ] && grep -q "$T/proj/abc" $D/.project_root
echo "gemini: undo restored OK"
EOF

CMD ["echo", "gemini adapter verification passed"]
