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

Implementado em Rust, sem dependências de runtime, roda em Linux / macOS / Windows. Cada [release](https://github.com/fly88oj/movara/releases) traz pacotes pré-compilados; os nomes de arquivo abaixo usam `1.2.0` — substitua pela versão que você baixou.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**Outras distribuições Linux (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg ou tar.gz)**

```bash
# abra o dmg e copie bin/movara para /usr/local/bin, ou:
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Macs Intel executam a build arm64 via Rosetta 2 ou compilam do código-fonte.

**Windows (zip)**

Extraia `movara-1.2.0-x86_64-pc-windows-msvc.zip` e coloque `movara.exe`
no seu `PATH`.

**Do código-fonte**

```bash
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

Comportamento do `movara mv`: escanear e mostrar qual estado de agente referencia o caminho antigo → confirmar → `mv` do diretório (entre sistemas de arquivos diferentes, recorre a copiar + remover) → migrar cada agente → imprimir um relatório com o id de desfazer. Se algo der errado, um único `movara undo` restaura tanto a localização do diretório quanto todo o estado dos agentes. Somente diretórios são aceitos (arquivos não carregam histórico de agente — use o `mv` comum); alvos já existentes, diretórios-pai do alvo ausentes e origem == destino são recusados.

### Comandos

| comando | propósito |
|---|---|
| `movara mv <SRC> <DST>` | move o diretório (DST existente = move para dentro) e migra todos os agentes em uma etapa — o substituto cotidiano do mv |
| `movara scan --from <OLD> [--to <NEW>]` | relatório somente-leitura de qual estado de agente referencia um caminho; `--to` serve apenas para pré-visualizar o novo nome |
| `movara migrate --from <OLD> --to <NEW>` | rechaveia o estado depois que o diretório foi movido por outros meios; `--move-project` move o diretório primeiro |
| `movara undo --id <ID>` | reverte uma migração por completo (veja abaixo) |
| `movara backups` | lista os diários de migração (veja abaixo) |
| `movara agents` | lista os agentes suportados e o status de instalação |
| `movara export [--path CAMINHO]... [--agents LISTA]` | grava um arquivo `.tar.gz` portátil do estado — o host inteiro, ou filtrado por caminho de projeto / agentes |
| `movara import <ARQUIVO> [--rebase OLD:NEW]...` | restaura um arquivo neste host remapeando caminhos; registrado como uma migração, então `movara undo` o reverte |
| `movara move <SRC> [usuário@]host:<DST>` | move um projeto E o estado dos agentes para outro host via ssh em um comando — a memória viaja, limpeza opcional (sem literais IPv6) |
| `movara receive --dst <DST>` | [lado destino] importa o arquivo em fluxo do stdin (disparado por `move`) |

### Opções

| opção | aplica-se a | significado |
|---|---|---|
| `--lang CODE` | global | sobrescreve o idioma da interface (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LISTA` | scan, migrate, mv | lista separada por vírgulas; limitar a agentes específicos (padrão: todos os instalados) |
| `--extra-root PATH` | scan, migrate, mv | também reescrever uma árvore arbitrária (dotfiles, configs de IDE); repetível |
| `--deep` | migrate, mv | também reescrever menções de caminho dentro de conteúdo de chat / logs (padrão: somente campos de identidade — cwd, directory, project, …) |
| `--backup-dir DIR` | migrate, mv, undo, backups | raiz dos diários de backup (padrão `~/.movara/backups`) |
| `--dry-run` | migrate, mv | somente relatório, nada é alterado |
| `--yes` | migrate, mv | pular o aviso de confirmação |
| `--move-project` | migrate | mover primeiro o próprio diretório do projeto |
| `--json` | agents, scan, migrate, mv, backups | emitir um único documento JSON no stdout (legível por máquina) |
| `--out FILE` | export | caminho do arquivo (padrão `movara-export-<timestamp>.tar.gz`) |
| `--path CAMINHO` | export | apenas o estado que referencia este caminho de projeto (repetível; interseção com `--agents`) |
| `--rebase OLD:NEW` | import | mapeamento de caminhos, repetível; regras sobrepostas ou encadeadas são recusadas |
| `--dst <DST>` | receive | diretório de destino do projeto neste host |
| `--plan-only` | receive | verificar o destino e sair |
| `--yes` | receive | não interativo (obrigatório em streaming) |
| `--on-conflict POLICY` | import | `skip` (padrão) ou `replace` o estado local existente |
| `--allow-missing-path` | import | prosseguir quando caminhos do arquivo não existirem localmente |
| `--state-only` | move | levar apenas estado + memória do projeto, não o código |
| `--cleanup` | move | remover o conjunto migrado neste host após sucesso verificado (bancos/configs compartilhados ficam) |

### Desfazer e backups

Cada `mv` / `migrate` grava um diário em `~/.movara/backups/<id>/` antes de
mudar qualquer coisa: cópias de cada arquivo que será alterado, bancos
SQLite completos (após um `wal_checkpoint`) e um livro-razão de renomeações
que inclui o diretório movido.

- **`movara backups`** lista os diários — id, data, caminho antigo → novo,
  agentes afetados, contagens de arquivos/bancos/renomeações (`--json` para
  scripts). O diário é a unidade de desfazer: filtrar por caminho ou agente
  não é suportado hoje; apague manualmente diários antigos para liberar
  disco.
- **`movara undo --id <ID>`** reproduz um diário de trás para frente — os
  conteúdos dos arquivos voltam, os bancos são trocados de volta, as
  renomeações se invertem e o diretório movido retorna ao lugar. Use quando
  uma migração apontou para o caminho errado, um agente ainda estava
  rodando durante a mudança ou você simplesmente quer o layout antigo de
  volta. Desfazer é tudo-ou-nada por migração: um id reverte aquela
  migração inteira, não um único agente ou arquivo, e deve rodar antes de
  uma nova migração dos mesmos caminhos.

Trocas via `export` / `import` são registradas da mesma forma: `undo` reverte uma importação por completo, incluindo o estado que ela criou.
Um `move` entre hosts adiciona a árvore do projeto (com `.git`) à troca: `--state-only` descarta o código mas mantém os arquivos de memória do projeto (CLAUDE.md, AGENTS.md, rules); `--cleanup` apaga apenas o conjunto migrado na origem — nunca bancos ou configs compartilhados — e é reversível.

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
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | buckets `wd_<basename>_<sha256[:12]>` (dirs+arquivos) + workDir |

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

Veja [CONTRIBUTING.md](CONTRIBUTING.md) para o guia de adaptadores,
> **A sincronização entre hosts está em desenvolvimento** para o próximo
> lançamento; veja a seção Unreleased de [CHANGELOG.md](CHANGELOG.md).
 convenções de commit e configuração; [CHANGELOG.md](CHANGELOG.md) para o histórico de versões; [SECURITY.md](SECURITY.md) para reportar problemas de segurança; [docs/research.md](docs/research.md) para formatos de armazenamento, codificações e fontes por agente.

## Licença

Copyright (C) 2026 Movara contributors.

Licenciado sob Apache License 2.0 ou MIT, à sua escolha. Veja
[LICENSE-APACHE](LICENSE-APACHE) e [LICENSE-MIT](LICENSE-MIT).
