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
disponible en Linux / macOS / Windows. Cada
[release](https://github.com/fly88oj/movara/releases) incluye paquetes
precompilados; los nombres de archivo siguientes usan `1.2.0`, sustitúyelo
por la versión que descargues.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**Otras distribuciones Linux (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg o tar.gz)**

```bash
# abre el dmg y copia bin/movara a /usr/local/bin, o:
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Los Mac Intel ejecutan la compilación arm64 bajo Rosetta 2 o compilan desde
el código fuente.

**Windows (zip)**

Extrae `movara-1.2.0-x86_64-pc-windows-msvc.zip` y pon `movara.exe` en tu
`PATH`.

**Desde el código fuente**

```bash
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

Comportamiento de `movara mv`: escanear y mostrar qué estado de agente
referencia la ruta antigua → confirmar → `mv` del directorio (con reserva
automática de copiar + eliminar si los sistemas de archivos difieren) →
migrar cada agente → imprimir un informe con el id de deshacer. Si algo
falla, un solo `movara undo` restaura tanto la ubicación del
directorio como todo el estado de los agentes. Solo acepta directorios
(los archivos no llevan historial de agente: use el `mv` normal); se
rechazan destinos existentes, directorios padre inexistentes y
origen == destino.

### Comandos

| comando | propósito |
|---|---|
| `movara mv <SRC> <DST>` | mueve el directorio (DST existente = mover dentro) y migra todos los agentes en un paso — el reemplazo cotidiano de mv |
| `movara scan --from <OLD> [--to <NEW>]` | informe de solo lectura de qué estado de agente referencia una ruta; `--to` solo añade vistas previas del destino |
| `movara migrate --from <OLD> --to <NEW>` | recodifica el estado tras mover el directorio por otros medios; `--move-project` mueve primero el directorio |
| `movara undo --id <ID>` | revierte por completo una migración (ver abajo) |
| `movara backups` | lista los diarios de migración (ver abajo) |
| `movara agents` | lista los agentes admitidos y su estado de instalación |
| `movara export [--path RUTA]... [--agents LISTA]` | escribe un archivo `.tar.gz` portátil del estado — todo el host, o filtrado por ruta de proyecto / agentes |
| `movara import <ARCHIVO> [--rebase OLD:NEW]...` | restaura un archivo en este host remapeando rutas; se registra como una migración, así que `movara undo` lo revierte |
| `movara move <SRC> [usuario@]host:<DST>` | mueve un proyecto Y su estado de agentes a otro host por ssh en un comando — la memoria viaja, limpieza opcional (sin literales IPv6) |
| `movara receive --dst <DST>` | [lado destino] importa el archivo en flujo desde stdin (lo lanza `move`) |

### Opciones

| opción | aplica a | significado |
|---|---|---|
| `--lang CODE` | global | sobrescribe el idioma de la interfaz (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LISTA` | scan, migrate, mv | lista separada por comas; limitar a agentes concretos (por defecto: todos) |
| `--extra-root RUTA` | scan, migrate, mv | reescribir también un árbol arbitrario; repetible |
| `--deep` | migrate, mv | reescribir también menciones dentro del contenido del chat / registros (por defecto: solo campos de identidad) |
| `--backup-dir DIR` | migrate, mv, undo, backups | raíz de los diarios de copia (por defecto `~/.movara/backups`) |
| `--dry-run` | migrate, mv | solo informe, sin cambios |
| `--yes` | migrate, mv | omitir el aviso de confirmación |
| `--move-project` | migrate | mover primero el directorio del proyecto |
| `--json` | agents, scan, migrate, mv, backups | emitir un único documento JSON en stdout |
| `--out FILE` | export | ruta del archivo (por defecto `movara-export-<marca-de-tiempo>.tar.gz`) |
| `--path RUTA` | export | solo el estado que referencia esta ruta de proyecto (repetible; interseca con `--agents`) |
| `--rebase OLD:NEW` | import | mapeo de rutas, repetible; las reglas solapadas o encadenadas se rechazan |
| `--dst <DST>` | receive | directorio de destino del proyecto en este host |
| `--plan-only` | receive | comprobar el destino y salir |
| `--yes` | receive | no interactivo (obligatorio en streaming) |
| `--on-conflict POLICY` | import | `skip` (por defecto) o `replace` el estado local existente |
| `--allow-missing-path` | import | continuar cuando las rutas del archivo no existan localmente |
| `--state-only` | move | llevar solo estado + memoria del proyecto, no el código |
| `--cleanup` | move | eliminar el conjunto migrado en este host tras el éxito verificado (las bases/configs compartidas se quedan) |

### Deshacer y copias de seguridad

Cada `mv` / `migrate` escribe un diario en `~/.movara/backups/<id>/` antes
de cambiar nada: copias de cada archivo que va a modificarse, bases SQLite
completas (tras un `wal_checkpoint`) y un registro de renombrados que
incluye el directorio movido.

- **`movara backups`** lista los diarios — id, fecha, ruta antigua → nueva,
  agentes afectados, recuentos de archivos/bases/renombrados (`--json` para
  scripts). El diario es la unidad de deshacer: filtrar por ruta o agente no
  está soportado hoy; borra a mano los diarios antiguos para liberar disco.
- **`movara undo --id <ID>`** reproduce un diario hacia atrás — los archivos
  vuelven a su contenido, las bases se intercambian, los renombrados se
  revierten y el directorio movido regresa a su sitio. Úsalo cuando una
  migración apuntó a la ruta equivocada, un agente seguía ejecutándose
  durante el movimiento o simplemente quieres el diseño antiguo. Deshacer es
  todo-o-nada por migración: un id revierte esa migración completa, no un
  solo agente o archivo, y debe ejecutarse antes de una nueva migración de
  las mismas rutas.

Los intercambios con `export` / `import` se registran igual: `undo` revierte una importación por completo, incluido el estado que creó.
Un `move` entre hosts añade el árbol del proyecto (con `.git`) al intercambio: `--state-only` descarta el código pero conserva los archivos de memoria del proyecto (CLAUDE.md, AGENTS.md, rules); `--cleanup` borra solo el conjunto migrado en el origen — nunca bases de datos ni configs compartidas — y es reversible.

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
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | buckets `wd_<basename>_<sha256[:12]>` (dirs+archivos) + workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + `*.jsonl` heredados, `~/.config/goose/` | `sessions.working_dir` + metadatos `working_dir` + claves de permisos |
| Cline / Roo Code / Kilo Code | `~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code}` | campos `path` de tareas + `workspace`/`cwdOnTaskInitialization` + `core.worktree` de checkpoints + buckets cwdHash/sha256 |

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
> **La sincronización entre hosts está en desarrollo** para el próximo
> lanzamiento; véase la sección Unreleased de [CHANGELOG.md](CHANGELOG.md).

[SECURITY.md](SECURITY.md) y [docs/research.md](docs/research.md).

## Licencia

Copyright (C) 2026 Movara contributors.

Con licencia dual Apache License 2.0 o MIT, a tu elección. Consulta
[LICENSE-APACHE](LICENSE-APACHE) y [LICENSE-MIT](LICENSE-MIT).
