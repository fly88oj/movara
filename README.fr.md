# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | **[Français](README.fr.md)** | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

Un état d'espace de travail portable pour les agents de codage IA.

```
~/abc  ──renommé──>  ~/cba
  └─ les enregistrements de /home/me/abc chez chaque agent  ──movara──>  /home/me/cba
```

## Pourquoi

La plupart des agents de codage IA (Claude Code, Codex, la famille Gemini CLI, OpenCode, omp, Cursor, Windsurf, …) indexent leur historique de sessions par **chemin de projet** : les noms de répertoires sont un encodage du chemin (tirets / sha256 / md5) et les valeurs `cwd` se trouvent dans des fichiers JSON, JSONL, SQLite et protobuf. Déplacez ou renommez le répertoire, et les anciennes sessions « disparaissent » — elles sont toujours sur le disque, simplement rattachées à un chemin qui n'existe plus. `movara` déplace le répertoire et réindexe toutes ces références vers le nouveau chemin en une seule fois, avec annulation complète.

## Installation

Implémenté en Rust, sans dépendance à l'exécution, fonctionne sur Linux / macOS / Windows :

```bash
# les binaires précompilés (tar.gz / zip) et les paquets natifs (.deb / .rpm / .dmg)
# sont attachés à chaque version étiquetée par GitHub Actions
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # fournit la commande `movara`
```

La langue de l'interface suit automatiquement les paramètres régionaux du système (English, 简体中文, 日本語, 한국어, Español, Français, Deutsch, Português) ; surchargez-la avec `--lang` ou `MOVARA_LANG`.

## Utilisation

```bash
# le cas courant : à la place de mv — déplacer le répertoire ET
# migrer tout l'historique des agents en une seule commande
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # dst est un répertoire existant : déplacement à l'intérieur (sémantique mv)
movara mv --dry-run ~/works/abc ~/works/cba

# raccourci optionnel pour le quotidien (à ajouter dans votre rc de shell) :
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# voir quels agents référencent un chemin
movara scan --from ~/works/abc

# migration seule (répertoire déjà déplacé)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# tout annuler
movara backups
movara undo --id 20260903-131427-644777
```

Comportement de `movara` : scanner et montrer quel état d'agent référence l'ancien chemin → confirmation → `mv` du répertoire (recopie + suppression si les systèmes de fichiers diffèrent) → migration de chaque agent → rapport avec l'identifiant d'annulation. En cas de problème, un simple `movara undo` restaure à la fois l'emplacement du répertoire et tout l'état des agents. Seuls les répertoires sont acceptés (les fichiers ne portent pas d'historique d'agent — utilisez `mv`) ; les cibles existantes, les parents de cible manquants et source == cible sont refusés.

| option | signification |
|---|---|
| `--agents claude,codex` | se limiter à certains agents (par défaut : tous ceux installés) |
| `--extra-root PATH` | réécrire aussi une arborescence arbitraire (dotfiles, configs d'IDE) ; répétable |
| `--deep` | réécrire aussi les mentions de chemin dans le contenu des conversations / journaux (par défaut : champs d'identité uniquement — cwd, directory, project, …) |
| `--backup-dir DIR` | répertoire de sauvegarde (par défaut `~/.movara/backups`) |
| `--dry-run` | rapport seul |
| `--yes` | passer l'invite de confirmation (mv, migrate) |
| `--json` | émettre un unique document JSON sur stdout (exploitable par machine) |

## Sécurité

- **Remplacement conscient des frontières** : `/a/abc` ne correspond jamais à `/a/abc2` ni à `/a/abc-def` ; les URI `file://`, l'échappement JSON et les sous-chemins (`/a/abc/sub`) sont gérés.
- **Les jetons dérivés sont aussi remplacés** : sha256 complet (`projectHash` de gemini, répertoires tmp de qwen/iflow), sha256[:16] (clés de mémoire zcode), md5 (répertoires context_state/database de windsurf) et le nom de répertoire encodé en tirets de chaque éditeur.
- Sauvegarde complète avant chaque migration : les fichiers modifiés et les bases SQLite sont copiés (après un `wal_checkpoint`), les renommages sont journalisés, et `undo` restaure tout ; les déplacements de répertoires effectués par `movara` sont également annulés.
- Les renommages sont ignorés quand la cible existe ; `--from /` est refusé.
- Fermez les agents concernés avant la migration (les bases WAL reçoivent un avertissement mais ne sont pas corrompues).

## Agents pris en charge

| agent | emplacement de l'état | clé de chemin |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | répertoire en tirets + clés `projects` + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | répertoire en tirets + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | encodage propre + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | colonnes directory de session/project/workspace |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | bucket en tirets relatif au home + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/URI file:// + composerData |
| Windsurf | `~/.codeium/windsurf/` + state.vscdb de l'IDE | md5(path) + URI file:// |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | comme les forks VS Code |
| Crush | `<projet>/.crush/crush.db` + projects.json global | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, barres obliques seulement |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | URI file:// + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | bucket `--encoded--` + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | chemins absolus dans la configuration |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | MRU de répertoires + hachage du nom de fichier |

Non pris en charge (volontairement) :

- **GitHub Copilot CLI** — schéma local non publié, le cloud fait autorité.
- **Amp** — les threads vivent côté serveur.
- **claude-code-router** — aucun état indexé par chemin (vérifié dans les sources).

## Contribuer

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # les 32 tests doivent passer
cargo clippy --all-targets -- -D warnings # aucun avertissement
cargo fmt --all -- --check
pre-commit install                        # hooks locaux (identiques à la CI)
```

Voir [CONTRIBUTING.md](CONTRIBUTING.md) pour le guide d'adaptateur, les conventions de commit et la mise en place ; [CHANGELOG.md](CHANGELOG.md) pour l'historique des versions ; [SECURITY.md](SECURITY.md) pour signaler un problème de sécurité ; [docs/research.md](docs/research.md) pour les formats de stockage, encodages et sources par agent.

## Licence

Copyright (C) 2026 movara contributors.

Ce programme est un logiciel libre : vous pouvez le redistribuer ou le modifier selon les termes de la Licence Publique Générale GNU publiée par la Free Software Foundation, version 3 de la Licence ou (à votre choix) toute version ultérieure. Voir [LICENSE](LICENSE).
