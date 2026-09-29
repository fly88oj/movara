# Zed verification container: the editor is GUI-only — the container
# verifies the adapter's storage layer in isolation (threads.db
# folder_paths columns + db/0-stable sidebar/worktree tables).
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
D=$T/home/.local/share/zed/threads
mkdir -p $D $T/proj/abc
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/zed/threads/threads.db")
con.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, summary TEXT, folder_paths TEXT, folder_paths_order TEXT)")
con.execute("INSERT INTO threads VALUES ('t1', 's', ?, '0')", (t + "/proj/abc",))
con.commit()
PY
# the db/0-<channel> store carries the workspace-scoped tables — seed
# the same shape into two channels (0-stable and 0-global)
for DB in $T/home/.local/share/zed/db/0-stable/db.sqlite $T/home/.local/share/zed/db/0-global/db.sqlite; do
    mkdir -p $(dirname $DB)
    python3 - "$DB" "$T" <<'PY'
import sqlite3, sys
db, t = sys.argv[1], sys.argv[2]
old = t + "/proj/abc"
con = sqlite3.connect(db)
con.execute("CREATE TABLE sidebar_threads (thread_id INTEGER PRIMARY KEY, folder_paths TEXT, main_worktree_paths TEXT)")
con.execute("CREATE TABLE trusted_worktrees (trust_id INTEGER PRIMARY KEY AUTOINCREMENT, absolute_path TEXT, user_name TEXT, host_name TEXT)")
con.execute("CREATE TABLE workspaces (workspace_id INTEGER PRIMARY KEY, paths TEXT, paths_order TEXT)")
con.execute("CREATE TABLE toolchains (workspace_id INTEGER, worktree_root_path TEXT, language_name TEXT)")
con.execute("CREATE TABLE user_toolchains (remote_connection_id INTEGER, workspace_id INTEGER, worktree_root_path TEXT)")
con.execute("CREATE TABLE archived_git_worktrees (id INTEGER PRIMARY KEY, worktree_path TEXT, main_repo_path TEXT)")
con.execute("INSERT INTO sidebar_threads VALUES (1, ?, ?)", (old, old))
con.execute("INSERT INTO trusted_worktrees VALUES (7, ?, 'u', 'h')", (old,))
con.execute("INSERT INTO workspaces VALUES (1, ?, 'x')", (old,))
con.execute("INSERT INTO toolchains VALUES (1, ?, 'rust')", (old,))
con.execute("INSERT INTO user_toolchains VALUES (1, 1, ?)", (old,))
con.execute("INSERT INTO archived_git_worktrees VALUES (1, ?, ?)", (old, old))
con.commit()
PY
done
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents zed --yes
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/zed/threads/threads.db")
assert con.execute("SELECT folder_paths FROM threads").fetchone()[0] == t + "/proj/cba"
print("zed: threads.folder_paths rekeyed OK")
PY
python3 - "$T" <<'PY'
import sqlite3, sys
t = sys.argv[1]
new = t + "/proj/cba"
for ch in ("0-stable", "0-global"):
    con = sqlite3.connect(t + "/home/.local/share/zed/db/" + ch + "/db.sqlite")
    assert con.execute("SELECT folder_paths, main_worktree_paths FROM sidebar_threads WHERE thread_id = 1").fetchone() == (new, new)
    assert con.execute("SELECT absolute_path FROM trusted_worktrees WHERE trust_id = 7").fetchone()[0] == new
    assert con.execute("SELECT paths FROM workspaces WHERE workspace_id = 1").fetchone()[0] == new
    assert con.execute("SELECT worktree_root_path FROM toolchains").fetchone()[0] == new
    assert con.execute("SELECT worktree_root_path FROM user_toolchains").fetchone()[0] == new
    assert con.execute("SELECT worktree_path, main_repo_path FROM archived_git_worktrees WHERE id = 1").fetchone() == (new, new)
    print("zed: " + ch + " channel db path columns rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/zed/threads/threads.db")
assert con.execute("SELECT folder_paths FROM threads").fetchone()[0] == t + "/proj/abc"
print("zed: undo restored OK")
PY
python3 - "$T" <<'PY'
import sqlite3, sys
t = sys.argv[1]
old = t + "/proj/abc"
con = sqlite3.connect(t + "/home/.local/share/zed/db/0-stable/db.sqlite")
assert con.execute("SELECT folder_paths FROM sidebar_threads WHERE thread_id = 1").fetchone()[0] == old
assert con.execute("SELECT absolute_path FROM trusted_worktrees WHERE trust_id = 7").fetchone()[0] == old
print("zed: undo restored 0-stable channel db OK")
PY
EOF

CMD ["echo", "zed adapter verification passed"]
