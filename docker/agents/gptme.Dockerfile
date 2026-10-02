# gptme verification container: pip-installs the real CLI, then runs a
# full scan/migrate/undo round trip against synthetic state shaped like
# the real layout (config.toml [chat] workspace + symlink + jsonl).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (official PyPI package)
RUN pip install --break-system-packages gptme && gptme --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
CONV=$T/home/.local/share/gptme/logs/2026-09-01-verif
mkdir -p $CONV $T/proj/abc
printf '[chat]\nname = "verif"\nworkspace = "%s/proj/abc"\n' "$T" > $CONV/config.toml
printf '{"role": "user", "content": "hi"}\n' > $CONV/conversation.jsonl
ln -s $T/proj/abc $CONV/workspace
# ACP surface: logs/acp-<sha256(resolved cwd)[:8]>/ is path-derived and
# renames with the move
A=acp-$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:8])" "$T/proj/abc")
AN=acp-$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:8])" "$T/proj/cba")
ACONV=$T/home/.local/share/gptme/logs/$A
mkdir -p $ACONV
printf '{"role": "user", "content": "acp"}\n' > $ACONV/conversation.jsonl
# agent's own view BEFORE: the installed CLI lists the seeded chat
HOME=$T/home gptme chats list | grep -q verif
echo "gptme: agent's own chats list sees the seeded conversation"
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents gptme --yes
grep -q "$T/proj/cba" $CONV/config.toml
[ "$(readlink $CONV/workspace)" = "$T/proj/cba" ]
echo "gptme: workspace + symlink rekeyed OK"
[ -d $T/home/.local/share/gptme/logs/$AN ] && [ ! -d $ACONV ]
echo "gptme: acp session dir rekeyed OK"
# agent's own view AFTER: the conversation still lists through the CLI
HOME=$T/home gptme chats list | grep -q verif
echo "gptme: agent's own chats list still sees the migrated conversation"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $CONV/config.toml
[ "$(readlink $CONV/workspace)" = "$T/proj/abc" ]
echo "gptme: undo restored OK"
[ -d $ACONV ]
echo "gptme: undo restored acp dir OK"
EOF

CMD ["echo", "gptme adapter verification passed"]
