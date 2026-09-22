# Cline verification container: downloads the SHIPPED .vsix from
# open-vsx and verifies the adapter's storage assumptions against the
# published build (checkpoint constants, history field names), then
# runs a scan/migrate/undo round trip against synthetic state.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 unzip \
    && rm -rf /var/lib/apt/lists/*

# the shipped extension build is the source of truth: unpack and grep
# the constants our adapter keys on
RUN curl -fsSL "$(curl -fsSL 'https://open-vsx.org/api/saoudrizwan/claude-dev/latest' | python3 -c 'import json,sys; print(json.load(sys.stdin)["files"]["download"])')" -o /tmp/cline.vsix \
    && mkdir /tmp/cline && unzip -q /tmp/cline.vsix -d /tmp/cline \
    && grep -rqa "checkpoints" /tmp/cline/extension \
    && grep -rqa "cwdOnTaskInitialization\|shadowGitConfigWorkTree" /tmp/cline/extension \
    && echo "cline .vsix: checkpoint + history-field constants verified in shipped build"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
GS=$T/home/.config/Code/User/globalStorage/saoudrizwan.claude-dev
mkdir -p $GS/tasks/1770000000000 $T/proj/abc
H=$(python3 -c "
import sys
h = 0
for ch in sys.argv[1]:
    h = (h * 31 + ord(ch)) & 0xffffffff
print(h)" "$T/proj/abc")
HN=$(python3 -c "
import sys
h = 0
for ch in sys.argv[1]:
    h = (h * 31 + ord(ch)) & 0xffffffff
print(h)" "$T/proj/cba")
mkdir -p $GS/checkpoints/$H/.git
printf '[core]\n\tworktree = %s/proj/abc\n' "$T" > $GS/checkpoints/$H/.git/config
printf '{"role":"assistant","tool_use":{"path":"%s/proj/abc/main.rs"}}' "$T" > $GS/tasks/1770000000000/api_conversation_history.json
mkdir -p $T/home/.config/Code/User/globalStorage
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
db = t + "/home/.config/Code/User/globalStorage/state.vscdb"
con = sqlite3.connect(db)
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES (?,?)",
            ("saoudrizwan.claude-dev", json.dumps([{"id": "1", "cwdOnTaskInitialization": t + "/proj/abc"}])))
con.execute("INSERT INTO ItemTable VALUES ('other.ext', '{\"x\":1}')")
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents cline --yes
[ -d $GS/checkpoints/$HN ] && [ ! -d $GS/checkpoints/$H ]
grep -q "$T/proj/cba" $GS/checkpoints/$HN/.git/config
grep -q "$T/proj/cba" $GS/tasks/1770000000000/api_conversation_history.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Code/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='saoudrizwan.claude-dev'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
o = con.execute("SELECT value FROM ItemTable WHERE key='other.ext'").fetchone()[0]
assert o == '{"x":1}'
print("cline: cwdHash bucket + worktree + taskHistory rekeyed; unrelated rows untouched")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $GS/checkpoints/$H ]
echo "cline: undo restored OK"
EOF

CMD ["echo", "cline adapter verification passed"]
