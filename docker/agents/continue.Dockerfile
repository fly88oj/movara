# Continue verification container: npm-installs the real CLI (best
# effort), then runs a scan/migrate/undo round trip (session
# workspaceDirectory file:// URIs + index.sqlite tag_catalog.dir).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 nodejs npm \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g continue-cli && continue --version || echo "npm channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
S=$T/home/.continue/sessions
IDX=$T/home/.continue/index
mkdir -p $S $IDX $T/proj/abc
printf '{"sessionId":"uuid1","title":"t","workspaceDirectory":"file://%s/proj/abc","history":[]}' "$T" > $S/uuid1.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.continue/index/index.sqlite")
con.execute("CREATE TABLE tag_catalog (dir TEXT, branch TEXT, artifactId TEXT, path TEXT, cacheKey TEXT)")
con.execute("INSERT INTO tag_catalog VALUES (?, 'main', 'a', 'p', 'c')", (t + "/proj/abc",))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents continue --yes
grep -q "file://$T/proj/cba" $S/uuid1.json
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.continue/index/index.sqlite")
assert con.execute("SELECT dir FROM tag_catalog").fetchone()[0] == t + "/proj/cba"
print("continue: workspace URI + tag_catalog.dir rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "file://$T/proj/abc" $S/uuid1.json
echo "continue: undo restored OK"
EOF

CMD ["echo", "continue adapter verification passed"]
