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
printf '{"x":1}' > $T/proj/abc/.crush/crush.db
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents crush --yes
python3 - <<PY
import json, os
t = os.environ.get("T", "/tmp/verify")
d = json.load(open(t + "/home/.local/share/crush/projects.json"))
by = {p["path"]: p for p in d["projects"]}
assert t + "/proj/cba" in by and t + "/proj/abc" not in by
assert by[t + "/proj/cba"]["data_dir"] == t + "/proj/cba/.crush"
assert "/other" in by and by["/other"]["data_dir"] == "/other/.crush"
print("crush: projects.json path/data_dir rekeyed; other projects untouched")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
grep -q "$T/proj/abc" $T/home/.local/share/crush/projects.json
echo "crush: undo restored OK"
EOF

CMD ["echo", "crush adapter verification passed"]
