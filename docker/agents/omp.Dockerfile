# Oh My Pi (omp) verification container: npm-installs the real CLI
# (best effort), then runs scan/migrate/undo round trips (session
# buckets keyed by the upstream omp path encoding + history.db cwd).
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
# omp bucket (upstream getDefaultSessionDirName): canonicalized cwd with
# ONLY '/' '\' ':' mapped to '-' (underscores, dots and spaces ride
# verbatim); home itself -> "-"; under home -> "-" + rel; under the OS
# temp dir -> "-tmp-" + rel; anywhere else -> "--<path minus its leading
# separator>--"
obucket() {
python3 -c '
import sys
p, home, tmp = sys.argv[1], sys.argv[2], sys.argv[3]
enc = lambda s: s.replace("/", "-")  # "\" and ":" map the same way
if p == home:
    print("-")
elif p.startswith(home + "/"):
    print("-" + enc(p[len(home) + 1:]))
elif p.startswith(tmp + "/"):
    print("-tmp-" + enc(p[len(tmp) + 1:]))
else:
    print("--" + enc(p.lstrip("/")) + "--")
' "$1" "$T/home" "/tmp"
}
B=$(obucket "$T/proj/abc")
BN=$(obucket "$T/proj/cba")
BH=$(obucket "$T/home/my_repo")
BHN=$(obucket "$T/home/my_repo2")
BO=$(obucket "/opt/repo")
BON=$(obucket "/opt/repo2")
D=$T/home/.omp/agent/sessions/$B
DH=$T/home/.omp/agent/sessions/$BH
DO=$T/home/.omp/agent/sessions/$BO
mkdir -p $D $DH $DO $T/home/.omp/agent $T/proj/abc $T/home/my_repo /opt/repo
printf '{"type":"title","title":"t"}\n{"type":"session","version":3,"id":"s1","cwd":"%s/proj/abc"}\n' "$T" > $D/2026-s1.jsonl
printf '{"type":"title","title":"u"}\n{"type":"session","version":3,"id":"s2","cwd":"%s/home/my_repo"}\n' "$T" > $DH/2026-s2.jsonl
printf '{"type":"title","title":"v"}\n{"type":"session","version":3,"id":"s3","cwd":"/opt/repo"}\n' > $DO/2026-s3.jsonl
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.omp/agent/history.db")
con.execute("CREATE TABLE history (id INTEGER PRIMARY KEY, prompt TEXT, cwd TEXT)")
con.execute("INSERT INTO history VALUES (1, 'hi', ?)", (t + "/proj/abc",))
con.commit()
PY
export MOVARA_HOME=$T/home
# the underscore bucket (under home) and the outside-home bucket each
# get their own migration first, so the primary abc migration below is
# the newest backup (the undo at the end targets it)
/src/target/debug/movara migrate --from $T/home/my_repo --to $T/home/my_repo2 --agents omp --yes
/src/target/debug/movara migrate --from /opt/repo --to /opt/repo2 --agents omp --yes
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents omp --yes
[ -d $T/home/.omp/agent/sessions/$BN ] && [ ! -d $D ]
grep -q "$T/proj/cba" $T/home/.omp/agent/sessions/$BN/2026-s1.jsonl
[ -d $T/home/.omp/agent/sessions/$BHN ] && [ ! -d $DH ]
grep -q "$T/home/my_repo2" $T/home/.omp/agent/sessions/$BHN/2026-s2.jsonl
[ -d $T/home/.omp/agent/sessions/$BON ] && [ ! -d $DO ]
grep -q "/opt/repo2" $T/home/.omp/agent/sessions/$BON/2026-s3.jsonl
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
