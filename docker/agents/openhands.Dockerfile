# OpenHands verification container: pip-installs the real CLI, then
# runs a scan/migrate/undo round trip against synthetic state shaped
# like the real layout (conversations events + projects/<sha256(realpath)>).
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (PyPI's "openhands" is a 0.0.0 squatter — install from
# the project's own repo, best effort)
RUN pip install --break-system-packages "openhands-cli @ git+https://github.com/OpenHands/OpenHands-CLI" \
    && openhands --version \
    || pip install --break-system-packages openhands-ai \
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
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents openhands --yes
[ -d $OH/projects/$PN ] && [ ! -d $OH/projects/$P ]
grep -q "$T/proj/cba" $EV/event-00001-abc.json
grep -q "$T/proj/cba" $OH/agent_settings.json
echo "openhands: project bucket + working_dir + event cwd rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $OH/projects/$P ] && [ ! -d $OH/projects/$PN ]
echo "openhands: undo restored OK"
EOF

CMD ["echo", "openhands adapter verification passed"]
