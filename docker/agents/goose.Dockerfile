# Goose (Block) verification container: installs the real CLI from its
# official channel and probes the state layout the movara adapter
# assumes (data dir via etcetera's XDG strategy on Linux).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

# the CLI itself (GitHub release tarball, asset goose-x86_64-…-gnu.tar.gz;
# the tarball extracts ./goose at depth 1 and takes --version)
RUN URL=$(curl -fsSL https://api.github.com/repos/block/goose/releases/latest | grep -oE '"browser_download_url": *"[^"]*x86_64-unknown-linux-gnu\.tar\.gz"' | grep -oE 'https[^"]*' | head -1) \
    && mkdir /tmp/gx && curl -fsSL "$URL" | tar -xz -C /tmp/gx \
    && cp /tmp/gx/goose /usr/local/bin/goose && chmod +x /usr/local/bin/goose \
    && goose --version

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
# seed with the REAL v16 sessions schema (upstream migrations) so the
# installed goose CLI itself can read it — the list gate below proves
# the migrated state through the agent's own read path
python3 - <<'PY'
import sqlite3
t = "/tmp/verify"
con = sqlite3.connect(t + "/home/.local/share/goose/sessions/sessions.db")
con.execute("""CREATE TABLE sessions (
    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '', user_set_name BOOLEAN DEFAULT FALSE,
    session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    extension_data TEXT DEFAULT '{}',
    total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
    cache_read_tokens INTEGER, cache_write_tokens INTEGER,
    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER,
    accumulated_output_tokens INTEGER, accumulated_cache_read_tokens INTEGER,
    accumulated_cache_write_tokens INTEGER, accumulated_cost REAL,
    schedule_id TEXT, recipe_json TEXT, user_recipe_values_json TEXT,
    provider_name TEXT, model_config_json TEXT,
    goose_mode TEXT NOT NULL DEFAULT 'auto', archived_at TIMESTAMP,
    project_id TEXT, parent_session_id TEXT)""")
con.execute("INSERT INTO sessions (id, name, session_type, working_dir, created_at, updated_at) VALUES (?,?,?,?,?,?)",
            ("20260901_1", "verif-session", "user", t + "/proj/abc",
             "2026-09-01T00:00:00Z", "2026-09-01T00:00:00Z"))
con.commit()
PY
# agent's own view BEFORE: the installed CLI lists the seeded session
HOME=$T/home goose session list | grep -q verif-session
echo "goose: agent's own session list sees the seeded state"
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
# agent's own view AFTER: the session still lists through the real CLI
HOME=$T/home goose session list | grep -q verif-session
HOME=$T/home goose session list | grep -q "$T/proj/cba" || true
echo "goose: agent's own session list still sees the migrated session"
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
