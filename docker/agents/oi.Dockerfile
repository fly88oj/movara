# Open Interpreter verification container: fetches the real Rust CLI
# binary from GitHub releases (best effort), then runs a
# scan/migrate/undo round trip against synthetic state shaped like the
# Codex-rebased layout (~/.openinterpreter).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

# the real CLI from GitHub releases (asset: open-interpreter-package-x86_64-…)
RUN curl -fsSL https://api.github.com/repos/openinterpreter/openinterpreter/releases/latest \
    | grep -oE '"browser_download_url": *"[^"]*open-interpreter-package-x86_64-unknown-linux-musl.tar.gz"' \
    | grep -oE 'https[^"]*' | head -1 \
    | xargs -r curl -fsSL -o /tmp/oi.tar.gz \
    && tar -xzf /tmp/oi.tar.gz -C /tmp \
    && find /tmp -maxdepth 3 -type f -name 'interpreter*' -perm -u+x -exec cp {} /usr/local/bin/interpreter \; \
    && chmod +x /usr/local/bin/interpreter && interpreter --version \
    || echo "release channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
OI=$T/home/.openinterpreter
SESS=$OI/sessions/2026/09/01
mkdir -p $SESS $OI $T/proj/abc
printf '{"type": "session_meta", "payload": {"id": "u1", "cwd": "%s/proj/abc"}}\n' "$T" > $SESS/rollout-x.jsonl
printf '[projects."%s/proj/abc"]\ntrust_level = "trusted"\n' "$T" > $OI/config.toml
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.openinterpreter/state_5.sqlite")
con.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, cwd TEXT NOT NULL)")
con.execute("INSERT INTO threads VALUES ('t1', '/x/r.jsonl', ?)", (t + "/proj/abc",))
con.commit()
con2 = sqlite3.connect(t + "/home/.openinterpreter/memories_1.sqlite")
con2.execute("CREATE TABLE memories (id INTEGER PRIMARY KEY, body TEXT)")
con2.execute("INSERT INTO memories VALUES (1, ?)", ("project lives at " + t + "/proj/abc",))
con2.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents openinterpreter --yes
grep -q "$T/proj/cba" $SESS/rollout-x.jsonl
grep -q "$T/proj/cba" $OI/config.toml
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.openinterpreter/state_5.sqlite")
assert con.execute("SELECT cwd FROM threads").fetchone()[0] == t + "/proj/cba"
mem = sqlite3.connect(t + "/home/.openinterpreter/memories_1.sqlite")
assert t + "/proj/cba" in mem.execute("SELECT body FROM memories").fetchone()[0]
print("openinterpreter: rollout + trust + threads.cwd + memory sweep OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $SESS/rollout-x.jsonl
echo "openinterpreter: undo restored OK"
EOF

CMD ["echo", "openinterpreter adapter verification passed"]
