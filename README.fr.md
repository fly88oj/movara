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

Implémenté en Rust, sans dépendance à l'exécution, fonctionne sur Linux / macOS / Windows. Chaque [release](https://github.com/fly88oj/movara/releases) fournit des paquets précompilés ; les noms de fichiers ci-dessous utilisent `1.2.0`, remplacez-le par la version téléchargée.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**Autres distributions Linux (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg ou tar.gz)**

```bash
# ouvrez le dmg et copiez bin/movara dans /usr/local/bin, ou :
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Les Mac Intel exécutent la build arm64 via Rosetta 2 ou compilent depuis
les sources.

**Windows (zip)**

Extrayez `movara-1.2.0-x86_64-pc-windows-msvc.zip` et placez `movara.exe`
dans votre `PATH`.

**Depuis les sources**

```bash
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

Comportement de `movara mv` : scanner et montrer quel état d'agent référence l'ancien chemin → confirmation → `mv` du répertoire (recopie + suppression si les systèmes de fichiers diffèrent) → migration de chaque agent → rapport avec l'identifiant d'annulation. En cas de problème, un simple `movara undo` restaure à la fois l'emplacement du répertoire et tout l'état des agents. Seuls les répertoires sont acceptés (les fichiers ne portent pas d'historique d'agent — utilisez `mv`) ; les cibles existantes, les parents de cible manquants et source == cible sont refusés.

### Commandes

| commande | rôle |
|---|---|
| `movara mv <SRC> <DST>` | déplace le répertoire (DST existant = déplacement à l'intérieur) et migre tous les agents en une étape — le remplaçant quotidien de mv |
| `movara scan --from <OLD> [--to <NEW>]` | rapport en lecture seule des états d'agent référençant un chemin ; `--to` sert uniquement aux aperçus de renommage |
| `movara migrate --from <OLD> --to <NEW>` | recode l'état après un déplacement fait par d'autres moyens ; `--move-project` déplace d'abord le répertoire |
| `movara undo --id <ID>` | annule entièrement une migration (voir ci-dessous) |
| `movara backups` | liste les journaux de migration (voir ci-dessous) |
| `movara agents` | liste les agents pris en charge et leur état d'installation |
| `movara export [--path CHEMIN]... [--agents LISTE]` | écrit une archive `.tar.gz` portable de l'état — tout l'hôte, ou filtrée par chemin de projet / agents |
| `movara import <ARCHIVE> [--rebase OLD:NEW]...` | restaure une archive sur cet hôte en rebasant les chemins ; journalisée comme une migration, donc `movara undo` l'annule |
| `movara move <SRC> [utilisateur@]hôte:<DST>` | déplace un projet ET l'état de ses agents vers un autre hôte via ssh en une commande — la mémoire voyage, nettoyage optionnel (pas de littéraux IPv6) |
| `movara receive --dst <DST>` | [côté cible] importe l'archive en flux depuis stdin (lancé par `move`) |

### Options

| option | s'applique à | signification |
|---|---|---|
| `--lang CODE` | global | surcharge la langue de l'interface (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LISTE` | scan, migrate, mv | liste à virgules ; se limiter à certains agents (par défaut : tous ceux installés) |
| `--extra-root PATH` | scan, migrate, mv | réécrire aussi une arborescence arbitraire (dotfiles, configs d'IDE) ; répétable |
| `--deep` | migrate, mv | réécrire aussi les mentions de chemin dans le contenu des conversations / journaux (par défaut : champs d'identité uniquement — cwd, directory, project, …) |
| `--backup-dir DIR` | migrate, mv, undo, backups | racine des journaux de sauvegarde (par défaut `~/.movara/backups`) |
| `--dry-run` | migrate, mv | rapport seul, aucun changement |
| `--yes` | migrate, mv | passer l'invite de confirmation |
| `--move-project` | migrate | déplacer d'abord le répertoire du projet |
| `--json` | agents, scan, migrate, mv, backups | émettre un unique document JSON sur stdout (exploitable par machine) |
| `--out FILE` | export | chemin de l'archive (par défaut `movara-export-<horodatage>.tar.gz`) |
| `--path CHEMIN` | export | uniquement l'état référençant ce chemin de projet (répétable ; intersection avec `--agents`) |
| `--rebase OLD:NEW` | import | mapping de chemins, répétable ; les règles qui se chevauchent ou s'enchaînent sont refusées |
| `--dst <DST>` | receive | répertoire de destination du projet sur cet hôte |
| `--plan-only` | receive | vérifier la destination puis sortir |
| `--yes` | receive | non interactif (requis en flux) |
| `--on-conflict POLICY` | import | `skip` (défaut) ou `replace` l'état local existant |
| `--allow-missing-path` | import | continuer quand les chemins de l'archive n'existent pas localement |
| `--state-only` | move | emporter uniquement l'état + la mémoire du projet, pas le code |
| `--cleanup` | move | supprimer l'ensemble migré sur cet hôte après succès vérifié (les bases/configs partagées restent) |

### Annulation et sauvegardes

Chaque `mv` / `migrate` écrit un journal dans `~/.movara/backups/<id>/`
avant toute modification : copies de chaque fichier sur le point de changer,
bases SQLite complètes (après un `wal_checkpoint`) et un registre des
renommages incluant le répertoire déplacé.

- **`movara backups`** liste les journaux — id, date, ancien → nouveau
  chemin, agents touchés, compteurs fichiers/bases/renommages (`--json`
  pour les scripts). Le journal est l'unité d'annulation : aucun filtre par
  chemin ou par agent aujourd'hui ; supprimez à la main les anciens
  journaux pour libérer de l'espace.
- **`movara undo --id <ID>`** rejoue un journal à l'envers — les contenus de
  fichiers reviennent, les bases sont échangées, les renommages s'inversent
  et le répertoire déplacé rentre chez lui. À utiliser quand une migration
  visait le mauvais chemin, qu'un agent tournait encore pendant le
  déplacement, ou simplement pour retrouver l'ancienne disposition.
  L'annulation est tout-ou-rien par migration : un id revertit toute la
  migration, pas un seul agent ou fichier, et doit précéder toute nouvelle
  migration des mêmes chemins.

Les échanges via `export` / `import` sont journalisés de la même façon : `undo` annule totalement une importation, y compris l'état qu'elle a créé.
Un `move` inter-hôtes ajoute l'arborescence du projet (avec `.git`) à l'échange : `--state-only` laisse le code mais conserve les fichiers de mémoire du projet (CLAUDE.md, AGENTS.md, rules) ; `--cleanup` ne supprime que l'ensemble migré côté source — jamais les bases ou configs partagées — et reste réversible.

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
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | buckets `wd_<basename>_<sha256[:12]>` (dirs+fichiers) + workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + `*.jsonl` hérités, `~/.config/goose/` | `sessions.working_dir` + métadonnées `working_dir` + clés de permissions |
| Cline / Roo Code / Kilo Code | `~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code}` | champs `path` des tâches + `workspace`/`cwdOnTaskInitialization` + `core.worktree` des checkpoints + buckets cwdHash/sha256 |
| OpenHands | `~/.openhands/` | `working_dir` + `projects/<sha256(realpath)>/` |
| Codebuff / Freebuff | `~/.config/manicode/projects/<basename>/` | bucket par basename + `cwd` de run-state |
| gptme | `~/.local/share/gptme/logs/<date>-<name>/` | `config.toml [chat] workspace` (forme tilde incluse) + lien symbolique `workspace` + listes `files` |
| Qoder / Lingma (CN) | `~/.config/Qoder` + `~/.qoder` + `~/.lingma/qoder-cn` | state.vscdb de l'IDE + `memories/<compte>/projects/<dash>/` |
| Trae (ByteDance) | `~/.config/Trae CN` + `~/.trae` | state.vscdb de l'IDE + agents/mcp.json |
| GitHub Copilot CLI | `~/.copilot` | définitions agents/hooks/skills |
| Warp | `~/.local/share/warp/warp.db` | balayage générique des colonnes texte (schéma fermé) |
| Open Interpreter | `~/.openinterpreter/` | rollout `payload.cwd`, `config.toml [projects]`, `state_*.sqlite threads.cwd` |

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

Voir [CONTRIBUTING.md](CONTRIBUTING.md) pour le guide d'adaptateur,
> **La synchronisation inter-hôtes est en cours de développement**
> pour la prochaine version — voir la section Unreleased de
> [CHANGELOG.md](CHANGELOG.md).
 les conventions de commit et la mise en place ; [CHANGELOG.md](CHANGELOG.md) pour l'historique des versions ; [SECURITY.md](SECURITY.md) pour signaler un problème de sécurité ; [docs/research.md](docs/research.md) pour les formats de stockage, encodages et sources par agent.

## Licence

Copyright (C) 2026 Movara contributors.

Sous double licence Apache License 2.0 ou MIT, au choix. Voir
[LICENSE-APACHE](LICENSE-APACHE) et [LICENSE-MIT](LICENSE-MIT).
