# Oh My Pi (omp) verification container: npm-installs the real CLI
# (best effort), then runs a scan/migrate/undo round trip (home-
# relative dash session bucket + history.db cwd).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g oh-my-pi && oh-my-pi --version || echo "npm channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
# omp bucket: cwd relative to home when under it, else the full path
# with the leading '/' stripped; dash-encoded, '-' prefix
B=$(python3 -c "
import re, sys
p, home = sys.argv[1], sys.argv[2] + '/home'
rel = p[len(home)+1:] if p.startswith(home + '/') else p.lstrip('/')
print('-' + re.sub(r'[^A-Za-z0-9]', '-', rel))" "$T/proj/abc" "$T")
BN=$(python3 -c "
import re, sys
p, home = sys.argv[1], sys.argv[2] + '/home'
rel = p[len(home)+1:] if p.startswith(home + '/') else p.lstrip('/')
print('-' + re.sub(r'[^A-Za-z0-9]', '-', rel))" "$T/proj/cba" "$T")
D=$T/home/.omp/agent/sessions/$B
mkdir -p $D $T/home/.omp/agent $T/proj/abc
printf '{"type":"title","title":"t"}\n{"type":"session","version":3,"id":"s1","cwd":"%s/proj/abc"}\n' "$T" > $D/2026-s1.jsonl
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.omp/agent/history.db")
con.execute("CREATE TABLE history (id INTEGER PRIMARY KEY, prompt TEXT, cwd TEXT)")
con.execute("INSERT INTO history VALUES (1, 'hi', ?)", (t + "/proj/abc",))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents omp --yes
[ -d $T/home/.omp/agent/sessions/$BN ] && [ ! -d $D ]
grep -q "$T/proj/cba" $T/home/.omp/agent/sessions/$BN/2026-s1.jsonl
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.omp/agent/history.db")
assert con.execute("SELECT cwd FROM history").fetchone()[0] == t + "/proj/cba"
print("omp: bucket + history.cwd rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $D ]
echo "omp: undo restored OK"
EOF

CMD ["echo", "omp adapter verification passed"]
