# Windsurf verification container: the IDE is GUI-only — the container
# verifies the adapter's storage layers in isolation (md5 context_state
# buckets + mcp_config + IDE state.vscdb).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
M=$(printf '%s/proj/abc' "$T" | md5sum | cut -d' ' -f1)
MN=$(printf '%s/proj/cba' "$T" | md5sum | cut -d' ' -f1)
CS=$T/home/.codeium/windsurf/context_state/$M
GS=$T/home/.config/Windsurf/User/globalStorage
mkdir -p $CS $T/home/.codeium/windsurf $GS $T/proj/abc
printf '{"cwd":"%s/proj/abc"}' "$T" > $CS/state.json
printf '{"mcpServers":{"x":{"command":"/bin/ls","cwd":"%s/proj/abc"}}}' "$T" > $T/home/.codeium/windsurf/mcp_config.json
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Windsurf/User/globalStorage/state.vscdb")
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES ('codeium.windsurf', ?)",
            (json.dumps({"folder": "file://" + t + "/proj/abc"}),))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents windsurf --yes
[ -d $T/home/.codeium/windsurf/context_state/$MN ] && [ ! -d $CS ]
grep -q "$T/proj/cba" $T/home/.codeium/windsurf/context_state/$MN/state.json
grep -q "$T/proj/cba" $T/home/.codeium/windsurf/mcp_config.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Windsurf/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='codeium.windsurf'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
print("windsurf: md5 bucket + mcp config + ItemTable rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $CS ]
echo "windsurf: undo restored OK"
EOF

CMD ["echo", "windsurf adapter verification passed"]
