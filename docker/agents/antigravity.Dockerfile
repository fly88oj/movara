# Antigravity verification container: the IDE is GUI-only — the
# container verifies the adapter's storage layers in isolation (IDE
# state.vscdb + ~/.gemini/antigravity fork state).
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
GS=$T/home/.config/Antigravity/User/globalStorage
AG=$T/home/.gemini/antigravity/tmp/abc
mkdir -p $GS $AG $T/proj/abc
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Antigravity/User/globalStorage/state.vscdb")
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES ('workbench.panel.aichat', ?)",
            (json.dumps({"workspace": "file://" + t + "/proj/abc"}),))
con.commit()
PY
printf '%s/proj/abc\n' "$T" > $AG/.project_root
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents antigravity --yes
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Antigravity/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='workbench.panel.aichat'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
print("antigravity: ItemTable rekeyed OK")
PY
grep -q "$T/proj/cba" $AG/.project_root
echo "antigravity: fork ownership marker rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $AG/.project_root
echo "antigravity: undo restored OK"
EOF

CMD ["echo", "antigravity adapter verification passed"]
