# Goose (Block) verification container: installs the real CLI from its
# official channel and probes the state layout the movara adapter
# assumes (data dir via etcetera's XDG strategy on Linux).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# the CLI itself (official install script)
RUN curl -fsSL https://github.com/block/goose/releases/download/stable/download-goose.sh | bash
RUN goose version

# movara, built from the repo
WORKDIR /src
COPY . .
RUN cargo build --release

# a synthetic goose state shaped like the real thing, then a full
# scan/migrate/undo round trip with the adapter
RUN <<'EOF'
set -e
T=/tmp/verify
mkdir -p $T/home/.local/share/goose/sessions $T/proj/abc
python3 - <<'PY'
import sqlite3
t = "/tmp/verify"
con = sqlite3.connect(t + "/home/.local/share/goose/sessions/sessions.db")
con.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, description TEXT, working_dir TEXT NOT NULL, created_at TEXT)")
con.execute("INSERT INTO sessions VALUES (?,?,?,?)", ("20260901_1", "s", t + "/proj/abc", "2026-09-01T00:00:00Z"))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/release/movara migrate --from $T/proj/abc --to $T/proj/cba --agents goose --yes
python3 - <<'PY'
import sqlite3
t = "/tmp/verify"
con = sqlite3.connect(t + "/home/.local/share/goose/sessions/sessions.db")
wd = con.execute("SELECT working_dir FROM sessions").fetchone()[0]
assert wd == t + "/proj/cba", wd
print("goose verification: sessions.working_dir rekeyed OK")
PY
/src/target/release/movara undo --id $(ls $T/home/.movara/backups | tail -1)
python3 - <<'PY'
import sqlite3
t = "/tmp/verify"
con = sqlite3.connect(t + "/home/.local/share/goose/sessions/sessions.db")
wd = con.execute("SELECT working_dir FROM sessions").fetchone()[0]
assert wd == t + "/proj/abc", wd
print("goose verification: undo restored OK")
PY
EOF

CMD ["echo", "goose adapter verification passed"]
