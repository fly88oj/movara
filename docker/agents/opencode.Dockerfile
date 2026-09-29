# OpenCode verification container: installs the real CLI (official
# script), then runs a scan/migrate/undo round trip over opencode.db
# directory columns.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g opencode-ai && opencode --version

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
# second project row (child-path worktree + sandboxes JSON array), the
# composite-PK project_directory rows, and a second channel db
python3 - "$T" <<'PY'
import json, sqlite3, sys
t = sys.argv[1]
old = t + "/proj/abc"
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
con.execute("INSERT INTO project_directory VALUES ('p1', ?, 'folder', 'auto', 1)", (old,))
con.execute("INSERT INTO project_directory VALUES ('p1', ?, 'worktree', 'auto', 2)", (old + "/wt2",))
con.execute("INSERT INTO project VALUES ('pid2', ?, 'git', ?, '[]')",
            (old + "/sandbox", json.dumps([old + "/s1", old + "/s2"])))
con.commit()
# a second channel db (opencode-stable.db) with the same schema and
# old-path rows — the adapter migrates every opencode*.db channel
scon = sqlite3.connect(t + "/home/.local/share/opencode/opencode-stable.db")
scon.execute("CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT, vcs TEXT, sandboxes TEXT, commands TEXT)")
scon.execute("CREATE TABLE workspace (id TEXT PRIMARY KEY, type TEXT, name TEXT, branch TEXT, directory TEXT, extra TEXT, project_id TEXT, time_used INTEGER)")
scon.execute("CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, slug TEXT, directory TEXT, title TEXT, path TEXT, version TEXT)")
scon.execute("CREATE TABLE project_directory (project_id TEXT, directory TEXT, type TEXT, strategy TEXT, time_created INTEGER)")
scon.execute("CREATE TABLE event (id TEXT PRIMARY KEY, data TEXT)")
scon.execute("INSERT INTO project VALUES ('sp1', ?, 'git', '[]', '[]')", (old,))
scon.execute("INSERT INTO session VALUES ('ss1', 'sp1', NULL, 'x', ?, 't', ?, 'v')", (old, old))
scon.commit()
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
python3 - "$T" <<'PY'
import json, sqlite3, sys
t = sys.argv[1]
new = t + "/proj/cba"
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
dirs = sorted(r[0] for r in con.execute("SELECT directory FROM project_directory"))
assert dirs == [new, new + "/wt2"], dirs
wt2 = con.execute("SELECT worktree FROM project WHERE id = 'pid2'").fetchone()[0]
assert wt2 == new + "/sandbox", wt2
sb = json.loads(con.execute("SELECT sandboxes FROM project WHERE id = 'pid2'").fetchone()[0])
assert sb[0] == new + "/s1" and sb[1] == new + "/s2", sb
scon = sqlite3.connect(t + "/home/.local/share/opencode/opencode-stable.db")
assert scon.execute("SELECT worktree FROM project").fetchone()[0] == new
assert scon.execute("SELECT directory FROM session").fetchone()[0] == new
print("opencode: project_directory rows + pid2 sandboxes + stable channel rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/opencode/opencode.db")
assert con.execute("SELECT worktree FROM project").fetchone()[0] == t + "/proj/abc"
print("opencode: undo restored OK")
PY
python3 - "$T" <<'PY'
import sqlite3, sys
t = sys.argv[1]
scon = sqlite3.connect(t + "/home/.local/share/opencode/opencode-stable.db")
assert scon.execute("SELECT worktree FROM project").fetchone()[0] == t + "/proj/abc"
print("opencode: undo restored stable channel OK")
PY
EOF

CMD ["echo", "opencode adapter verification passed"]
