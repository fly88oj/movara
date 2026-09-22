# Qwen Code verification container: npm-installs the real CLI, then
# runs a scan/migrate/undo round trip over the gemini-fork layout
# re-rooted at ~/.qwen (sha256 tmp bucket + ownership marker).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @qwen-code/qwen-code && qwen --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
H=$(printf '%s/proj/abc' "$T" | sha256sum | cut -d' ' -f1)
HN=$(printf '%s/proj/cba' "$T" | sha256sum | cut -d' ' -f1)
TMP=$T/home/.qwen/tmp
ENC=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/abc")
ENCN=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/cba")
mkdir -p $TMP/$H $T/home/.qwen/projects/$ENC $T/proj/abc
printf '%s/proj/abc\n' "$T" > $TMP/$H/.project_root
printf '{"cwd":"%s/proj/abc","msg":"hi"}\n' "$T" > $T/home/.qwen/projects/$ENC/s1.jsonl
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents qwen --yes
[ -d $TMP/$HN ] && [ ! -d $TMP/$H ]
grep -q "$T/proj/cba" $TMP/$HN/.project_root
[ -d $T/home/.qwen/projects/$ENCN ] && grep -q "$T/proj/cba" $T/home/.qwen/projects/$ENCN/s1.jsonl
echo "qwen: sha256 tmp bucket + dash project dir + ownership marker rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $TMP/$H ] && grep -q "$T/proj/abc" $TMP/$H/.project_root
echo "qwen: undo restored OK"
EOF

CMD ["echo", "qwen adapter verification passed"]
