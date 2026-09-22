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
## 2026-09-22 市场普查增补（11 个新 Agent）

普查来源：社区/awesome 列表、编排类项目支持矩阵（Vibe Kanban、claude-code-router 等）、GitHub topics。约 30 个候选中，以下 12 个具备可验证的本地会话/历史存储并已落地适配器；存储事实均对照各项目开源仓库核实，Qoder/Trae/Copilot/Kimi 另经本机真实状态验证。

### Goose（Block）— 54.5k★
- `~/.local/share/goose/sessions/sessions.db`（SQLite/WAL/schema v9）：`sessions.working_dir`。旧版为扁平 `sessions/*.jsonl`，首行会话元数据含 `working_dir`。
- 配置 `~/.config/goose/permissions/tool_permissions.json` 按绝对路径键控。
- 跨 OS 根按 etcetera 策略（macOS `Block.block.goose` bundle 目录；Windows `%APPDATA%\Block\goose`）。无路径派生桶名；`messages.content_json` 属正文层（--deep）。

### Cline / Roo Code / Kilo Code（VS Code 扩展家族）
- 每个市场 id 一个 globalStorage（`saoudrizwan.claude-dev`、`rooveterinaryinc.roo-cline`、`kilocode.kilo-code`），出现在每个所运行的 IDE（Code/Cursor/Windsurf/VSCodium/vscode-server）下。
- 任务历史：Roo `tasks/_index.json` + `history_item.json`（`workspace` 字段）；Cline 4.x `state/taskHistory.json`（`cwdOnTaskInitialization`/旧 `shadowGitConfigWorkTree`）；Cline ≤3.x 与经典 Kilo 存于 IDE 的 state.vscdb ItemTable（按扩展键行级限定重写）。
- 检查点是影子 git，`.git/config core.worktree` 为工作区绝对路径——过期则扩展拒绝恢复。Cline ≤3.x 键为 `checkpoints/<cwdHash>/`（多项式哈希 ×31、u32、UTF-16 码元、十进制——仅按精确名改名，刻意不作文本针）；Roo/Kilo 按任务键控；经典 Kilo 另有 `checkpoints/<sha256(cwd)[:8]>/` 与 `sessions/<sha256[:16]>/`。
- Roo 的 `roo-index-cache-<sha256>.json` 可再生，迁移时删除（派生存储失效）。Roo Code 已于 2026-05 归档，磁盘上常见遗留状态。

### OpenHands
- `~/.openhands`：`conversations/<uuid>/events/event-*.json`（UUID 键控）、`agent_settings.json` 的 `working_dir`、`projects/<sha256(realpath(cwd))>/prompt_history.json`。云端会话存储为 stub。

### Codebuff / Freebuff
- `~/.config/manicode/projects/<basename>/chats/<timestamp>/`（改名后旧配置名沿用）。项目键为裸 basename——同名项目上游即共享存储（已记录的设计怪癖）；改名碰撞时大声拒绝。run-state.json 的 sessionState 内嵌 cwd。

### gptme
- `~/.local/share/gptme/logs/<YYYY-MM-DD>-<name>/`——扁平，名字来自日期+随机/用户/LLM 命名，与路径无关。项目链接是 `config.toml [chat] workspace`，home 下保存为**波浪号缩写形态**（两种形态都重写；spec 机制会把 "~" 路径接到 CWD，故波浪号形态用专用边界替换）。`workspace` 是指向项目的符号链接（重定向、永不跟随）。消息 `files` 列表携带附件路径。

### Qoder / 通义灵码（CN）
- IDE 为 VS Code fork（`~/.config/Qoder` 的 state.vscdb + workspace 存储）。home 根 `~/.qoder`：`memories/<账户哈希>/projects/<dash 编码路径>/**`——项目层是经典 dash 编码；上层 8-hex 桶为账户键控、非路径派生。灵码 CN 版已迁移到 `~/.lingma/qoder-cn`，memories 布局相同。灵码自身 `index/` 为二进制可再生索引（不动）。

### Trae（字节）
- VS Code fork 状态在 `~/.config/Trae CN`（CN 版应用目录带空格；国际版为 `Trae`）+ 薄定义根 `~/.trae`（agents/skills/mcp.json）。

### GitHub Copilot CLI
- `~/.copilot` 的 agents/hooks/skills 定义。实测 GA 安装在该根下无会话转录；定义层即本地承载面。

### Warp
- `~/.local/share/warp/warp.db`（macOS `~/.warp`）。闭源未公开 schema：适配器运行时经 PRAGMA table_info 发现全部表的 TEXT 列并按行泛化重写（边界感知模式、绑定参数）；整库记账可撤销。仅经合成 db 验证——无真实样本（已在研究注记中说明）。

### Kimi Code（月之暗面）——已真机端到端验证
- `~/.kimi-code`：桶 `wd_<basename(root)>_<sha256(root)[:12]>`（真机 34/34 命中）命名 sessions/ 目录与 file-history/、workspace-trust/ **文件**；workspaces.json（桶键+root+显示名）；session_index.jsonl（sessionDir/workDir）；每会话 state.json（workDir+homedir）与 wire.jsonl（`runtime.set_binding.workspaceId` 绑定会话与工作区）；server 事件流（`event.workspace.updated` 载荷 id 在泛型键下）。
- **派生存储持有迁移前元数据、必须失效**：`cache/query-store`（分片 WAL+generations 物化视图，server 的会话/工作区查询全走它——失效前永不回读权威文件，grep 不可见，靠 strace 现形）、`sessions/.index-cache`、`search-index`。三者迁移时删除、下次启动重建。真实事故：权威层全干净而陈旧 store 在 Web UI 复活旧工作区——验收必须用 Agent 自身视角（新路径 `kimi session list` + API/UI），磁盘 grep 干净不算数。

## 普查排除项（记录在案的范围外）

- **纯云端/服务端会话**：Google Jules（CLI 驱动云端会话）、Devin、Replit Agent、Lovable、Bolt.new、v0、Firebase Studio、Roomote（Roo 关停后的云产品）、Sweep（2026-04 关停）、MiniMax Agent（web）、CodeGeeX web 聊天。
- **混合型、仅本地配置**：Amp（线程在服务端 ampcode.com/feed；本地 `~/.local/share/amp` 为配置与线程镜像，内部格式未公开且版本不稳定——无承重内容需重键）。
- **既有适配器覆盖**：MiniMax Code 复用 `~/.local/share/opencode`（OpenCode 系；opencode 适配器已覆盖）；VS Code Copilot 聊天会话在 IDE 的 workspaceStorage（支持的 IDE 由 fork 机制覆盖）。
- **相邻工具、非会话状态**：Backlog.md（项目任务 markdown）、编排器（Vibe Kanban、claude-squad、crystal/Nimbalyst、Conductor、claude-code-router——配置可嵌路径但无会话转录）。
- **弱/不可验证的本地信号**：Junie（JetBrains；`~/.junie` 信任标记，会话子目录未证实）、Cody（v1.20 起聊天服务端同步；本地转录次要）、Refact.ai（自托管 Docker 卷）、灵码迁移前布局（已被 qoder-cn 取代，已覆盖）。
### Open Interpreter（2026 Rust CLI）
- 仓库已更名 openinterpreter/openinterpreter——Codex 重定基（codex-rs 树）；INTERPRETER_HOME 覆盖，CODEX_HOME 被刻意忽略。
- `~/.openinterpreter` 镜像 Codex 布局：sessions/**/rollout-*.jsonl 的 session_meta payload.cwd（可选 .zst，二进制跳过）；state_*.sqlite `threads.cwd`（过期 cwd 会让会话从按目录过滤的列表与 resume --last 中静默消失）；config.toml [projects] 规范化路径信任键。memories/logs/goals 架构未公开——按通用文本列扫描。无路径派生名。

### Plandex——改名安全，无需适配器
- v1/v2 均为客户端-服务器架构：计划/版本/元数据在服务端（PLANDEX_BASE_DIR + Postgres，UUID 键控 git 仓库）；`~/.plandex-home-v2/` 按 projectId/planId 键控；项目内 `.plandex-v2/` 只有 projectId 映射、随目录移动。没有任何文件内嵌项目绝对路径——Context 文件路径为项目相对；无 git worktree。项目改名零破坏（服务端显示名仅装饰性）。
