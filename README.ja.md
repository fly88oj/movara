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

Rust 実装。実行時依存なし、Linux / macOS / Windows に対応：

```bash
# タグ付き Release ごとに GitHub Actions がビルド成果物を公開します：
#   tar.gz / zip（exe を含む）と .deb / .rpm / .dmg ネイティブパッケージ
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # Movara コマンドが使えます
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

`movara` の動作：旧パスを参照するエージェント状態をスキャンして表示 →
確認 → ディレクトリを `mv`（ファイルシステムをまたぐ場合はコピー+削除へ
自動フォールバック）→ 各エージェントを移行 → undo ID 付きのレポートを
出力。問題が起きても `movara undo` 1 回でディレクトリ位置と全エージェント
状態を復元します。ディレクトリのみを受け付けます（ファイルにはエージェント
履歴がないため通常の `mv` を）。移動先が既に存在・親ディレクトリが無い・
src == dst は拒否します。

| オプション | 意味 |
|---|---|
| `--agents claude,codex` | 対象エージェントを限定（デフォルト：全て） |
| `--extra-root PATH` | 任意のツリーも書き換え。繰り返し可 |
| `--deep` | チャット本文内の旧パスも書き換え（デフォルトは身分フィールドのみ） |
| `--backup-dir DIR` | バックアップディレクトリ（デフォルト `~/.movara/backups`） |
| `--dry-run` | レポートのみ |
| `--yes` | 確認プロンプトをスキップ（mv・migrate） |
| `--json` | stdout に単一の JSON ドキュメントを出力 |

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
[SECURITY.md](SECURITY.md)、[docs/research.md](docs/research.md) を参照。

## ライセンス

Copyright (C) 2026 movara contributors.

このプログラムはフリーソフトウェアです。あなたはこれを、フリーソフトウェア
財団が発行する GNU 一般公衆利用許諾書（バージョン 3 か、希望によっては
それ以降のバージョン）の定めの下で再頒布または改変することができます。
詳細は [LICENSE](LICENSE) を参照してください。
