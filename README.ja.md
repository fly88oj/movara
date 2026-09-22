# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | **[日本語](README.ja.md)** | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

AI コーディングエージェントのワークスペース状態をポータブルに。

```
~/abc  ──リネーム──>  ~/cba
  └─ 各エージェントが記録した /home/me/abc  ──movara──>  /home/me/cba
```

## 背景

多くの AI コーディングエージェント（Claude Code、Codex、Gemini CLI 系、
OpenCode、omp、Cursor、Windsurf など）は、セッション履歴を
**プロジェクトパス**をキーとして保存します。ディレクトリ名はパスの
何らかのエンコード（ダッシュ化 / sha256 / md5）で、cwd の値は JSON・
JSONL・SQLite・protobuf ファイルの中にあります。ディレクトリを移動・
リネームすると、古いセッションは「消えた」ようになります——実際には
ディスク上に残っていて、存在しないパスに紐づいているだけです。
`movara` はディレクトリの移動と、これら全キーの新パスへの付け替えを
一括で行い、完全な取り消し（undo）に対応します。

## インストール

Rust 実装。実行時依存なし、Linux / macOS / Windows に対応。各
[Release](https://github.com/fly88oj/movara/releases) にビルド済み
パッケージが同梱されます。以下のファイル名は `1.2.0` の例なので、
ダウンロードしたバージョンに置き換えてください。

**Debian / Ubuntu（.deb）**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL（.rpm）**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**その他の Linux（tar.gz）**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon（.dmg / tar.gz）**

```bash
# dmg を開いて bin/movara を /usr/local/bin にコピー、または：
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Intel Mac は Rosetta 2 経由で arm64 ビルドを実行するか、ソースから
ビルドしてください。

**Windows（zip）**

`movara-1.2.0-x86_64-pc-windows-msvc.zip` を展開し、`movara.exe` の
あるフォルダを `PATH` に追加してください。

**ソースから**

```bash
cargo install --path .        # movara コマンドが使えます
```

UI 言語はシステムロケールに自動追従します（English / 简体中文 / 日本語 /
한국어 / Español / Français / Deutsch / Português）。`--lang` または
`MOVARA_LANG` で上書きできます。

## 使い方

```bash
# mv の代わりに使う —— ディレクトリ移動 + 全エージェント履歴の移行
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # dst が既存ディレクトリなら移入
movara mv --dry-run ~/works/abc ~/works/cba

# お好みで日常用のエイリアス（シェルの設定ファイルに追記）：
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# どのエージェントがこのパスを記録しているか確認
movara scan --from ~/works/abc

# 移行のみ（ディレクトリは移動済み）
movara migrate --from ~/works/abc --to ~/works/cba --yes

# 全部取り消す
movara backups
movara undo --id 20260903-131427-644777
```

`movara mv` の動作：旧パスを参照するエージェント状態をスキャンして表示 →
確認 → ディレクトリを `mv`（ファイルシステムをまたぐ場合はコピー+削除へ
自動フォールバック）→ 各エージェントを移行 → undo ID 付きのレポートを
出力。問題が起きても `movara undo` 1 回でディレクトリ位置と全エージェント
状態を復元します。ディレクトリのみを受け付けます（ファイルにはエージェント
履歴がないため通常の `mv` を）。移動先が既に存在・親ディレクトリが無い・
src == dst は拒否します。

### サブコマンド

| コマンド | 用途 |
|---|---|
| `movara mv <SRC> <DST>` | ディレクトリを移動（DST が既存なら中へ）し全エージェントを一括移行 —— 日常の mv 代替 |
| `movara scan --from <OLD> [--to <NEW>]` | 読み取り専用で、パスを参照するエージェント状態を報告。`--to` はリネーム先プレビュー用のみ |
| `movara migrate --from <OLD> --to <NEW>` | ディレクトリを他の手段で移動済みの状態から再キーイング。`--move-project` は先にディレクトリ自体を移動 |
| `movara undo --id <ID>` | 1 回の移行を完全に巻き戻す（下記） |
| `movara backups` | 移行ジャーナルの一覧（下記） |
| `movara agents` | 対応エージェントとインストール状態の一覧 |
| `movara export [--path PATH]... [--agents LIST]` | エージェント状態を可搬な `.tar.gz` に書き出す。ホスト全体、またはパス/エージェントで絞り込み |
| `movara import <ARCHIVE> [--rebase OLD:NEW]...` | アーカイブをこのホストに復元しパスをリベース。移行と同じくジャーナル化され `movara undo` で完全に取り消せます |
| `movara move <SRC> [user@]host:<DST>` | プロジェクトとエージェント状態を ssh 経由で 1 コマンドで別ホストへ移動 — 記憶も同行、クリーンアップは任意（IPv6 ホストリテラルは不可） |
| `movara receive --dst <DST>` | [受け側] stdin のストリームアーカイブを取り込む（`move` が起動） |

### オプション

| オプション | 適用範囲 | 意味 |
|---|---|---|
| `--lang CODE` | グローバル | 表示言語を上書き（en, zh-CN, ja, ko, es, fr, de, pt-BR） |
| `--agents LIST` | scan, migrate, mv | カンマ区切りで対象エージェントを限定（デフォルト：全て） |
| `--extra-root PATH` | scan, migrate, mv | 任意のツリーも書き換え。繰り返し可 |
| `--deep` | migrate, mv | チャット本文・ログ内の旧パスも書き換え（デフォルトは身分フィールドのみ） |
| `--backup-dir DIR` | migrate, mv, undo, backups | バックアップジャーナルのルート（デフォルト `~/.movara/backups`） |
| `--dry-run` | migrate, mv | レポートのみ。何も変更しない |
| `--yes` | migrate, mv | 確認プロンプトをスキップ |
| `--move-project` | migrate | 再キーイングの前にプロジェクトディレクトリ自体を移動 |
| `--json` | agents, scan, migrate, mv, backups | stdout に単一の JSON ドキュメントを出力 |
| `--out FILE` | export | アーカイブパス（デフォルト `movara-export-<タイムスタンプ>.tar.gz`） |
| `--path PATH` | export | このプロジェクトパスを参照する状態のみ（繰り返し可。`--agents` との積） |
| `--rebase OLD:NEW` | import | パスマッピング。繰り返し可。重複・連鎖ルールは拒否されます |
| `--dst <DST>` | receive | このホストでのプロジェクト配置先 |
| `--plan-only` | receive | 宛先を事前チェックして終了 |
| `--yes` | receive | 非対話（ストリーム受信時に必須） |
| `--on-conflict POLICY` | import | 既存ローカル状態への `skip`（デフォルト）または `replace` |
| `--allow-missing-path` | import | アーカイブパスがローカルに存在しなくても続行 |
| `--state-only` | move | コードを除きエージェント状態とプロジェクト記憶のみ運ぶ |
| `--cleanup` | move | 検証成功後に移動元の状態セットを削除（共有 DB/設定は残す） |

### undo とバックアップ

`mv` / `migrate` は何かを変更する前に、`~/.movara/backups/<id>/` に
ジャーナルを書き出します：変更対象ファイルのコピー、SQLite データベース
全体（`wal_checkpoint` 実行後）、移動したプロジェクトディレクトリを含む
リネーム台帳です。

- **`movara backups`** はジャーナルの一覧（ID・日付・旧→新パス・対象
  エージェント・ファイル/DB/リネーム数、`--json` はスクリプト用）を
  表示します。ジャーナルが undo の単位で、パスやエージェントによる
  絞り込みは現在サポートされていません。古いジャーナルは手動で削除して
  ディスクを解放できます。
- **`movara undo --id <ID>`** はジャーナルを逆再生します —— ファイル内容
  を復元し、DB を戻し、リネームを巻き戻し、移動済みディレクトリを元の
  場所へ戻します。移行先を間違えた、移行中にエージェントが動いていた、
  旧レイアウトに戻したい、といった場面で使います。undo は移行 1 回分の
  全か無かです。1 つの ID でその移行全体を巻き戻し、特定エージェントや
  ファイルだけを戻すことはできません。同じパスを再移行する前に実行して
  ください。

`export` / `import` も同様にジャーナル化されます。`undo` は作成された状態も含めてインポートを完全に巻き戻します。
クロスホスト `move` はプロジェクトツリー（`.git` を含む）も交換します。`--state-only` はコードを除外しつつプロジェクト内記憶ファイル（CLAUDE.md、AGENTS.md、rules）は保持し、`--cleanup` は移動済み状態セットのみを削除（共有 DB や設定は決して削除しない）し、それ自体取り消し可能です。

## 安全性

- **境界を考慮した置換**：`/a/abc` は `/a/abc2` や `/a/abc-def` には
  一致しません。`file://` URI、JSON エスケープ、サブパスも正しく扱います。
- **派生トークンも置換**：sha256 / sha256[:16] / md5 / 各ベンダーの
  ダッシュエンコードディレクトリ名。
- 移行のたびに完全バックアップ（`wal_checkpoint` 後に SQLite を丸ごと
  コピー、リネーム記録、`undo` が全て復元）。
- 移動先が存在する場合はスキップ。`--from /` は拒否。
- 移行対象のエージェントは事前に終了してください（WAL データベースは
  警告しますが破損はしません）。


## 対応エージェント

| エージェント | 状態の保存場所 | パスキー |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | ダッシュ化ディレクトリ + `projects` キー + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | ダッシュ化ディレクトリ + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | 独自エンコード + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | session/project/workspace の directory カラム |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | ホーム相対ダッシュバケット + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor（IDE+CLI） | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/file:// URI + composerData |
| Windsurf | `~/.codeium/windsurf/` + IDE state.vscdb | md5(path) + file:// URI |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | VS Code fork と同一 |
| Crush | `<プロジェクト>/.crush/crush.db` + グローバル projects.json | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath（スラッシュのみ変換） |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | file:// URI + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | `--encoded--` バケット + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | 設定内の絶対パス |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | ディレクトリ MRU + ファイル名ハッシュ |
| Kimi Code | `~/.kimi-code/` workspaces.json、session_index.jsonl、sessions/、file-history/、workspace-trust/ | `wd_<basename>_<sha256[:12]>` バケット（ディレクトリ+ファイル）+ workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + レガシー `*.jsonl`, `~/.config/goose/` | `sessions.working_dir` + `working_dir` メタデータ + 権限キー |

非対応（意図的な判断）：

- **GitHub Copilot CLI** — ローカルスキーマは非公開、クラウドが情報源。
- **Amp** — スレッドはサーバー側に存在。
- **claude-code-router** — パスでキー付けされた状態なし（ソースで確認済み）。

## コントリビューション

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # 32 件のテストが全て通ること
cargo clippy --all-targets -- -D warnings # 警告ゼロ
cargo fmt --all -- --check
pre-commit install                        # ローカルフック（CI と同一）
```

詳細は [CONTRIBUTING.md](CONTRIBUTING.md)、[CHANGELOG.md](CHANGELOG.md)、
> **クロスホスト同期 (sync) は次期リリース向けに開発中です** —
> [CHANGELOG.md](CHANGELOG.md) の Unreleased セクションを参照してください。

> **クロスホスト同期 (sync) は次期リリース向けに開発中です** —
> [CHANGELOG.md](CHANGELOG.md) の Unreleased セクションを参照してください。

[SECURITY.md](SECURITY.md)、[docs/research.md](docs/research.md) を参照。

## ライセンス

Copyright (C) 2026 Movara contributors.

本プロジェクトは Apache License 2.0 または MIT license のデュアルライセンスです。どちらかを選択してください。詳細は [LICENSE-APACHE](LICENSE-APACHE) と [LICENSE-MIT](LICENSE-MIT) を参照してください。
