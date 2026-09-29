# ZCode verification container: the desktop IDE is not headless-
# installable — the container verifies the adapter's storage layer
# (db.sqlite session.directory/path + project_id identity columns +
# memory-key buckets + the desktop v2 side) in isolation.
FROM rust:1.98-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends git curl pkg-config libsqlite3-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build

RUN <<'EOF'
set -e
T=/tmp/verify
Z=$T/home/.zcode/cli
V=$T/home/.zcode/v2
# derived keys mirroring src/encodings.rs: slug = lowercase with every
# run outside [a-z0-9._-] collapsed to one '-', edges trimmed; project
# id = proj_ + slug(path)[:80] (empty -> default); memory key =
# slug(basename)[:48] (empty -> project) + '-' + sha256(path)[:16];
# desktop workspace hash = sha256(path)[:12]
ztok() {
python3 -c '
import hashlib, re, sys
def slug(s):
    return re.sub(r"[^a-z0-9._-]+", "-", s.lower()).strip("-")
def proj_id(p):
    return "proj_" + (slug(p)[:80] or "default")
def mem_key(p):
    base = p.rstrip("/").rsplit("/", 1)[-1]
    return (slug(base)[:48] or "project") + "-" + hashlib.sha256(p.encode()).hexdigest()[:16]
def h12(p):
    return hashlib.sha256(p.encode()).hexdigest()[:12]
print({"proj": proj_id, "mem": mem_key, "h12": h12}[sys.argv[1]](sys.argv[2]))
' "$1" "$2"
}
K=$(ztok mem "$T/proj/abc")
KN=$(ztok mem "$T/proj/cba")
P=$(ztok proj "$T/proj/abc")
PN=$(ztok proj "$T/proj/cba")
H12O=$(ztok h12 "$T/proj/abc")
H12N=$(ztok h12 "$T/proj/cba")
mkdir -p $Z/db $Z/memories/projects/$K $Z/agents/sess_1/agent_1 \
         $V/checkpoints/$H12O $T/proj/abc
python3 - "$T" "$P" <<'PY'
import json, sqlite3, sys
t, pid = sys.argv[1], sys.argv[2]
old = t + "/proj/abc"
con = sqlite3.connect(t + "/home/.zcode/cli/db/db.sqlite")
con.execute("CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, workspace_id TEXT, directory TEXT, path TEXT, title TEXT)")
con.execute("CREATE TABLE workflow_run (id TEXT PRIMARY KEY, cwd TEXT, script_path TEXT, status TEXT)")
con.execute("CREATE TABLE workflow_definition (id TEXT PRIMARY KEY, name TEXT, script_path TEXT)")
con.execute("CREATE TABLE permission (project_id TEXT PRIMARY KEY, data TEXT)")
con.execute("CREATE TABLE input_history (id TEXT PRIMARY KEY, project_id TEXT, text TEXT, attachments TEXT)")
con.execute("CREATE TABLE local_setting (scope TEXT, scope_id TEXT, namespace TEXT, key TEXT, value TEXT)")
con.execute("CREATE TABLE dwf_run (id TEXT PRIMARY KEY, cwd TEXT)")
con.execute("INSERT INTO session VALUES ('sess_1', ?, NULL, ?, ?, 't')", (pid, old, old))
con.execute("INSERT INTO workflow_run VALUES ('run_1', ?, ?, 'done')", (old, old + "/wf.ts"))
con.execute("INSERT INTO workflow_definition VALUES ('def_1', 'wf', ?)", (old + "/wf.ts",))
con.execute("INSERT INTO permission VALUES (?, '{}')", (pid,))
con.execute("INSERT INTO input_history VALUES ('ih_1', ?, 'hello', '[]')", (pid,))
ruleset = json.dumps({"version": 1,
                      "allow": [{"toolName": "Write",
                                 "ruleContent": old + "/src/**"}]})
con.execute("INSERT INTO local_setting VALUES ('project', ?, 'permission', 'ruleset', ?)",
            (pid, ruleset))
con.execute("INSERT INTO dwf_run VALUES ('dwf_1', ?)", (old,))
con.commit()
tcon = sqlite3.connect(t + "/home/.zcode/v2/tasks-index.sqlite")
tcon.execute("CREATE TABLE tasks (task_id TEXT PRIMARY KEY, workspace_key TEXT, workspace_path TEXT)")
tcon.execute("INSERT INTO tasks VALUES ('task_1', ?, ?)", (old, old))
tcon.commit()
PY
printf '{"bots": {"bot-1": {"workspacePath": "%s/proj/abc", "workspaceId": "%s/proj/abc"}}}\n' "$T" "$T" > $V/bot-state.v3.json
printf '{"recentProjects": ["%s/proj/abc"], "lastWorkspaceSession": [{"workspacePath": "%s/proj/abc"}]}\n' "$T" "$T" > $V/setting.json
printf '{"workspacePath": "%s/proj/abc"}\n' "$T" > $V/checkpoints/$H12O/state.json
printf '{"workspace": "%s/proj/abc"}' "$T" > $Z/agents/sess_1/agent_1/metadata.json
printf '# mem\n' > $Z/memories/projects/$K/MEMORY.md
export MOVARA_HOME=$T/home
/src/target/debug/movara migrate --from $T/proj/abc --to $T/proj/cba --agents zcode --yes
[ -d $Z/memories/projects/$KN ] && [ ! -d $Z/memories/projects/$K ]
[ -d $V/checkpoints/$H12N ] && [ ! -d $V/checkpoints/$H12O ]
grep -q "$T/proj/cba" $Z/agents/sess_1/agent_1/metadata.json
python3 - "$T" "$PN" <<'PY'
import hashlib, json, sqlite3, sys
t, pid = sys.argv[1], sys.argv[2]
old, new = t + "/proj/abc", t + "/proj/cba"
con = sqlite3.connect(t + "/home/.zcode/cli/db/db.sqlite")
assert con.execute("SELECT directory FROM session").fetchone()[0] == new
assert con.execute("SELECT project_id FROM session").fetchone()[0] == pid
assert con.execute("SELECT project_id FROM permission").fetchone()[0] == pid
assert con.execute("SELECT project_id FROM input_history").fetchone()[0] == pid
scope, value = con.execute(
    "SELECT scope_id, value FROM local_setting WHERE key = 'ruleset'").fetchone()
assert scope == pid
assert new in value and old not in value
assert con.execute("SELECT cwd FROM dwf_run").fetchone()[0] == new
assert con.execute("SELECT script_path FROM workflow_run").fetchone()[0] == new + "/wf.ts"
assert con.execute("SELECT script_path FROM workflow_definition").fetchone()[0] == new + "/wf.ts"
bot = json.load(open(t + "/home/.zcode/v2/bot-state.v3.json"))
assert bot["bots"]["bot-1"]["workspacePath"] == new
assert bot["bots"]["bot-1"]["workspaceId"] == new
setting = json.load(open(t + "/home/.zcode/v2/setting.json"))
assert setting["recentProjects"][0] == new
assert setting["lastWorkspaceSession"][0]["workspacePath"] == new
h12n = hashlib.sha256(new.encode()).hexdigest()[:12]
ck = json.load(open(t + "/home/.zcode/v2/checkpoints/" + h12n + "/state.json"))
assert ck["workspacePath"] == new
tcon = sqlite3.connect(t + "/home/.zcode/v2/tasks-index.sqlite")
assert tcon.execute("SELECT workspace_key, workspace_path FROM tasks").fetchone() == (new, new)
print("zcode: db identity columns + workflow paths + v2 desktop state rekeyed OK")
PY
/src/target/debug/movara undo --id $(ls $T/home/.movara/backups | tail -1)
[ -d $Z/memories/projects/$K ]
echo "zcode: undo restored OK"
EOF

CMD ["echo", "zcode adapter verification passed"]
