# Trae verification container: downloads the real IDE .deb (best
# effort — trae.cn / trae.ai distribution), unpacks with dpkg-deb -x to
# verify the layout constants, then runs a scan/migrate/undo round
# trip over the IDE state.vscdb + ~/.trae definitions.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 dpkg \
    && rm -rf /var/lib/apt/lists/*

# the real IDE .deb via the official version manifest API (extracts
# under usr/share/trae-cn)
RUN URL=$(curl -fsSL "https://api.trae.cn/icube/api/v1/native/version/trae/cn/latest" | grep -oE 'https://lf-cdn[^"]*linux-x64\.deb' | head -1) \
    && [ -n "$URL" ] && curl -fL "$URL" -o /tmp/trae.deb \
    && dpkg-deb -x /tmp/trae.deb /tmp/trae-extract \
    && ls /tmp/trae-extract/usr/share/trae-cn \
    && echo "trae .deb unpacked from the official manifest"

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
GSDB="$T/home/.config/Trae CN/User/globalStorage/state.vscdb"
mkdir -p "$T/home/.config/Trae CN/User/globalStorage" $T/home/.trae $T/proj/abc
python3 - <<PY
import sqlite3, json, os
t = os.environ.get("T", "/tmp/verify")
db = t + "/home/.config/Trae CN/User/globalStorage/state.vscdb"
con = sqlite3.connect(db)
con.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)")
con.execute("INSERT INTO ItemTable VALUES ('aicode.chatSessions', ?)",
            (json.dumps({"sessions": [{"workspace": t + "/proj/abc"}]}),))
con.commit()
PY
printf '{"mcpServers":{"y":{"command":"npx","cwd":"%s/proj/abc"}}}' "$T" > $T/home/.trae/mcp.json
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents trae --yes
python3 - <<PY
import sqlite3, os
t = os.environ.get("T", "/tmp/verify")
con = sqlite3.connect(t + "/home/.config/Trae CN/User/globalStorage/state.vscdb")
v = con.execute("SELECT value FROM ItemTable WHERE key='aicode.chatSessions'").fetchone()[0]
assert t + "/proj/cba" in v and t + "/proj/abc" not in v
print("trae: IDE ItemTable rekeyed OK")
PY
grep -q "$T/proj/cba" $T/home/.trae/mcp.json
echo "trae: definitions rekeyed OK"
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $T/home/.trae/mcp.json
echo "trae: undo restored OK"
EOF

CMD ["echo", "trae adapter verification passed"]
