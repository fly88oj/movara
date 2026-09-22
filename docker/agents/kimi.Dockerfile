# Kimi Code verification container — the incident reproduction.
#
# Installs the REAL CLI from Moonshot's official CDN
# (code.kimi.com/kimi-code/binaries/<ver>/kimi-code-linux-x64), seeds
# state shaped like a real workspace, migrates, and then gates on the
# AGENT'S OWN VIEW — `kimi session list` from the new project path
# must list the session under the new workspace. Disk greps alone
# proved insufficient in the wild (the cache/query-store incident):
# the agent's own listing is the acceptance test.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 xz-utils zstd \
    && rm -rf /var/lib/apt/lists/*

# the real CLI (official CDN: latest.json -> binaries/<ver>/manifest.json)
RUN V=$(curl -fsSL https://code.kimi.com/kimi-code/latest.json | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])') \
    && F=$(curl -fsSL "https://code.kimi.com/kimi-code/binaries/$V/manifest.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["platforms"]["linux-x64"]["filename"])') \
    && mkdir -p /root/.kimi-code/bin \
    && curl -fsSL "https://code.kimi.com/kimi-code/binaries/$V/$F" -o /root/.kimi-code/bin/kimi \
    && chmod +x /root/.kimi-code/bin/kimi \
    && /root/.kimi-code/bin/kimi --version
ENV PATH="/root/.kimi-code/bin:${PATH}"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
export HOME=$T/home
K=$HOME/.kimi-code
B=$(python3 -c "
import sys, hashlib
p = sys.argv[1]
print('wd_' + p.rsplit('/',1)[-1] + '_' + hashlib.sha256(p.encode()).hexdigest()[:12])" "$T/proj/abc")
BN=$(python3 -c "
import sys, hashlib
p = sys.argv[1]
print('wd_' + p.rsplit('/',1)[-1] + '_' + hashlib.sha256(p.encode()).hexdigest()[:12])" "$T/proj/cba")
S=se_11111111-2222-3333-4444-555555555555
SESSDIR=$K/sessions/$B/session_11111111-2222-3333-4444-555555555555/agents/main
mkdir -p $SESSDIR $K/workspace-trust $K/server/events $K/cache $T/proj/abc
# workspaces.json registry
printf '{"version":1,"workspaces":{"%s":{"root":"%s/proj/abc","name":"abc","created_at":"2026-09-01T00:00:00.000Z","last_opened_at":"2026-09-01T00:00:00.000Z"}}}' "$B" "$T" > $K/workspaces.json
# session index (workDir + sessionDir through the bucket)
printf '{"sessionId":"%s","sessionDir":"%s","workDir":"%s/proj/abc"}\n' "$S" "$SESSDIR" "$T" > $K/session_index.jsonl
# per-session state + wire (workspace binding!)
printf '{"workDir":"%s/proj/abc","agents":{"main":{"homedir":"%s","type":"main"}}}' "$T" "$SESSDIR" > $SESSDIR/../../state.json
printf '{"type":"metadata","protocol_version":"1.4"}\n{"type":"session.start","workDir":"%s/proj/abc"}\n{"type":"runtime.set_binding","workspaceId":"%s"}\n' "$T" "$B" > $SESSDIR/wire.jsonl
# trust + file-history bucket FILES
printf '{"root":"%s/proj/abc","trustedAt":1789975118890}' "$T" > $K/workspace-trust/$B
printf '{"sessions":[{"id":"session_1","touchedAt":1789975118890}]}' > /dev/null
mkdir -p $K/file-history && printf '{"sessions":[]}' > $K/file-history/$B
# server event stream (workspace registry + session identity)
printf '{"kind":"event","seq":1,"envelope":{"type":"event.workspace.updated","seq":1,"payload":{"workspace":{"id":"%s","root":"%s/proj/abc","name":"abc"}}}}\n' "$B" "$T" > $K/server/events/__global__.jsonl
printf '{"kind":"event","seq":1,"envelope":{"type":"event.session.created","seq":1,"payload":{"session":{"id":"%s","workspace_id":"%s","metadata":{"cwd":"%s/proj/abc"}}}}}\n' "$S" "$B" "$T" > $K/server/events/$S.jsonl
# a stale derived store that must be invalidated
mkdir -p $K/cache/query-store/shard-00 && printf 'STALE' > $K/cache/query-store/shard-00/db.wal

/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents kimi --yes

# disk layer: bucket renamed, registry rekeyed, derived store gone
[ -d $K/sessions/$BN ] && [ ! -d $K/sessions/$B ]
[ -f $K/workspace-trust/$BN ] && [ ! -f $K/workspace-trust/$B ]
grep -q "$T/proj/cba" $K/workspaces.json && ! grep -q "$T/proj/abc" $K/workspaces.json
grep -q "$T/proj/cba" $K/session_index.jsonl && ! grep -q "$T/proj/abc" $K/session_index.jsonl
[ ! -e $K/cache/query-store ]
echo "kimi: bucket + registry + index + derived-store invalidation OK"

# THE AGENT'S OWN VIEW — the acceptance test the incident taught us:
# session list from the NEW path must show the session
mkdir -p $T/proj/cba
cd $T/proj/cba
OUT=$(kimi session list 2>&1 || true)
echo "kimi session list: $OUT"
echo "$OUT" | grep -q "11111111" || { echo "FAIL: session not listed from new path"; exit 1; }
echo "kimi: agent's own session list sees the migrated session OK"
EOF

CMD ["echo", "kimi adapter verification passed (agent's own view)"]
