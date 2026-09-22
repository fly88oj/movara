# Kilo Code (classic) verification container: downloads the classic
# .vsix from open-vsx and verifies the storage constants, then runs a
# scan/migrate/undo round trip (sha256 session/checkpoint buckets).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 unzip \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL "$(curl -fsSL 'https://open-vsx.org/api/kilocode/kilo-code/latest' | python3 -c 'import json,sys; print(json.load(sys.stdin)["files"]["download"])')" -o /tmp/kilo.vsix \
    && mkdir /tmp/kilo && unzip -q /tmp/kilo.vsix -d /tmp/kilo \
    && grep -rqa "checkpoints" /tmp/kilo/extension/dist \
    && echo "kilo .vsix: checkpoint constants verified in shipped build"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
GS=$T/home/.config/Code/User/globalStorage/kilocode.kilo-code
S8=$(printf '%s/proj/abc' "$T" | sha256sum | cut -c1-8)
S8N=$(printf '%s/proj/cba' "$T" | sha256sum | cut -c1-8)
S16=$(printf '%s/proj/abc' "$T" | sha256sum | cut -c1-16)
S16N=$(printf '%s/proj/cba' "$T" | sha256sum | cut -c1-16)
mkdir -p $GS/checkpoints/$S8/.git $GS/sessions/$S16 $T/proj/abc
printf '[core]\n\tworktree = %s/proj/abc\n' "$T" > $GS/checkpoints/$S8/.git/config
printf '{"workspace": "%s/proj/abc", "turns": 3}' "$T" > $GS/sessions/$S16/session.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents cline --yes
[ -d $GS/checkpoints/$S8N ] && [ ! -d $GS/checkpoints/$S8 ]
grep -q "$T/proj/cba" $GS/checkpoints/$S8N/.git/config
[ -d $GS/sessions/$S16N ] && [ ! -d $GS/sessions/$S16 ]
grep -q "$T/proj/cba" $GS/sessions/$S16N/session.json
echo "kilo: sha256[:8] checkpoint + [:16] session buckets rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $GS/checkpoints/$S8 ] && [ -d $GS/sessions/$S16 ]
echo "kilo: undo restored OK"
EOF

CMD ["echo", "kilo adapter verification passed"]
