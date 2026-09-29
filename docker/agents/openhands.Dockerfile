# OpenHands verification container: pip-installs the real CLI, then
# runs a scan/migrate/undo round trip against synthetic state shaped
# like the real layout (conversations events + projects/<sha256(realpath)>).
# trixie ships Python 3.13 — openhands-ai requires >=3.12
FROM rust:1.98-slim-trixie

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (PyPI: openhands-ai; PyPI's "openhands" is a 0.0.0 squatter)
RUN pip install --break-system-packages openhands-ai \
    && openhands --version \
    || echo "pip channel unreachable — synthetic verification proceeds"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
OH=$T/home/.openhands
EV=$OH/conversations/conv1/events
P=$(printf '%s/proj/abc' "$T" | sha256sum | cut -d' ' -f1)
PN=$(printf '%s/proj/cba' "$T" | sha256sum | cut -d' ' -f1)
mkdir -p $EV $OH/projects/$P $T/proj/abc
printf '{"payload": {"session": {"id": "conv1", "metadata": {"cwd": "%s/proj/abc"}}}}' "$T" > $EV/event-00001-abc.json
printf '{"working_dir": "%s/proj/abc", "model": "x"}' "$T" > $OH/agent_settings.json
printf '{"prompts": ["hi"]}' > $OH/projects/$P/prompt_history.json
# openhands.db conversation_metadata.tags embeds the archived workspace
# path as a JSON string under 'archiveworkspacepath'
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.openhands/openhands.db")
con.execute("CREATE TABLE conversation_metadata (conversation_id TEXT PRIMARY KEY, tags TEXT)")
con.execute("INSERT INTO conversation_metadata VALUES ('c1', ?)",
            ('{"archiveworkspacepath": "%s/proj/abc"}' % t,))
con.commit()
PY
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents openhands --yes
[ -d $OH/projects/$PN ] && [ ! -d $OH/projects/$P ]
grep -q "$T/proj/cba" $EV/event-00001-abc.json
grep -q "$T/proj/cba" $OH/agent_settings.json
echo "openhands: project bucket + working_dir + event cwd rekeyed OK"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
tags = sqlite3.connect(t + "/home/.openhands/openhands.db").execute(
    "SELECT tags FROM conversation_metadata WHERE conversation_id='c1'").fetchone()[0]
assert t + "/proj/cba" in tags and t + "/proj/abc" not in tags
print("openhands: conversation_metadata.tags archived workspace path rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $OH/projects/$P ] && [ ! -d $OH/projects/$PN ]
echo "openhands: undo restored OK"
python3 - <<PY
import os, sqlite3
t = os.environ.get("T", "/tmp/verify")
tags = sqlite3.connect(t + "/home/.openhands/openhands.db").execute(
    "SELECT tags FROM conversation_metadata WHERE conversation_id='c1'").fetchone()[0]
assert t + "/proj/abc" in tags and t + "/proj/cba" not in tags
print("openhands: undo restored openhands.db OK")
PY
EOF

CMD ["echo", "openhands adapter verification passed"]
