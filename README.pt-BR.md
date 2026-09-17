# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | **[Português](README.pt-BR.md)**

Estado de workspace portátil para agentes de programação com IA.

```
~/abc  ──renomeado──>  ~/cba
  └─ os registros de /home/me/abc em cada agente  ──movara──>  /home/me/cba
```

## Por quê

A maioria dos agentes de programação com IA (Claude Code, Codex, a família Gemini CLI, OpenCode, omp, Cursor, Windsurf, …) indexa o histórico de sessões pelo **caminho do projeto**: os nomes de diretório são alguma codificação do caminho (hífens / sha256 / md5) e os valores de `cwd` vivem dentro de arquivos JSON, JSONL, SQLite e protobuf. Mova ou renomeie o diretório, e as sessões antigas "desaparecem" — elas continuam no disco, apenas indexadas a um caminho que não existe mais. O `movara` move o diretório e reindexa todas essas referências para o novo caminho de uma só vez, com desfazer completo.

## Instalação

Implementado em Rust, sem dependências de runtime, roda em Linux / macOS / Windows:

```bash
# binários pré-compilados (tar.gz / zip) e pacotes nativos (.deb / .rpm / .dmg)
# são anexados a cada release com tag pelo GitHub Actions
#   https://github.com/fly88oj/movara/releases

cargo install --path .        # fornece o comando `movara`
```

O idioma da interface segue automaticamente o locale do sistema (English, 简体中文, 日本語, 한국어, Español, Français, Deutsch, Português); sobrescreva com `--lang` ou `MOVARA_LANG`.

## Uso

```bash
# o caso do dia a dia: use no lugar do mv — move o diretório E
# migra todo o histórico dos agentes em um único comando
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # dst é um diretório existente: move para dentro (semântica do mv)
movara mv --dry-run ~/works/abc ~/works/cba

# atalho opcional para o dia a dia (adicione ao rc do seu shell):
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# ver quais agentes referenciam um caminho
movara scan --from ~/works/abc

# somente migrar (diretório já movido)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# desfazer tudo
movara backups
movara undo --id 20260903-131427-644777
```

Comportamento do `movara`: escanear e mostrar qual estado de agente referencia o caminho antigo → confirmar → `mv` do diretório (entre sistemas de arquivos diferentes, recorre a copiar + remover) → migrar cada agente → imprimir um relatório com o id de desfazer. Se algo der errado, um único `movara undo` restaura tanto a localização do diretório quanto todo o estado dos agentes. Somente diretórios são aceitos (arquivos não carregam histórico de agente — use o `mv` comum); alvos já existentes, diretórios-pai do alvo ausentes e origem == destino são recusados.

| opção | significado |
|---|---|
| `--agents claude,codex` | limitar a agentes específicos (padrão: todos os instalados) |
| `--extra-root PATH` | também reescrever uma árvore arbitrária (dotfiles, configs de IDE); repetível |
| `--deep` | também reescrever menções de caminho dentro de conteúdo de chat / logs (padrão: somente campos de identidade — cwd, directory, project, …) |
| `--backup-dir DIR` | diretório de backup (padrão `~/.movara/backups`) |
| `--dry-run` | somente relatório |
| `--yes` | pular o aviso de confirmação (mv, migrate) |
| `--json` | emitir um único documento JSON no stdout (legível por máquina) |

## Segurança

- **Substituição ciente de limites**: `/a/abc` nunca corresponde a `/a/abc2` ou `/a/abc-def`; URIs `file://`, escapes de JSON e subcaminhos (`/a/abc/sub`) são todos tratados.
- **Tokens derivados também são substituídos**: sha256 completo (`projectHash` do gemini, diretórios tmp do qwen/iflow), sha256[:16] (chaves de memória do zcode), md5 (diretórios context_state/database do windsurf) e o nome de diretório codificado com hífens de cada fornecedor.
- Backup completo antes de cada migração: arquivos alterados e bancos SQLite são copiados (após um `wal_checkpoint`), renomeações são registradas em journal, e o `undo` restaura tudo; movimentos de diretório feitos pelo `movara` também são desfeitos.
- Renomeações são puladas quando o alvo já existe; `--from /` é recusado.
- Feche os agentes que serão migrados (bancos WAL recebem um aviso, mas não são corrompidos).

## Agentes suportados

| agente | local do estado | chave de caminho |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | diretório com hífens + chaves `projects` + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | diretório com hífens + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | codificação própria + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | colunas directory de session/project/workspace |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | bucket de hífens relativo ao home + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/URIs file:// + composerData |
| Windsurf | `~/.codeium/windsurf/` + state.vscdb da IDE | md5(path) + URIs file:// |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | igual aos forks do VS Code |
| Crush | `<projeto>/.crush/crush.db` + projects.json global | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, apenas barras |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | URI file:// + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | bucket `--encoded--` + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | caminhos absolutos na configuração |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | MRU de diretórios + hash no nome do arquivo |

Não suportado (por decisão de projeto):

- **GitHub Copilot CLI** — esquema local não publicado, a nuvem é autoritativa.
- **Amp** — as threads vivem no servidor.
- **claude-code-router** — sem estado indexado por caminho (verificado no código-fonte).

## Contribuindo

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # os 32 testes devem passar
cargo clippy --all-targets -- -D warnings # nenhum aviso
cargo fmt --all -- --check
pre-commit install                        # hooks locais (idênticos ao CI)
```

Veja [CONTRIBUTING.md](CONTRIBUTING.md) para o guia de adaptadores, convenções de commit e configuração; [CHANGELOG.md](CHANGELOG.md) para o histórico de versões; [SECURITY.md](SECURITY.md) para reportar problemas de segurança; [docs/research.md](docs/research.md) para formatos de armazenamento, codificações e fontes por agente.

## Licença

Copyright (C) 2026 Movara contributors.

Licenciado sob Apache License 2.0 ou MIT, à sua escolha. Veja
[LICENSE-APACHE](LICENSE-APACHE) e [LICENSE-MIT](LICENSE-MIT).
