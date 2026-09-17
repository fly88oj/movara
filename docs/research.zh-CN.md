# 调研报告：AI Agent 本地会话的路径关联方式

调研时间 2026-09-03。方法：本机实盘检查（Linux，Claude Code 2.1.x / Codex
0.122 / qwen-code 0.21 / iflow 0.5.x / opencode 1.1.36 / omp 18 / Cursor /
Windsurf / Antigravity / zed / Factory agent / pi 0.80 / crush 配置存在）+
GitHub 源码/官方文档核对（见各节来源）。除标注"未证实"外，所有编码/哈希
算法都在本机数据上双向验证过。

## 1. 按目录名编码键控的 Agent

### Claude Code
- 会话：`~/.claude/projects/<encoded>/<sessionId>.jsonl`，每条消息带 `cwd`。
- 编码：路径中**每个非字母数字字符替换为 `-`**（`/`、`.`、`_` 都算），
  有损不可逆；`.paperclip` → `--paperclip`。
- `~/.claude.json` 顶层 `projects` 字典的键是**原始绝对路径**（权威映射）。
- `~/.claude/history.jsonl` 每行 `project` 字段。
- 迁移点：重命名 projects 子目录（含 `-` 前缀歧义注意）+ `projects` 键 +
  history.project + jsonl 内 `cwd`（`--deep` 才动内容）。
- 来源：官方文档 code.claude.com/docs/en/claude-directory；issue #1516
  （官方 workaround 就是重命名编码目录）、#18829（编码歧义）；
  harnez.ai/posts/fix-broken-project-paths。

### omp（Oh My Pi）— pi 的分支
- 会话桶：`~/.omp/agent/sessions/<encoded-cwd>/`。编码规则（官方
  docs/session.md）：**规范化(realpath)后的 cwd**；home 下取相对 home 的
  部分，其余取全路径；`/`→`-`。例：`-works-foo`（不是 `-home-u-works-foo`）。
- header `{"type":"session","cwd":...}`；`history.db` 的 `history.cwd` 列。
- 来源：github.com/can1357/oh-my-pi/blob/main/docs/session.md、
  packages/coding-agent/src/session/history-storage.ts。

### pi coding agent
- 会话桶：`~/.pi/agent/sessions/--<encoded>--/`，`/ \ :`→`-`，双 `--` 包裹。
- `~/.pi/agent/projects-memory/<basename>/` 来自第三方记忆扩展
  （pi-hermes-memory 等），非核心。
- 来源：github.com/earendil-works/pi packages/coding-agent/src/core/session-manager.ts。

### Factory Droid（agent）
- 会话桶：`~/.factory/sessions/<encoded>/`。编码（二进制反编译 v0.211）：
  `~` 展开 → resolve → **存在则 realpath** → 去首尾 `/` → 连续 `/`→单个
  `-` → 前置 `-`。**只替换斜杠，`.`/`_` 保留**（与 Claude 不同）。
- 来源：docs.factory.ai/droid-cli/settings、二进制内嵌文档。

### Qwen Code
- 会话：`~/.qwen/projects/<sanitize(cwd)>/chats/*.jsonl`；
  `sanitize = cwd.replace(/[^a-zA-Z0-9]/g,'-')`（Windows 先小写）。
- 临时/检查点：`~/.qwen/tmp/<sha256(cwd)>/`。
- 归属过滤：读文件首条记录的 `cwd`，`sha256(recordCwd)===sha256(当前cwd)`
  才显示 —— **只改桶名不改记录内 cwd 会话仍不显示**，两者都要改。
- 来源：github.com/QwenLM/qwen-code packages/core/src/utils/paths.ts、
  services/sessionService.ts。

### iFlow CLI
- 会话：`~/.iflow/projects/<fromPath(cwd)>/session-<uuid>.jsonl`；
  `fromPath`：去前导 `/`，`\ / : \s`→`-`，非 `[\w\-_.]`→`-`，缺前导 `-`
  则补，**连续 `-` 折叠**。
- `tmp/ history/ cache/ snapshots/` 下按 `sha256(projectRoot)` 命名。
- 注意：0.5.x 的 `-p` 非交互模式实测不落盘会话（本机验证）。
- 来源：npm @iflow-ai/iflow-cli bundle 逆向、官方 checkpointing 文档。

### Cursor CLI
- `~/.cursor/projects/<encoded>/`：去前导 `/` 后 `/`→`-`，**无前导连字符**
  （`home-user-...`，与 Claude 的 `-home-...` 不同）。
- 新版 CLI 另有 `~/.cursor/chats/*/*/store.db`（本机未出现）。
- 来源：agentgrep.org/backends/cursor-cli、本机实证。

## 2. 按哈希键控的 Agent

| Agent | 哈希 | 用在哪 | 本机验证 |
|---|---|---|---|
| Gemini CLI | `sha256(cwd)` hex | chats json 的 `projectHash` 字段 | ✅ 逐字节比对 |
| Qwen / iFlow | `sha256(cwd)` | `tmp/`（iFlow 还有 history/cache/snapshots） | ✅ |
| zcode | `sha256(cwd)[:16]` | `~/.zcode/cli/memories/projects/<basename>-<hash16>` | ✅ |
| Windsurf | `md5(去掉 file:// 的路径)` | `~/.codeium/windsurf/context_state|database/<32hex>`、state.vscdb 里 `cachedWorkspaceInfosResponse:<hash>` 键 | ✅（多根 workspace 用 workspace.json 路径，未展开支持） |
| cc-connect | `sha256(workDir)[:4]` 字节 = 8 hex | `sessions/<项目名>_<hash>.json` 文件名 | 源码证实 |
| OpenCode | `sha1("git-remote:"+归一化URL)` / 根 commit / `"global"` | `project.id`（**与本地路径无关**，目录改名 id 不变） | ✅ 本机 id 非 sha1/sha256(路径) |

## 3. 按数据库列键控的 Agent

- **Codex**：`sessions/**/rollout-*.jsonl` 首行 `session_meta.payload.cwd`
  （另有 turn_context/world_state 里嵌入的 cwd XML，属内容层）；0.147+
  `state_*.sqlite` `threads.cwd`（resume picker 按当前 cwd 过滤，
  `--all` 关闭）；config.toml `[projects."<path>"]` 信任条目。
- **OpenCode**：opencode.db `project.worktree`、`session.directory/path`、
  `workspace.directory`、`project_directory`；`event.data`/`message.data`
  JSON 内嵌 directory；启动时 refresh 会删掉目录不存在的
  project_directory 行（所以必须 UPDATE 而不是等自愈）。
- **zcode**：db.sqlite `session.directory/path`、`workflow_run.cwd`；
  agents/exec/artifacts 的 metadata.json 带 workspace。
- **omp**：history.db `history.cwd`。
- **Zed**：`~/.local/share/zed/threads/threads.db` `threads.folder_paths`
  （`\n` 连接的排序绝对路径）；`db/0-stable/db.sqlite`
  `sidebar_threads.folder_paths/main_worktree_paths`（JSON 数组）、
  `trusted_worktrees.absolute_path`；消息体在 zstd 压缩 blob 里（不动）。
- **Continue**：sessions/*.json 顶层 `workspaceDirectory`（`file://` URI）；
  index.sqlite `tag_catalog.dir`。
- **Cursor IDE**：`~/.config/Cursor/User/globalStorage/state.vscdb`
  `ItemTable.value`（JSON 串）+ `cursorDiskKV.value`（composerData:/
  bubbleId: 行里带 `fsPath` 与 `file://` URI）；workspaceStorage/
  `<id>/workspace.json` `folder`。workspaceStorage 目录名是 VS Code 内部
  id（试过 md5/sha1/sha256 的 7 种变体都不匹配）→ 只改内容不改目录名，
  聊天记录在 globalStorage，靠 URI 重写重新关联。
- **Windsurf IDE**：state.vscdb ItemTable 键 `codeium.windsurf` 的
  `windsurf.workspaceCascadeMap:{"file:///<path>":"<cascade-uuid>"}` 是
  会话↔工作区映射（迁移关键点）；`cascade/*.pb` 当前版本加密（高熵无明
  文路径，strings 验证），旧版可解的社区工具对新版无效。

## 4. 无路径键 / 随项目移动的 Agent

- **Crush**：会话库 `<project>/.crush/crush.db`（sessions 表无 cwd 列，
  靠库文件物理位置归属项目）；只需改全局
  `~/.local/share/crush/projects.json` 的 `path`/`data_dir`。
- **Aider**：`.aider*` 历史文件在项目根目录内，随目录移动；仅
  `~/.aider.conf.yml` 可能引用绝对路径。
- **claude-code-router**：无会话存储、无路径键控状态（源码证实）。
- **GitHub Copilot CLI**：本地 `~/.copilot/session-store.db` schema 未公
  开，云端为权威副本 → 建议云同步恢复，不做适配。
- **Amp**：线程存服务端（ampcode.com/feed），本地 `~/.local/share/amp/
  threads/T-*.json` 只是镜像 → 不适配。

## 5. 迁移算法（本工具实现）

1. **目录改名**：各家编码（dash / dash-nolead / omp 桶 / `--enc--` /
   droid 斜杠 / iflow 折叠 / basename-slug）+ 哈希目录（sha256 全程、
   sha256[:16]、md5、sha256[:8] 文件名后缀）。桶名带前缀匹配以覆盖
   子项目会话（`/old/sub` 的桶以 enc(old) 为前缀）。
2. **身份字段改写**：JSON/JSONL 递归只动身份键（cwd、directory、
   project、workspaceDirectory、workspace_roots 数组……）与路径型字典
   键（claude.json `projects`、gemini projects.json）。
3. **派生令牌替换**：sha256(old)→sha256(new) 等，同一边界规则。
4. **SQLite**：逐表字面量 SQL + 参数绑定；改前 `wal_checkpoint` + 整库
   备份；opencode 改后 `VACUUM` 清理空闲页残留。
5. **protobuf**：通用 wire-format 游走，重写含旧路径的 length-delimited
   字段并修正 varint 长度（windsurf/antigravity）。
6. **`--deep`**：日志 / 聊天内容 / 环境上下文 XML 里的旧路径也替换。
7. **undo**：逆序还原目录改名（含记录路径→原始位置的映射）、还原文件、
   还原 SQLite 整库。

## 6. 未证实 / 已知限制

- VS Code 系 `workspaceStorage/<id>` 目录名算法（未破解，靠内容重写）。
- Windsurf 多根 workspace 的 md5 输入（用 workspace.json 路径，未支持）。
- Cursor CLI `~/.cursor/chats` store.db（本机该版本未产生）。
- Codex 0.147 `state_5.sqlite`（本机 0.122 无此文件，按源码适配）。
- iflow `-p` 不落盘会话（0.5.x 实测），无法做功能性 resume 验证。
- zcode/zed 的桌面 GUI 未做端到端启动验证（按真实 schema 做了库级验证）。

## 主要来源

- Claude Code: code.claude.com/docs/en/claude-directory; anthropics/claude-code #1516 #18829 #21085; harnez.ai/posts/fix-broken-project-paths
- Codex: openai/codex #22037 #31317; zread.ai/openai/codex/10-rollout-and-state-persistence
- Gemini: google-gemini/gemini-cli packages/core/src/config/projectRegistry.ts, utils/paths.ts
- Qwen: QwenLM/qwen-code packages/core/src/{utils/paths.ts, config/storage.ts, services/sessionService.ts}
- iFlow: @iflow-ai/iflow-cli bundle; docs_en/features/checkpointing.md
- omp: can1357/oh-my-pi docs/session.md, src/session/history-storage.ts, #8323
- pi: earendil-works/pi packages/coding-agent/src/core/session-manager.ts, docs/session-format.md
- OpenCode: sst/opencode packages/core/src/{project.ts, project/sql.ts, session/sql.ts, util/hash.ts, database/database.ts}, packages/opencode/src/session/session.ts
- Crush: charmbracelet/crush internal/{config/load.go, config/config.go, db/connect.go, projects/projects.go, db/migrations/*}
- Factory: docs.factory.ai/droid-cli/settings; @factory/cli 二进制 strings
- cc-connect: chenhg5/cc-connect core/dir_history.go, cmd/cc-connect/main.go
- CCR: musistudio/claude-code-router packages/core/src/runtime/app-paths.ts 等
- Cursor: agentgrep.org/backends/{cursor-ide,cursor-cli}; vibe-replay.com/blog/cursor-local-storage; forum.cursor.com #143475 #152450 #165486; github.com/S2thend/cursor-history
- Windsurf: Exafunction/codeium #127 #136; agent-steward; 本机实证（md5 算法、加密 .pb）
- Continue: continuedev/continue core/util/paths.ts; docs.continue.dev
- Zed: zed-industries/zed crates/{agent/src/db.rs, util/src/path_list.rs, paths/src/paths.ts}; discussions #32335
- Copilot: docs.github.com copilot-cli chronicle / overview; jonmagic.com posts
- Amp: ampcode.com/security; docs.rs/ampcode
