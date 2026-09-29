# OpenAI Codex CLI verification container: npm-installs the real CLI,
# then runs a scan/migrate/undo round trip (rollout session_meta cwd +
# config.toml [projects] trust keys + state_5.sqlite threads.cwd /
# project_roots.path).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates nodejs npm python3 \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @openai/codex && codex --version

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
S=$T/home/.codex/sessions/2026/09/03
mkdir -p $S $T/proj/abc
printf '{"type":"session_meta","payload":{"id":"u1","cwd":"%s/proj/abc"}}\n{"type":"response_item","payload":{"type":"message","content":"hi"}}\n' "$T" > $S/rollout-x.jsonl
printf '[projects."%s/proj/abc"]\ntrust_level = "trusted"\n' "$T" > $T/home/.codex/config.toml
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.codex/state_5.sqlite")
con.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT)")
con.execute("INSERT INTO threads VALUES ('t1', ?)", (t + "/proj/abc",))
con.execute("CREATE TABLE project_roots (project_id TEXT, position INTEGER, path TEXT)")
con.execute("INSERT INTO project_roots VALUES ('pr1', 0, ?)", (t + "/proj/abc",))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents codex --yes
head -1 $S/rollout-x.jsonl | grep -q "$T/proj/cba"
grep -q "$T/proj/cba" $T/home/.codex/config.toml && ! grep -q "$T/proj/abc" $T/home/.codex/config.toml
echo "codex: session_meta cwd + trust key rekeyed OK"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.codex/state_5.sqlite")
assert con.execute("SELECT cwd FROM threads").fetchone()[0] == t + "/proj/cba"
assert con.execute("SELECT path FROM project_roots").fetchone()[0] == t + "/proj/cba"
print("codex: state_5.sqlite threads.cwd + project_roots.path rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
head -1 $S/rollout-x.jsonl | grep -q "$T/proj/abc"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.codex/state_5.sqlite")
assert con.execute("SELECT cwd FROM threads").fetchone()[0] == t + "/proj/abc"
assert con.execute("SELECT path FROM project_roots").fetchone()[0] == t + "/proj/abc"
print("codex: undo restored state db OK")
PY
echo "codex: undo restored OK"
EOF

CMD ["echo", "codex adapter verification passed"]
