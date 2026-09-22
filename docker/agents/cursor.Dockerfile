# Cursor verification container: the IDE is GUI-only — the container
# verifies the adapter's two storage layers in isolation (CLI project
# buckets + IDE state.vscdb ItemTable/cursorDiskKV).
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
ENC=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]).lstrip('-'))" "$T/proj/abc")
ENCN=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]).lstrip('-'))" "$T/proj/cba")
CLI=$T/home/.cursor/projects/$ENC
GS=$T/home/.config/Cursor/User/globalStorage
mkdir -p $CLI/agent-transcripts $GS $T/proj/abc
printf '{"cwd":"%s/proj/abc"}\n' "$T" > $CLI/agent-transcripts/t1.jsonl
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Cursor/User/globalStorage/state.vscdb")
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES ('workbench.panel.aichat', ?)",
            (json.dumps({"workspace": "file://" + t + "/proj/abc"}),))
con.execute("INSERT INTO cursorDiskKV VALUES ('composer1', ?)",
            (json.dumps({"fsPath": t + "/proj/abc/main.rs"}),))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents cursor --yes
[ -d $T/home/.cursor/projects/$ENCN ] && [ ! -d $CLI ]
grep -q "$T/proj/cba" $T/home/.cursor/projects/$ENCN/agent-transcripts/t1.jsonl
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Cursor/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='workbench.panel.aichat'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
k = con.execute("SELECT value FROM cursorDiskKV WHERE key='composer1'").fetchone()[0]
assert t + "/proj/cba" in k
print("cursor: CLI bucket + ItemTable + cursorDiskKV rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $CLI ]
echo "cursor: undo restored OK"
EOF

CMD ["echo", "cursor adapter verification passed"]
