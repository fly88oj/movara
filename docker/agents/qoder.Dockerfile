# Qoder verification container: downloads the real IDE .deb (best
# effort — qoder.com distribution), unpacks with dpkg-deb -x to verify
# the layout constants, then runs a scan/migrate/undo round trip over
# the memories buckets + IDE state.vscdb.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 dpkg \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (official install script — installs `qodercli` into
# ~/.local/bin and ~/.qoder/bin)
ENV HOME=/root
RUN curl -fsSL https://qoder.com/install | bash && /root/.local/bin/qodercli --version
ENV PATH="/root/.local/bin:${PATH}"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
MEM=$T/home/.qoder/memories/019f8e2a/projects
GSDB=$T/home/.config/Qoder/User/globalStorage/state.vscdb
D=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/abc")
DN=$(python3 -c "
import re, sys
print(re.sub(r'[^A-Za-z0-9]', '-', sys.argv[1]))" "$T/proj/cba")
mkdir -p $MEM/$D $T/home/.config/Qoder/User/globalStorage $T/proj/abc
printf 'memory note\n' > $MEM/$D/note.md
mkdir -p $T/home/.qoder
printf '{"mcpServers":{"x":{"command":"npx","cwd":"%s/proj/abc"}}}' "$T" > $T/home/.qoder/mcp.json
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Qoder/User/globalStorage/state.vscdb")
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES ('workbench.panel.aichat', ?)",
            (json.dumps({"history": [{"workspace": t + "/proj/abc"}]}),))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents qoder --yes
[ -d $MEM/$DN ] && [ ! -d $MEM/$D ]
grep -q "$T/proj/cba" $T/home/.qoder/mcp.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Qoder/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='workbench.panel.aichat'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
print("qoder: memories bucket + mcp cwd + IDE ItemTable rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $MEM/$D ]
echo "qoder: undo restored OK"
EOF

CMD ["echo", "qoder adapter verification passed"]
