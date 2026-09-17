# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | **[Español](README.es.md)** | [Français](README.fr.md) | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

Estado de workspace portátil para agentes de código con IA.

```
~/abc  ──renombrado──>  ~/cba
  └─ los registros de /home/me/abc de cada agente  ──movara──>  /home/me/cba
```

## Motivación

La mayoría de los agentes de código con IA (Claude Code, Codex, la familia
Gemini CLI, OpenCode, omp, Cursor, Windsurf, …) asocian su historial de
sesiones a la **ruta del proyecto**: los nombres de directorio son alguna
codificación de la ruta (guiones / sha256 / md5) y los valores de `cwd`
viven dentro de JSON, JSONL, SQLite y protobuf. Si mueves o renombras el
directorio, las sesiones antiguas «desaparecen» — siguen en el disco, pero
apuntan a una ruta que ya no existe. `movara` mueve el directorio y
reasigna todas esas referencias a la nueva ruta de una vez, con deshacer
completo.

## Instalación

Implementación en Rust, sin dependencias en tiempo de ejecución,
disponible en Linux / macOS / Windows:

```bash
# cada Release etiquetada publica artefactos construidos por GitHub Actions:
#   tar.gz / zip (incluye exe) y paquetes nativos .deb / .rpm / .dmg
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # instala el comando `movara`
```

El idioma de la interfaz sigue automáticamente la configuración regional del
sistema (English / 简体中文 / 日本語 / 한국어 / Español / Français / Deutsch /
Português); puede sobrescribirse con `--lang` o `MOVARA_LANG`.

## Uso

```bash
# úsalo en lugar de mv — mueve el directorio Y migra todo el historial
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # destino existente: se mueve dentro
movara mv --dry-run ~/works/abc ~/works/cba

# alias opcional para el día a día (añádelo al rc de tu shell):
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# ver qué agentes registran una ruta
movara scan --from ~/works/abc

# migrar solo (el directorio ya se movió)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# deshacer todo
movara backups
movara undo --id 20260903-131427-644777
```

Comportamiento de `movara`: escanear y mostrar qué estado de agente
referencia la ruta antigua → confirmar → `mv` del directorio (con reserva
automática de copiar + eliminar si los sistemas de archivos difieren) →
migrar cada agente → imprimir un informe con el id de deshacer. Si algo
falla, un solo `movara undo` restaura tanto la ubicación del
directorio como todo el estado de los agentes. Solo acepta directorios
(los archivos no llevan historial de agente: use el `mv` normal); se
rechazan destinos existentes, directorios padre inexistentes y
origen == destino.

| opción | significado |
|---|---|
| `--agents claude,codex` | limitar a agentes concretos (por defecto: todos) |
| `--extra-root RUTA` | reescribir también un árbol arbitrario; repetible |
| `--deep` | reescribir también menciones dentro del contenido del chat / registros (por defecto: solo campos de identidad) |
| `--backup-dir DIR` | directorio de copias de seguridad (por defecto `~/.movara/backups`) |
| `--dry-run` | solo informe |
| `--yes` | omitir el aviso de confirmación (mv, migrate) |
| `--json` | emitir un único documento JSON en stdout |

## Seguridad

- **Sustitución consciente de límites**: `/a/abc` nunca coincide con
  `/a/abc2` ni `/a/abc-def`; los URI `file://`, el escape JSON y las
  subrutas se tratan correctamente.
- **También se sustituyen los tokens derivados**: sha256 completo,
  sha256[:16], md5 y el nombre de directorio codificado de cada fabricante.
- Copia de seguridad completa antes de cada migración (tras un
  `wal_checkpoint`); `undo` lo restaura todo.
- Los renombrados se omiten si el destino existe; se rechaza `--from /`.
- Cierre los agentes que vaya a migrar (las bases WAL reciben un aviso
  pero no se corrompen).

## Agentes admitidos

| agente | ubicación del estado | clave de ruta |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | directorio con guiones + claves `projects` + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | directorio con guiones + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | codificación propia + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | columnas directory de session/project/workspace |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | bucket de guiones relativo al home + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/URI file:// + composerData |
| Windsurf | `~/.codeium/windsurf/` + state.vscdb del IDE | md5(path) + URI file:// |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | igual que los forks de VS Code |
| Crush | `<proyecto>/.crush/crush.db` + projects.json global | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, solo barras |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | URI file:// + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | bucket `--encoded--` + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | rutas absolutas en la configuración |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | MRU de directorios + hash en el nombre de archivo |

Sin soporte (por decisión de diseño):

- **GitHub Copilot CLI** — esquema local no publicado; la nube es autoritativa.
- **Amp** — los hilos viven en el servidor.
- **claude-code-router** — sin estado indexado por ruta (verificado en el código fuente).

## Contribuir

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # 32 pruebas deben pasar
cargo clippy --all-targets -- -D warnings # cero advertencias
cargo fmt --all -- --check
pre-commit install                        # ganchos locales (idénticos a CI)
```

Véase [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md),
[SECURITY.md](SECURITY.md) y [docs/research.md](docs/research.md).

## Licencia

Copyright (C) 2026 movara contributors.

Este programa es software libre: puede redistribuirlo y/o modificarlo bajo
los términos de la Licencia Pública General GNU tal como la publica la Free
Software Foundation, ya sea la versión 3 o (a su elección) cualquier versión
posterior. Vea [LICENSE](LICENSE).
