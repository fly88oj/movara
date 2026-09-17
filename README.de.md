# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | **[Deutsch](README.de.md)** | [Português](README.pt-BR.md)

Portabler Workspace-Zustand für KI-Coding-Agenten.

```
~/abc  ──umbenannt──>  ~/cba
  └─ die bei jedem Agenten gespeicherten Verweise auf /home/me/abc  ──movara──>  /home/me/cba
```

## Warum

Die meisten KI-Coding-Agenten (Claude Code, Codex, die Gemini-CLI-Familie, OpenCode, omp, Cursor, Windsurf, …) schlüsseln ihren Sitzungsverlauf nach dem **Projektpfad** ab: Verzeichnisnamen sind eine Codierung des Pfades (Bindestriche / sha256 / md5), und `cwd`-Werte stecken in JSON-, JSONL-, SQLite- und Protobuf-Dateien. Verschiebt oder benennt man das Verzeichnis um, „verschwinden“ die alten Sitzungen — sie liegen noch auf der Platte, nur eben an einen Pfad gebunden, den es nicht mehr gibt. `movara` verschiebt das Verzeichnis und schlüsselt all diese Verweise in einem Rutsch auf den neuen Pfad um — mit vollständiger Rückgängigmachung.

## Installation

In Rust implementiert, keine Laufzeitabhängigkeiten, läuft auf Linux / macOS / Windows:

```bash
# vorkompilierte Binaries (tar.gz / zip) und native Pakete (.deb / .rpm / .dmg)
# werden von GitHub Actions an jedes getaggte Release angehängt
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # stellt den Befehl `movara` bereit
```

Die Sprache der Oberfläche folgt automatisch dem Systemgebietsschema (English, 简体中文, 日本語, 한국어, Español, Français, Deutsch, Português); überschreibbar mit `--lang` oder `MOVARA_LANG`.

## Verwendung

```bash
# der Alltagsfall: statt mv verwenden — Verzeichnis verschieben UND
# alle Agentenverläufe in einem Befehl migrieren
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # Ziel ist ein vorhandenes Verz.: hineinverschieben (mv-Semantik)
movara mv --dry-run ~/works/abc ~/works/cba

# optionale Abkürzung für den Alltag (in die Shell-RC eintragen):
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# sehen, welche Agenten einen Pfad referenzieren
movara scan --from ~/works/abc

# nur migrieren (Verzeichnis bereits verschoben)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# alles rückgängig machen
movara backups
movara undo --id 20260903-131427-644777
```

Verhalten von `movara`: scannen und anzeigen, welcher Agentenzustand den alten Pfad referenziert → bestätigen → Verzeichnis verschieben (dateisystemübergreifend automatisch per Kopieren+Löschen) → jeden Agenten migrieren → Bericht mit Undo-Kennung ausgeben. Geht etwas schief, stellt ein einziges `movara undo` sowohl die Verzeichnisposition als auch den gesamten Agentenzustand wieder her. Nur Verzeichnisse werden akzeptiert (Dateien tragen keinen Agentenverlauf — normales `mv` verwenden); vorhandene Ziele, fehlende Ziel-Elternverzeichnisse und Quelle == Ziel werden abgelehnt.

| Option | Bedeutung |
|---|---|
| `--agents claude,codex` | auf bestimmte Agenten beschränken (Standard: alle installierten) |
| `--extra-root PATH` | zusätzlich einen beliebigen Baum umschreiben (Dotfiles, IDE-Konfigurationen); wiederholbar |
| `--deep` | auch Pfadnennungen im Chat-Inhalt / Logs umschreiben (Standard: nur Identitätsfelder — cwd, directory, project, …) |
| `--backup-dir DIR` | Sicherungsverzeichnis (Standard `~/.movara/backups`) |
| `--dry-run` | nur Bericht |
| `--yes` | Bestätigungsaufforderung überspringen (mv, migrate) |
| `--json` | ein einzelnes JSON-Dokument auf stdout ausgeben (maschinenlesbar) |

## Sicherheit

- **Grenzbewusste Ersetzung**: `/a/abc` matcht niemals `/a/abc2` oder `/a/abc-def`; `file://`-URIs, JSON-Escaping und Unterpfade (`/a/abc/sub`) werden korrekt behandelt.
- **Abgeleitete Token werden mitersetzt**: vollständiges sha256 (gemini `projectHash`, qwen/iflow-tmp-Verzeichnisse), sha256[:16] (zcode-Speicherschlüssel), md5 (windsurf context_state/database-Verzeichnisse) sowie der bindestrich-codierte Verzeichnisname jedes Herstellers.
- Vollständige Sicherung vor jeder Migration: veränderte Dateien und SQLite-Datenbanken werden kopiert (nach einem `wal_checkpoint`), Umbenennungen werden protokolliert, und `undo` stellt alles wieder her; von `movara` verschobene Verzeichnisse werden ebenfalls zurückgeholt.
- Umbenennungen werden übersprungen, wenn das Ziel existiert; `--from /` wird abgelehnt.
- Schließen Sie die zu migrierenden Agenten vorher (WAL-Datenbanken erhalten eine Warnung, werden aber nicht beschädigt).

## Unterstützte Agenten

| Agent | Zustandsort | Pfadschlüssel |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | Bindestrich-Verz. + `projects`-Schlüssel + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | Bindestrich-Verz. + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | eigene Codierung + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | directory-Spalten in session/project/workspace |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | home-relativer Bindestrich-Bucket + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/file://-URIs + composerData |
| Windsurf | `~/.codeium/windsurf/` + IDE state.vscdb | md5(path) + file://-URIs |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | wie die VS-Code-Forks |
| Crush | `<projekt>/.crush/crush.db` + globales projects.json | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, nur Schrägstriche |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | file://-URI + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | `--encoded--`-Bucket + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | absolute Pfade in der Konfiguration |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | Verzeichnis-MRU + Dateinamen-Hash |

Nicht unterstützt (bewusst so entschieden):

- **GitHub Copilot CLI** — lokales Schema unveröffentlicht, die Cloud ist maßgeblich.
- **Amp** — Threads liegen serverseitig.
- **claude-code-router** — kein pfadschlüsselbasierten Zustand (aus dem Quellcode verifiziert).

## Mitwirken

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # alle 32 Tests müssen bestehen
cargo clippy --all-targets -- -D warnings # keine Warnungen
cargo fmt --all -- --check
pre-commit install                        # lokale Hooks (wie in der CI)
```

Siehe [CONTRIBUTING.md](CONTRIBUTING.md) für die Adapter-Anleitung, Commit-Konventionen und Einrichtung; [CHANGELOG.md](CHANGELOG.md) für die Versionshistorie; [SECURITY.md](SECURITY.md) zum Melden von Sicherheitsproblemen; [docs/research.md](docs/research.md) für Speicherformate, Codierungen und Quellen je Agent.

## Lizenz

Copyright (C) 2026 Movara contributors.

Doppellizenziert unter Apache License 2.0 oder MIT-Lizenz, nach Ihrer
Wahl. Siehe [LICENSE-APACHE](LICENSE-APACHE) und
[LICENSE-MIT](LICENSE-MIT).
