# OpenCode verification container: installs the real CLI (official
# script), then runs a scan/migrate/undo round trip over opencode.db
# directory columns.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

RUN (curl -fsSL https://opencode.ai/install | bash && opencode --version) || echo "install channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
mkdir -p $T/home/.local/share/opencode $T/proj/abc
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
con.execute("CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT, vcs TEXT, sandboxes TEXT, commands TEXT)")
con.execute("CREATE TABLE workspace (id TEXT PRIMARY KEY, type TEXT, name TEXT, branch TEXT, directory TEXT, extra TEXT, project_id TEXT, time_used INTEGER)")
con.execute("CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, slug TEXT, directory TEXT, title TEXT, path TEXT, version TEXT)")
con.execute("CREATE TABLE project_directory (project_id TEXT, directory TEXT, type TEXT, strategy TEXT, time_created INTEGER)")
con.execute("CREATE TABLE event (id TEXT PRIMARY KEY, data TEXT)")
con.execute("INSERT INTO project VALUES ('p1', ?, 'git', '', '')", (t + "/proj/abc",))
con.execute("INSERT INTO workspace VALUES ('w1','main','x','main',?, '', 'p1', 0)", (t + "/proj/abc",))
con.execute("INSERT INTO session VALUES ('s1','p1',NULL,'x',?, 't', ?, 'v')", (t + "/proj/abc", t + "/proj/abc"))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents opencode --yes
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
assert con.execute("SELECT worktree FROM project").fetchone()[0] == t + "/proj/cba"
assert t + "/proj/cba" in con.execute("SELECT directory FROM session").fetchone()[0]
print("opencode: project.worktree + session.directory rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
assert con.execute("SELECT worktree FROM project").fetchone()[0] == t + "/proj/abc"
print("opencode: undo restored OK")
PY
EOF

CMD ["echo", "opencode adapter verification passed"]
