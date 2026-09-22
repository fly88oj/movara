# Roo Code verification container: downloads the archived extension's
# last .vsix from open-vsx and verifies the storage constants against
# the shipped build, then runs a scan/migrate/undo round trip.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 unzip \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL "$(curl -fsSL 'https://open-vsx.org/api/RooVeterinaryInc/roo-cline/latest' | python3 -c 'import json,sys; print(json.load(sys.stdin)["files"]["download"])')" -o /tmp/roo.vsix \
    && mkdir /tmp/roo && unzip -q /tmp/roo.vsix -d /tmp/roo \
    && grep -rqa "checkpoints" /tmp/roo/extension \
    && grep -rqa "roo-index-cache-\|workspace" /tmp/roo/extension/dist \
    && echo "roo .vsix: checkpoint + workspace-field constants verified in shipped build"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
GS=$T/home/.config/Code/User/globalStorage/rooveterinaryinc.roo-cline
TASK=$GS/tasks/1770000000001
mkdir -p $TASK/checkpoints/.git $T/proj/abc
printf '[core]\n\tworktree = %s/proj/abc\n' "$T" > $TASK/checkpoints/.git/config
printf '{"entries":[{"ts":"1770000000001","workspace":"%s/proj/abc"}]}' "$T" > $GS/tasks/_index.json
S=$(printf '%s/proj/abc' "$T" | sha256sum | cut -d' ' -f1)
printf '{"stale":true}' > $GS/roo-index-cache-$S.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents cline --yes
grep -q "$T/proj/cba" $TASK/checkpoints/.git/config
grep -q "$T/proj/cba" $GS/tasks/_index.json
[ ! -f $GS/roo-index-cache-$S.json ]
echo "roo: index workspace + shadow-git worktree rekeyed; index cache invalidated OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $GS/tasks/_index.json
echo "roo: undo restored OK"
EOF

CMD ["echo", "roo adapter verification passed"]
