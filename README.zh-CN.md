# Movara

[English](README.md) | **[简体中文](README.zh-CN.md)** | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

让 AI 编程 Agent 的工作区状态可迁移。

```
~/abc  ──改名──>  ~/cba
  └─ 各家 Agent 记录的 /home/me/abc  ──movara──>  /home/me/cba
```

## 起因

大多数 AI 编程 Agent（Claude Code、Codex、Gemini CLI 系、OpenCode、omp、
Cursor、Windsurf……）把会话记录按**项目路径**做键：目录名是路径的某种编码
（连字符化 / sha256 / md5），cwd 值存在 JSON、JSONL、SQLite 和 protobuf
文件里。目录一移动，旧会话就"消失"——其实还在磁盘上，只是键指向了不存在的
路径。`movara` 一条命令搬目录并把所有这些键迁到新路径，支持完整撤销。

## 安装

Rust 实现，无运行时依赖，支持 Linux / macOS / Windows：

```bash
# 每个打标签的 Release 由 GitHub Actions 构建并附带预编译产物：
#   tar.gz / zip（含 exe）与 .deb / .rpm / .dmg 原生安装包
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # 提供 movara 命令
```

界面语言自动跟随系统语言（English / 简体中文 / 日本語 / 한국어 / Español /
Français / Deutsch / Português），可用 `--lang` 或 `MOVARA_LANG` 覆盖。

## 用法

```bash
# 日常场景：直接代替 mv —— 搬目录 + 迁移所有 Agent 历史，一步到位
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # dst 是已存在目录时按 mv 语义移入
movara mv --dry-run ~/works/abc ~/works/cba

# 可选的日常快捷别名（写入 shell 配置文件）：
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# 查看哪些 Agent 记录了这个路径
movara scan --from ~/works/abc

# 只迁移（目录已经搬过了）
movara migrate --from ~/works/abc --to ~/works/cba --yes

# 撤销一切
movara backups
movara undo --id 20260903-131427-644777
```

`movara` 的行为：先扫描并显示哪些 Agent 状态引用了旧路径 → 确认 →
`mv` 项目目录（跨文件系统自动退化为复制+删除）→ 逐个 Agent 迁移 → 输出
报告与撤销编号。任何一步出问题，一条 `movara undo` 同时还原目录位置
和所有 Agent 状态。仅接受目录；目标已存在、父目录缺失、源==目标均拒绝。

| 选项 | 说明 |
|---|---|
| `--agents claude,codex` | 只处理指定 Agent（默认：所有已安装的） |
| `--extra-root PATH` | 额外重写任意目录树（dotfile、IDE 配置等），可重复 |
| `--deep` | 连聊天内容/日志里出现的旧路径也改写（默认只改身份字段） |
| `--backup-dir DIR` | 备份目录（默认 `~/.movara/backups`） |
| `--dry-run` | 只报告 |
| `--yes` | 跳过确认提示（mv、migrate） |
| `--json` | stdout 输出单个 JSON 文档（供脚本消费） |

## 安全

- **边界感知替换**：`/a/abc` 不会匹配 `/a/abc2`、`/a/abc-def`；
  `file://` URI、JSON 转义、子路径都正确处理。
- **派生令牌一并替换**：sha256 全串、sha256[:16]、md5、各家连字符编码目录名。
- 每次迁移前全量备份：文件与 SQLite 库先 `wal_checkpoint` 再整体拷贝，
  改名记录在案，`undo` 逐项还原；`movara` 搬的目录也会一并还原。
- 目标已存在时跳过改名并列出；`--from /` 拒绝执行。
- 建议先关掉正在运行的对应 Agent（WAL 库会警告但不会损坏）。

## 支持范围

| Agent | 状态目录 | 路径键 |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`、`~/.claude.json` | 连字符目录名 + `projects` 键 + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl` | `session_meta.payload.cwd`、`threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`、projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`、`~/.qwen/tmp/<sha256>` | 连字符目录名 + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/`、tmp/history/cache/snapshots `<sha256>` | 自有编码 + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | session/project/workspace directory 列 |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp桶>/`、history.db | home 相对连字符桶 + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`、memories/ | session.directory/path |
| Cursor（IDE+CLI） | `~/.config/Cursor/.../state.vscdb` | fsPath/file:// URI |
| Windsurf | `~/.codeium/windsurf/` + IDE state.vscdb | md5(path) + file:// URI |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` | 同 VS Code fork |
| Crush | `<project>/.crush/crush.db` + projects.json | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath |
| Continue | `~/.continue/sessions/*.json`、index.sqlite | file:// URI |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | `--encoded--` 桶 + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | 配置内绝对路径 |
| cc-connect | `~/.cc-connect/dir_history.json` | 目录 MRU + 文件名哈希 |

不支持（经调研确认）：GitHub Copilot CLI（云端权威）、Amp（服务端存储）、
claude-code-router（无路径键控状态）。

## 协作

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # 32 个测试必须全过
cargo clippy --all-targets -- -D warnings # 零警告
cargo fmt --all -- --check
pre-commit install                        # 本地钩子（与 CI 一致）
```

详见 [CONTRIBUTING.md](CONTRIBUTING.md)（适配器指南、提交规范）、
[CHANGELOG.md](CHANGELOG.md)（版本历史）、[SECURITY.md](SECURITY.md)、
[docs/research.zh-CN.md](docs/research.zh-CN.md)（各 Agent 存储格式与来源）。

## 许可

Copyright (C) 2026 Movara contributors.

本项目采用双许可：Apache License 2.0 或 MIT license，任选其一。
详见 [LICENSE-APACHE](LICENSE-APACHE) 与 [LICENSE-MIT](LICENSE-MIT)。
