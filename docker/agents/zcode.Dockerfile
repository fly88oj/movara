# ZCode verification container: the desktop IDE is not headless-
# installable — the container verifies the adapter's storage layer
# (db.sqlite session.directory/path + memory-key buckets) in isolation.
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
Z=$T/home/.zcode/cli
K=$(python3 -c "
import sys, hashlib, re
p = sys.argv[1]
base = re.sub(r'[^A-Za-z0-9]', '-', p.rsplit('/',1)[-1])
print(base + '-' + hashlib.sha256(p.encode()).hexdigest()[:16])" "$T/proj/abc")
KN=$(python3 -c "
import sys, hashlib, re
p = sys.argv[1]
base = re.sub(r'[^A-Za-z0-9]', '-', p.rsplit('/',1)[-1])
print(base + '-' + hashlib.sha256(p.encode()).hexdigest()[:16])" "$T/proj/cba")
mkdir -p $Z/db $Z/memories/projects/$K $Z/agents/sess_1/agent_1 $T/proj/abc
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.zcode/cli/db/db.sqlite")
con.execute("CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT, path TEXT, title TEXT)")
con.execute("CREATE TABLE workflow_run (id TEXT PRIMARY KEY, cwd TEXT, status TEXT)")
con.execute("INSERT INTO session VALUES ('sess_1', ?, ?, 't')", (t + "/proj/abc", t + "/proj/abc"))
con.execute("INSERT INTO workflow_run VALUES ('run_1', ?, 'done')", (t + "/proj/abc",))
con.commit()
PY
printf '{"workspace": "%s/proj/abc"}' "$T" > $Z/agents/sess_1/agent_1/metadata.json
printf '# mem\n' > $Z/memories/projects/$K/MEMORY.md
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents zcode --yes
[ -d $Z/memories/projects/$KN ] && [ ! -d $Z/memories/projects/$K ]
grep -q "$T/proj/cba" $Z/agents/sess_1/agent_1/metadata.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.zcode/cli/db/db.sqlite")
assert con.execute("SELECT directory FROM session").fetchone()[0] == t + "/proj/cba"
assert con.execute("SELECT cwd FROM workflow_run").fetchone()[0] == t + "/proj/cba"
print("zcode: session.directory/path + workflow cwd + memory bucket rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $Z/memories/projects/$K ]
echo "zcode: undo restored OK"
EOF

CMD ["echo", "zcode adapter verification passed"]
