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

Rust 实现，无运行时依赖，支持 Linux / macOS / Windows。每个
[Release](https://github.com/fly88oj/movara/releases) 都附带预编译包——
以下文件名以 `1.2.0` 为例，请替换为实际下载的版本号。

**Debian / Ubuntu（.deb）**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL（.rpm）**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**其他 Linux 发行版（tar.gz）**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon（.dmg 或 tar.gz）**

```bash
# 打开 dmg 把 bin/movara 拷到 /usr/local/bin，或：
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Intel Mac 通过 Rosetta 2 运行 arm64 版本，或从源码构建。

**Windows（zip）**

解压 `movara-1.2.0-x86_64-pc-windows-msvc.zip`，把 `movara.exe` 所在
目录加入 `PATH`。

**从源码安装**

```bash
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

`movara mv` 的行为：先扫描并显示哪些 Agent 状态引用了旧路径 → 确认 →
`mv` 项目目录（跨文件系统自动退化为复制+删除）→ 逐个 Agent 迁移 → 输出
报告与撤销编号。任何一步出问题，一条 `movara undo` 同时还原目录位置
和所有 Agent 状态。仅接受目录；目标已存在、父目录缺失、源==目标均拒绝。

### 子命令

| 命令 | 用途 |
|---|---|
| `movara mv <SRC> <DST>` | 搬目录（DST 已存在则移入其中）并一步迁移所有 Agent——日常替代 mv |
| `movara scan --from <OLD> [--to <NEW>]` | 只读报告哪些 Agent 状态引用了某路径；`--to` 仅用于改名目标预览 |
| `movara migrate --from <OLD> --to <NEW>` | 目录已被其他方式移动后补迁移；`--move-project` 先移动目录本身 |
| `movara undo --id <ID>` | 完整逆转一次迁移（见下） |
| `movara backups` | 列出迁移日志（见下） |
| `movara agents` | 列出支持的 Agent 及安装状态 |
| `movara export [--path PATH]... [--agents LIST]` | 把 Agent 状态打包为可携带的 `.tar.gz` 档案——默认整台主机，也可按项目路径 / Agent 过滤 |
| `movara import <ARCHIVE> [--rebase OLD:NEW]...` | 在本机恢复档案并按规则重定基路径；像迁移一样记日志，可用 `movara undo` 完整撤销 |
| `movara move <SRC> [user@]host:<DST>` | 一条命令经 ssh 把项目与 Agent 状态一起搬到另一台主机——记忆随行，可选源端清理（不支持 IPv6 主机字面量） |
| `movara receive --dst <DST>` | [目标端] 从 stdin 导入流式档案（由 `move` 拉起） |

### 选项

| 选项 | 适用范围 | 说明 |
|---|---|---|
| `--lang CODE` | 全局 | 覆盖界面语言（en, zh-CN, ja, ko, es, fr, de, pt-BR） |
| `--agents LIST` | scan, migrate, mv | 逗号列表；只处理指定 Agent（默认：所有已安装的） |
| `--extra-root PATH` | scan, migrate, mv | 额外重写任意目录树（dotfile、IDE 配置等），可重复 |
| `--deep` | migrate, mv | 连聊天内容/日志里出现的旧路径也改写（默认只改身份字段——cwd、directory、project 等） |
| `--backup-dir DIR` | migrate, mv, undo, backups | 备份日志根目录（默认 `~/.movara/backups`） |
| `--dry-run` | migrate, mv | 只报告，不改动 |
| `--yes` | migrate, mv | 跳过确认提示 |
| `--move-project` | migrate | 重写引用前先移动项目目录本身 |
| `--json` | agents, scan, migrate, mv, backups | stdout 输出单个 JSON 文档（供脚本消费） |
| `--out FILE` | export | 档案路径（默认 `movara-export-<时间戳>.tar.gz`） |
| `--path PATH` | export | 只导出引用此项目路径的状态（可重复；与 `--agents` 取交集） |
| `--rebase OLD:NEW` | import | 路径映射，可重复；重叠与链式规则会被拒绝 |
| `--dst <DST>` | receive | 本机的项目目标目录 |
| `--plan-only` | receive | 仅预检目标后退出 |
| `--yes` | receive | 非交互（流式接收必填） |
| `--on-conflict POLICY` | import | `skip`（默认）或 `replace` 已存在的本地状态 |
| `--allow-missing-path` | import | 档案路径在本地不存在时仍继续 |
| `--state-only` | move | 只携带 Agent 状态与项目记忆，不带代码 |
| `--cleanup` | move | 验证成功后删除本机的已迁移状态集（共享库/配置保留） |

### 撤销与备份

每次 `mv` / `migrate` 在改动任何东西之前，都会在
`~/.movara/backups/<id>/` 写入一份日志：待改文件的副本、完整的 SQLite
数据库（先做 `wal_checkpoint`）、改名台账（含被移动的项目目录本身）。

- **`movara backups`** 列出所有日志——编号、日期、旧→新路径、涉及的
  Agent、文件/数据库/改名计数（`--json` 供脚本使用）。日志是撤销的最小
  单位：目前不支持按路径或按 Agent 过滤；磁盘空间紧张时可手动删除旧日志。
- **`movara undo --id <ID>`** 将一条日志逆向回放——文件内容还原、数据库
  换回、改名反转、被移动的目录回到原位。迁移目标选错、迁移时有 Agent
  还在运行、或者单纯想回到旧布局时使用。撤销以单次迁移为单位、整体回退：
  一个编号逆转的是整次迁移，不能只撤某个 Agent 或某个文件；同一批路径
  再次迁移之前应先执行撤销。

`export` / `import` 同样全程记日志：`undo` 可完整逆转一次导入，包括它新建的状态。
跨主机 `move` 会把项目树（含 `.git`）一并交换：`--state-only` 不带代码但保留项目内记忆文件（CLAUDE.md、AGENTS.md、rules）；`--cleanup` 只删源端已迁移的状态集——绝不删共享数据库或配置——且本身可撤销。

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
> **跨主机同步（sync）正在为下一版本开发中**——见
> [CHANGELOG.md](CHANGELOG.md) 的 Unreleased 一节。

> **跨主机同步（sync）正在为下一版本开发中**——见
> [CHANGELOG.md](CHANGELOG.md) 的 Unreleased 一节。

[CHANGELOG.md](CHANGELOG.md)（版本历史）、[SECURITY.md](SECURITY.md)、
[docs/research.zh-CN.md](docs/research.zh-CN.md)（各 Agent 存储格式与来源）。

## 许可

Copyright (C) 2026 Movara contributors.

本项目采用双许可：Apache License 2.0 或 MIT license，任选其一。
详见 [LICENSE-APACHE](LICENSE-APACHE) 与 [LICENSE-MIT](LICENSE-MIT)。
