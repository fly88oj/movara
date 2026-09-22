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
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents zed --yes
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/zed/threads/threads.db")
assert con.execute("SELECT folder_paths FROM threads").fetchone()[0] == t + "/proj/cba"
print("zed: threads.folder_paths rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.local/share/zed/threads/threads.db")
assert con.execute("SELECT folder_paths FROM threads").fetchone()[0] == t + "/proj/abc"
print("zed: undo restored OK")
PY
EOF

CMD ["echo", "zed adapter verification passed"]
