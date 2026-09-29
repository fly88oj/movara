# GitHub Copilot CLI verification container: fetches the real binary
# (best effort — distribution channel has changed across GA), then
# runs a scan/migrate/undo round trip against synthetic state shaped
# like the real layout (agents/hooks/skills definitions).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm python3 \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (npm @github/copilot — the GA native binary's brew/GitHub
# channels are authenticated; the npm package ships the same CLI)
RUN npm install -g @github/copilot && copilot --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
AG=$T/home/.copilot/agents
mkdir -p $AG $T/proj/abc
printf '{"name": "review", "cwd": "%s/proj/abc", "tools": ["bash"]}' "$T" > $AG/review.json
# session-store.db chronicle index (sessions.cwd, session_files.file_path,
# forge_skill_proposals.git_root_path) + per-session events.jsonl
python3 - <<PY
import json, os, sqlite3
t = os.environ.get("T", "/tmp/verify")
old = t + "/proj/abc"
con = sqlite3.connect(t + "/home/.copilot/session-store.db")
con.executescript(
    "CREATE TABLE sessions (id TEXT PRIMARY KEY, cwd TEXT, repository TEXT);"
    "CREATE TABLE session_files (session_id TEXT, file_path TEXT, tool_name TEXT);"
    "CREATE TABLE forge_skill_proposals (id TEXT PRIMARY KEY, repo_owner TEXT, repo_name TEXT, git_root_path TEXT);"
)
con.execute("INSERT INTO sessions VALUES ('s1', ?, 'r')", (old,))
con.execute("INSERT INTO session_files VALUES ('s1', ?, 'edit')", (old + "/main.rs",))
con.execute("INSERT INTO forge_skill_proposals VALUES ('p1', 'o', 'r', ?)", (old,))
con.commit()
os.makedirs(t + "/home/.copilot/session-state/s1", exist_ok=True)
with open(t + "/home/.copilot/session-state/s1/events.jsonl", "w") as f:
    f.write(json.dumps({"type": "WorkingDirectoryContext", "cwd": old, "gitRoot": old}) + "\n")
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents copilot --yes
grep -q "$T/proj/cba" $AG/review.json
echo "copilot: definitions rekeyed OK"
python3 - <<PY
import json, os, sqlite3
t = os.environ.get("T", "/tmp/verify")
old, new = t + "/proj/abc", t + "/proj/cba"
con = sqlite3.connect(t + "/home/.copilot/session-store.db")
assert con.execute("SELECT cwd FROM sessions WHERE id='s1'").fetchone()[0] == new
assert con.execute("SELECT file_path FROM session_files").fetchone()[0] == new + "/main.rs"
assert con.execute("SELECT git_root_path FROM forge_skill_proposals").fetchone()[0] == new
ev = json.loads(open(t + "/home/.copilot/session-state/s1/events.jsonl").readline())
assert ev["cwd"] == new and ev["gitRoot"] == new
print("copilot: session-store.db cwd/file_path/git_root_path + events.jsonl cwd/gitRoot rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $AG/review.json
echo "copilot: undo restored OK"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.copilot/session-store.db")
assert con.execute("SELECT cwd FROM sessions WHERE id='s1'").fetchone()[0] == t + "/proj/abc"
assert con.execute("SELECT file_path FROM session_files").fetchone()[0] == t + "/proj/abc/main.rs"
print("copilot: undo restored session-store.db OK")
PY
EOF

CMD ["echo", "copilot adapter verification passed"]
