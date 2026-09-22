# iFlow CLI verification container: npm-installs the real CLI (best
# effort), then runs a scan/migrate/undo round trip over the
# gemini-fork layout re-rooted at ~/.iflow (own bucket encoding +
# sha256 dirs).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @iflow-ai/iflow-cli && iflow --version || echo "npm channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
H=$(printf '%s/proj/abc' "$T" | sha256sum | cut -d' ' -f1)
HN=$(printf '%s/proj/cba' "$T" | sha256sum | cut -d' ' -f1)
# iflow's own bucket: fromPath form (non-alnum -> '-', leading kept)
ENC=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1].replace('/fromPath','')))" "$T/proj/abc")
ENCN=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/cba")
TMP=$T/home/.iflow/tmp
mkdir -p $TMP/$H $T/home/.iflow/projects/$ENC $T/proj/abc
printf '%s/proj/abc\n' "$T" > $TMP/$H/.project_root
printf '{"cwd":"%s/proj/abc","msg":"hi"}\n' "$T" > $T/home/.iflow/projects/$ENC/s1.jsonl
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents iflow --yes
[ -d $TMP/$HN ] && [ ! -d $TMP/$H ]
grep -q "$T/proj/cba" $TMP/$HN/.project_root
grep -q "$T/proj/cba" $T/home/.iflow/projects/$ENCN/s1.jsonl
echo "iflow: sha256 dirs + own-encoded project bucket rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $TMP/$H ]
echo "iflow: undo restored OK"
EOF

CMD ["echo", "iflow adapter verification passed"]
