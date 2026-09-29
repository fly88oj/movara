# Crush verification container: installs the real CLI (best effort —
# charm's install script), then runs a scan/migrate/undo round trip
# (global projects.json path/data_dir entries).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (GitHub release .deb, charmbracelet/crush)
RUN URL=$(curl -fsSL https://api.github.com/repos/charmbracelet/crush/releases/latest | grep -oE '"browser_download_url": *"[^"]*_[0-9.]+_amd64\.deb"' | grep -oE 'https[^"]*' | head -1) \
    && curl -fsSL "$URL" -o /tmp/crush.deb && dpkg -i /tmp/crush.deb && crush --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
mkdir -p $T/home/.local/share/crush $T/proj/abc/.crush $T/proj/cba
printf '{"projects":[{"path":"%s/proj/abc","data_dir":"%s/proj/abc/.crush","last_accessed":1},{"path":"/other","data_dir":"/other/.crush","last_accessed":2}]}' "$T" "$T" > $T/home/.local/share/crush/projects.json
# project-local .crush/crush.db (INSIDE the project dir): edit version
# chains (files.path) + read history (read_files.path); the db file
# itself stays put — only its rows are rekeyed
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
old = t + "/proj/abc"
con = sqlite3.connect(old + "/.crush/crush.db")
con.executescript(
    "CREATE TABLE files (session_id TEXT, version INTEGER, path TEXT);"
    "CREATE TABLE read_files (session_id TEXT, path TEXT);"
)
con.execute("INSERT INTO files VALUES ('s1', 1, ?)", (old + "/main.rs",))
con.execute("INSERT INTO read_files VALUES ('s1', ?)", (old + "/lib.rs",))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents crush --yes
python3 - <<PY
import json, os, sqlite3
t = os.environ.get("T", "/tmp/verify")
d = json.load(open(t + "/home/.local/share/crush/projects.json"))
by = {p["path"]: p for p in d["projects"]}
assert t + "/proj/cba" in by and t + "/proj/abc" not in by
assert by[t + "/proj/cba"]["data_dir"] == t + "/proj/cba/.crush"
assert "/other" in by and by["/other"]["data_dir"] == "/other/.crush"
print("crush: projects.json path/data_dir rekeyed; other projects untouched")
assert os.path.isfile(t + "/proj/abc/.crush/crush.db")
assert not os.path.exists(t + "/proj/cba/.crush/crush.db")
con = sqlite3.connect(t + "/proj/abc/.crush/crush.db")
assert con.execute("SELECT path FROM files").fetchone()[0] == t + "/proj/cba/main.rs"
assert con.execute("SELECT path FROM read_files").fetchone()[0] == t + "/proj/cba/lib.rs"
print("crush: project-local crush.db files/read_files rekeyed (db file stayed in place)")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $T/home/.local/share/crush/projects.json
echo "crush: undo restored OK"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/proj/abc/.crush/crush.db")
assert con.execute("SELECT path FROM files").fetchone()[0] == t + "/proj/abc/main.rs"
assert con.execute("SELECT path FROM read_files").fetchone()[0] == t + "/proj/abc/lib.rs"
print("crush: undo restored crush.db rows OK")
PY
EOF

CMD ["echo", "crush adapter verification passed"]
