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

In Rust implementiert, keine Laufzeitabhängigkeiten, läuft auf Linux / macOS / Windows. Jedes [Release](https://github.com/fly88oj/movara/releases) enthält vorgebaute Pakete; die Dateinamen unten verwenden `1.2.0` — ersetzen Sie sie durch die heruntergeladene Version.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**Andere Linux-Distributionen (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg oder tar.gz)**

```bash
# dmg öffnen und bin/movara nach /usr/local/bin kopieren, oder:
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Intel-Macs führen die arm64-Build unter Rosetta 2 aus oder bauen aus dem
Quellcode.

**Windows (zip)**

`movara-1.2.0-x86_64-pc-windows-msvc.zip` entpacken und `movara.exe` in
den `PATH` legen.

**Aus dem Quellcode**

```bash
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

Verhalten von `movara mv`: scannen und anzeigen, welcher Agentenzustand den alten Pfad referenziert → bestätigen → Verzeichnis verschieben (dateisystemübergreifend automatisch per Kopieren+Löschen) → jeden Agenten migrieren → Bericht mit Undo-Kennung ausgeben. Geht etwas schief, stellt ein einziges `movara undo` sowohl die Verzeichnisposition als auch den gesamten Agentenzustand wieder her. Nur Verzeichnisse werden akzeptiert (Dateien tragen keinen Agentenverlauf — normales `mv` verwenden); vorhandene Ziele, fehlende Ziel-Elternverzeichnisse und Quelle == Ziel werden abgelehnt.

### Befehle

| Befehl | Zweck |
|---|---|
| `movara mv <SRC> <DST>` | verschiebt das Verzeichnis (vorhandenes DST = hineinverschieben) und migriert alle Agenten in einem Schritt — der Alltags-mv-Ersatz |
| `movara scan --from <OLD> [--to <NEW>]` | schreibgeschützter Bericht, welcher Agentenzustand einen Pfad referenziert; `--to` dient nur der Umbenennungsvorschau |
| `movara migrate --from <OLD> --to <NEW>` | schlüsselt den Zustand neu, nachdem das Verzeichnis anderswohin verschoben wurde; `--move-project` verschiebt zuerst das Verzeichnis |
| `movara undo --id <ID>` | macht eine Migration vollständig rückgängig (siehe unten) |
| `movara backups` | listet die Migrationsjournale (siehe unten) |
| `movara agents` | listet unterstützte Agenten und deren Installationsstatus |
| `movara export [--path PFAD]... [--agents LISTE]` | schreibt ein portables `.tar.gz`-Archiv — ganzer Host oder nach Projektpfad / Agenten gefiltert |
| `movara import <ARCHIV> [--rebase OLD:NEW]...` | stellt ein Archiv auf diesem Host wieder her und rebaset Pfade; wie eine Migration journaleliert, also mit `movara undo` vollständig rückgängig |
| `movara move <SRC> [benutzer@]host:<ZIEL>` | verschiebt ein Projekt UND seinen Agentenzustand per ssh in einem Befehl auf einen anderen Host — Speicher reist mit, Bereinigung optional (keine IPv6-Literale) |
| `movara receive --dst <ZIEL>` | [Zielseite] importiert das gestreamte Archiv von stdin (von `move` gestartet) |

### Optionen

| Option | gilt für | Bedeutung |
|---|---|---|
| `--lang CODE` | global | Sprache der Oberfläche überschreiben (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LISTE` | scan, migrate, mv | Kommaliste; auf bestimmte Agenten beschränken (Standard: alle installierten) |
| `--extra-root PATH` | scan, migrate, mv | zusätzlich einen beliebigen Baum umschreiben (Dotfiles, IDE-Konfigurationen); wiederholbar |
| `--deep` | migrate, mv | auch Pfadnennungen im Chat-Inhalt / Logs umschreiben (Standard: nur Identitätsfelder — cwd, directory, project, …) |
| `--backup-dir DIR` | migrate, mv, undo, backups | Wurzel der Sicherungsjournale (Standard `~/.movara/backups`) |
| `--dry-run` | migrate, mv | nur Bericht, keine Änderung |
| `--yes` | migrate, mv | Bestätigungsaufforderung überspringen |
| `--move-project` | migrate | zuerst das Projektverzeichnis selbst verschieben |
| `--json` | agents, scan, migrate, mv, backups | ein einzelnes JSON-Dokument auf stdout ausgeben (maschinenlesbar) |
| `--out FILE` | export | Archivpfad (Standard `movara-export-<zeitstempel>.tar.gz`) |
| `--path PFAD` | export | nur Zustand, der diesen Projektpfad referenziert (wiederholbar; Schnitt mit `--agents`) |
| `--rebase OLD:NEW` | import | Pfad-Mapping, wiederholbar; überlappende und verkettete Regeln werden abgelehnt |
| `--dst <DST>` | receive | Zielverzeichnis des Projekts auf diesem Host |
| `--plan-only` | receive | Ziel prüfen und beenden |
| `--yes` | receive | nicht-interaktiv (beim Streaming erforderlich) |
| `--on-conflict RICHTLINIE` | import | `skip` (Standard) oder `replace` vorhandenen lokalen Zustand |
| `--allow-missing-path` | import | fortfahren, wenn Archivpfade lokal nicht existieren |
| `--state-only` | move | nur Zustand + Projektspeicher übertragen, nicht den Code |
| `--cleanup` | move | nach verifiziertem Erfolg den migrierten Satz auf diesem Host entfernen (gemeinsame DBs/Configs bleiben) |

### Undo & Backups

Jedes `mv` / `migrate` schreibt vor jeder Änderung ein Journal nach
`~/.movara/backups/<id>/`: Kopien jeder Datei, die sich ändern wird,
vollständige SQLite-Datenbanken (nach einem `wal_checkpoint`) und ein
Protokoll der Umbenennungen einschließlich des verschobenen
Projektverzeichnisses.

- **`movara backups`** listet die Journale — Kennung, Datum, alter → neuer
  Pfad, betroffene Agenten, Zähler für Dateien/Datenbanken/Umbenennungen
  (`--json` für Skripte). Das Journal ist die Undo-Einheit: Filtern nach
  Pfad oder Agent wird heute nicht unterstützt; alte Journale von Hand
  löschen, um Platz zu schaffen.
- **`movara undo --id <ID>`** spielt ein Journal rückwärts — Dateiinhalte
  kehren zurück, Datenbanken werden zurückgetauscht, Umbenennungen laufen
  rückwärts und das verschobene Verzeichnis geht nach Hause. Nutzen Sie es,
  wenn eine Migration den falschen Pfad traf, während des Verschiebens noch
  ein Agent lief oder Sie einfach das alte Layout zurückwollen. Undo ist
  alles-oder-nichts pro Migration: eine Kennung macht genau diese Migration
  vollständig rückgängig, nicht einen einzelnen Agenten oder eine Datei,
  und sollte vor einer neuen Migration derselben Pfade laufen.

Austausch über `export` / `import` wird genauso journaleliert: `undo` macht eine Importierung vollständig rückgängig, inklusive des von ihr erzeugten Zustands.
Ein `move` über Hosts hinweg bringt den Projektbaum (inkl. `.git`) mit in den Austausch: `--state-only` lässt den Code weg, behält aber die Projekt-Speicherdateien (CLAUDE.md, AGENTS.md, rules); `--cleanup` löscht nur den migrierten Satz auf der Quelle — niemals gemeinsame Datenbanken oder Configs — und ist selbst rückgängig machbar.

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
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | `wd_<basename>_<sha256[:12]>` Buckets (Verz.+Dateien) + workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + Legacy-`*.jsonl`, `~/.config/goose/` | `sessions.working_dir` + `working_dir`-Metadaten + Berechtigungsschlüssel |
| Cline / Roo Code / Kilo Code | `~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code}` | Task-`path`-Felder + `workspace`/`cwdOnTaskInitialization` + Checkpoint-`core.worktree` + cwdHash/sha256-Buckets |
| OpenHands | `~/.openhands/` | `working_dir` + `projects/<sha256(realpath)>/` |
| Codebuff / Freebuff | `~/.config/manicode/projects/<basename>/` | Basename-Bucket + run-state `cwd` |
| gptme | `~/.local/share/gptme/logs/<date>-<name>/` | `config.toml [chat] workspace` (auch Tilde-Form) + `workspace`-Symlink + `files`-Listen |
| Qoder / Lingma (CN) | `~/.config/Qoder` + `~/.qoder` + `~/.lingma/qoder-cn` | IDE state.vscdb + `memories/<Konto>/projects/<dash>/` |
| Trae (ByteDance) | `~/.config/Trae CN` + `~/.trae` | IDE state.vscdb + agents/mcp.json |
| GitHub Copilot CLI | `~/.copilot` | agents/hooks/skills-Definitionen |
| Warp | `~/.local/share/warp/warp.db` | generisches Text-Spalten-Sweep (geschlossenes Schema) |
| Open Interpreter | `~/.openinterpreter/` | Rollout `payload.cwd`, `config.toml [projects]`, `state_*.sqlite threads.cwd` |

Nicht unterstützt (bewusst so entschieden):

- **GitHub Copilot CLI** — lokales Schema unveröffentlicht, die Cloud ist maßgeblich.
- **Amp** — Threads liegen serverseitig.
- **claude-code-router** — kein pfadschlüsselbasierten Zustand (aus dem Quellcode verifiziert).

## Mitwirken

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # die gesamte Suite muss bestehen
cargo clippy --all-targets -- -D warnings # keine Warnungen
cargo fmt --all -- --check
pre-commit install                        # lokale Hooks (wie in der CI)
```

Siehe [CONTRIBUTING.md](CONTRIBUTING.md) für die Adapter-Anleitung,
> **Die Cross-Host-Synchronisierung ist in Entwicklung** für das
> nächste Release — siehe den Abschnitt Unreleased in
> [CHANGELOG.md](CHANGELOG.md).
 Commit-Konventionen und Einrichtung; [CHANGELOG.md](CHANGELOG.md) für die Versionshistorie; [SECURITY.md](SECURITY.md) zum Melden von Sicherheitsproblemen; [docs/research.md](docs/research.md) für Speicherformate, Codierungen und Quellen je Agent.

## Lizenz

Copyright (C) 2026 Movara contributors.

Doppellizenziert unter Apache License 2.0 oder MIT-Lizenz, nach Ihrer
Wahl. Siehe [LICENSE-APACHE](LICENSE-APACHE) und
[LICENSE-MIT](LICENSE-MIT).
