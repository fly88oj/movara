# Movara

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | **[한국어](README.ko.md)** | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Português](README.pt-BR.md)

AI 코딩 에이전트의 워크스페이스 상태를 휴대 가능하게.

```
~/abc  ──이름 변경──>  ~/cba
  └─ 각 에이전트에 기록된 /home/me/abc  ──movara──>  /home/me/cba
```

## 배경

대부분의 AI 코딩 에이전트(Claude Code, Codex, Gemini CLI 계열, OpenCode, omp, Cursor, Windsurf …)는 세션 히스토리를 **프로젝트 경로**를 키로 저장합니다. 디렉터리 이름은 경로를 어떤 방식으로 인코딩한 값(대시 치환 / sha256 / md5)이고, `cwd` 값은 JSON·JSONL·SQLite·protobuf 파일 안에 들어 있습니다. 디렉터리를 옮기거나 이름을 바꾸면 예전 세션이 "사라진" 것처럼 보입니다 — 실제로는 디스크에 그대로 남아 있을 뿐, 더는 존재하지 않는 경로에 묶여 있는 것입니다. `movara`는 디렉터리 이동과 이 모든 참조의 재키잉을 한 번에 처리하며, 전체 되돌리기도 지원합니다.

## 설치

Rust로 구현되어 런타임 의존성이 없으며 Linux / macOS / Windows에서 동작합니다. 모든 [릴리스](https://github.com/fly88oj/movara/releases)에 빌드된 패키지가 첨부됩니다. 아래 파일명은 `1.2.0` 기준 예시이므로 실제 내려받은 버전으로 바꿔 주세요.

**Debian / Ubuntu (.deb)**

```bash
sudo dpkg -i movara_1.2.0-1_amd64.deb
```

**Fedora / RHEL (.rpm)**

```bash
sudo dnf install movara-1.2.0-1.x86_64.rpm
```

**기타 Linux (tar.gz)**

```bash
tar xzf movara-1.2.0-x86_64-unknown-linux-gnu.tar.gz
sudo cp movara /usr/local/bin/
```

**macOS Apple Silicon (.dmg / tar.gz)**

```bash
# dmg를 열어 bin/movara를 /usr/local/bin에 복사하거나:
tar xzf movara-1.2.0-aarch64-apple-darwin.tar.gz
sudo cp movara /usr/local/bin/
```

Intel Mac은 Rosetta 2로 arm64 빌드를 실행하거나 소스에서 빌드하세요.

**Windows (zip)**

`movara-1.2.0-x86_64-pc-windows-msvc.zip`의 압축을 풀고 `movara.exe`가
있는 폴더를 `PATH`에 추가하세요.

**소스에서 설치**

```bash
cargo install --path .        # `movara` 명령이 설치됩니다
```

UI 언어는 시스템 로캘을 자동으로 따라갑니다(English / 简体中文 / 日本語 / 한국어 / Español / Français / Deutsch / Português). `--lang` 또는 `MOVARA_LANG`으로 변경할 수 있습니다.

## 사용법

```bash
# 일상적인 경우: mv 대신 사용 — 디렉터리 이동과
# 전체 에이전트 히스토리 마이그레이션을 한 번에
movara mv ~/works/abc ~/works/cba
movara mv ~/works/abc ~/works/archived/    # 대상이 이미 있는 디렉터리면 mv 규칙대로 안으로 이동
movara mv --dry-run ~/works/abc ~/works/cba

# 편한 일상용 별칭 (셸 설정 파일에 추가):
alias mva='movara mv'
mva ~/works/abc ~/works/cba

# 어떤 에이전트가 이 경로를 참조하는지 확인
movara scan --from ~/works/abc

# 마이그레이션만 수행 (디렉터리는 이미 이동한 경우)
movara migrate --from ~/works/abc --to ~/works/cba --yes

# 전부 되돌리기
movara backups
movara undo --id 20260903-131427-644777
```

`movara mv`의 동작: 어떤 에이전트 상태가 예전 경로를 참조하는지 스캔해 보여 줍니다 → 확인 → 디렉터리 `mv`(파일 시스템이 다르면 복사+삭제로 자동 전환) → 각 에이전트 마이그레이션 → 되돌리기 ID가 담긴 보고서 출력. 무언가 잘못되면 `movara undo` 한 번으로 디렉터리 위치와 모든 에이전트 상태가 복원됩니다. 디렉터리만 받습니다(파일에는 에이전트 히스토리가 없으므로 일반 `mv`를 쓰세요). 대상이 이미 존재하거나, 대상의 상위 디렉터리가 없거나, 원본==대상이면 거부합니다.

### 하위 명령

| 명령 | 용도 |
|---|---|
| `movara mv <SRC> <DST>` | 디렉터리를 이동(DST가 이미 있으면 안으로)하고 모든 에이전트를 한 번에 마이그레이션 — 일상적인 mv 대체 |
| `movara scan --from <OLD> [--to <NEW>]` | 읽기 전용으로 어떤 에이전트 상태가 경로를 참조하는지 보고. `--to`는 이름 변경 대상 미리보기에만 사용 |
| `movara migrate --from <OLD> --to <NEW>` | 디렉터리를 다른 수단으로 이미 옮긴 뒤 상태를 다시 키잉. `--move-project`는 디렉터리 자체를 먼저 이동 |
| `movara undo --id <ID>` | 한 번의 마이그레이션을 완전히 되돌림 (아래 참고) |
| `movara backups` | 마이그레이션 저널 목록 (아래 참고) |
| `movara agents` | 지원 에이전트와 설치 여부 목록 |
| `movara export [--path PATH]... [--agents LIST]` | 에이전트 상태를 휴대 가능한 `.tar.gz` 로 생성 — 호스트 전체 또는 경로/에이전트로 필터 |
| `movara import <ARCHIVE> [--rebase OLD:NEW]...` | 아카이브를 이 호스트에 복원하고 경로를 재베이스; 마이그레이션처럼 저널에 기록되어 `movara undo` 로 완전히 되돌릴 수 있음 |
| `movara move <SRC> [user@]host:<DST>` | ssh 로 프로젝트와 에이전트 상태를 한 번에 다른 호스트로 이동 — 기억 동반, 정리 선택적 (IPv6 호스트 리터럴 미지원) |
| `movara receive --dst <DST>` | [수신 측] stdin 의 스트림 아카이브 가져오기 (`move` 가 실행) |

### 옵션

| 옵션 | 적용 범위 | 의미 |
|---|---|---|
| `--lang CODE` | 전역 | 표시 언어 변경 (en, zh-CN, ja, ko, es, fr, de, pt-BR) |
| `--agents LIST` | scan, migrate, mv | 쉼표 목록; 특정 에이전트만 처리 (기본값: 설치된 전체) |
| `--extra-root PATH` | scan, migrate, mv | 임의의 트리도 함께 다시 쓰기(dotfile, IDE 설정 등), 반복 지정 가능 |
| `--deep` | migrate, mv | 채팅 내용/로그 안의 경로 언급까지 다시 씀 (기본값: 식별 필드만 — cwd, directory, project, …) |
| `--backup-dir DIR` | migrate, mv, undo, backups | 백업 저널 루트 (기본값 `~/.movara/backups`) |
| `--dry-run` | migrate, mv | 보고만 하고 변경하지 않음 |
| `--yes` | migrate, mv | 확인 프롬프트 건너뛰기 |
| `--move-project` | migrate | 다시 키잉하기 전에 프로젝트 디렉터리 자체를 이동 |
| `--json` | agents, scan, migrate, mv, backups | stdout으로 단일 JSON 문서 출력(기계 판독용) |
| `--out FILE` | export | 아카이브 경로 (기본값 `movara-export-<타임스탬프>.tar.gz`) |
| `--path PATH` | export | 이 프로젝트 경로를 참조하는 상태만 (반복 가능; `--agents` 와 교집합) |
| `--rebase OLD:NEW` | import | 경로 매핑, 반복 지정 가능; 중복·연쇄 규칙은 거부됨 |
| `--dst <DST>` | receive | 이 호스트의 프로젝트 대상 디렉터리 |
| `--plan-only` | receive | 대상 사전 점검 후 종료 |
| `--yes` | receive | 비대화형 (스트리밍 수신 시 필수) |
| `--on-conflict POLICY` | import | 기존 로컬 상태에 대해 `skip`(기본값) 또는 `replace` |
| `--allow-missing-path` | import | 아카이브 경로가 로컬에 없어도 계속 |
| `--state-only` | move | 코드 없이 에이전트 상태와 프로젝트 기억만 운반 |
| `--cleanup` | move | 검증 성공 후 원본의 이동된 상태 세트 삭제 (공유 DB/설정은 유지) |

### 되돌리기와 백업

`mv` / `migrate`는 무엇이든 변경하기 전에 `~/.movara/backups/<id>/`에
저널을 기록합니다: 변경될 파일의 사본, SQLite 데이터베이스 전체
(`wal_checkpoint` 수행 후), 이동된 프로젝트 디렉터리를 포함한 이름 변경
대장.

- **`movara backups`**는 저널을 나열합니다 — ID, 날짜, 이전→새 경로, 관련
  에이전트, 파일/DB/이름 변경 수(`--json`은 스크립트용). 저널이 되돌리기의
  단위이며, 경로나 에이전트별 필터링은 현재 지원되지 않습니다. 디스가
  부족하면 오래된 저널을 직접 삭제하면 됩니다.
- **`movara undo --id <ID>`**는 저널을 거꾸로 재생합니다 — 파일 내용을
  복원하고, DB를 되돌리고, 이름 변경을 반대로 하고, 이동된 디렉터리를
  제자리로 돌려보냅니다. 마이그레이션 대상을 잘못 골랐거나, 이동 중
  에이전트가 실행 중이었거나, 단순히 예전 레이아웃으로 돌아가고 싶을 때
  사용합니다. 되돌리기는 마이그레이션 한 건 단위의 전부 아니면 전무입니다.
  하나의 ID가 해당 마이그레이션 전체를 되돌리며 특정 에이전트나 파일만
  되돌릴 수는 없고, 같은 경로를 다시 마이그레이션하기 전에 실행해야
  합니다.

`export` / `import` 도 같은 방식으로 저널에 기록됩니다. `undo` 는 생성된 상태를 포함해 가져오기를 완전히 되돌립니다.
크로스호스트 `move` 는 프로젝트 트리(`.git` 포함)도 교환합니다. `--state-only` 는 코드를 제외하되 프로젝트 내 기억 파일(CLAUDE.md, AGENTS.md, rules)은 유지하고, `--cleanup` 은 이동된 상태 세트만 삭제합니다(공유 DB 나 설정은 절대 삭제하지 않음) — 그 자체도 되돌릴 수 있습니다.

## 안전성

- **경계 인식 치환**: `/a/abc`는 `/a/abc2`나 `/a/abc-def`를 절대 건드리지 않습니다. `file://` URI, JSON 이스케이프, 하위 경로(`/a/abc/sub`) 모두 올바르게 처리됩니다.
- **파생 토큰도 함께 치환**: sha256 전체 해시(gemini `projectHash`, qwen/iflow tmp 디렉터리), sha256[:16](zcode 메모리 키), md5(windsurf context_state/database 디렉터리), 그리고 각 벤더의 대시 인코딩 디렉터리 이름.
- 마이그레이션마다 먼저 전체 백업: 변경된 파일과 SQLite 데이터베이스는(`wal_checkpoint` 수행 후) 복사되고, 이름 변경은 저널에 기록되며, `undo`가 전부 복원합니다. `movara`가 옮긴 디렉터리도 함께 복원됩니다.
- 대상이 존재하면 이름 변경을 건너뛰고 목록으로 보여 줍니다. `--from /`은 거부됩니다.
- 마이그레이션 대상 에이전트는 먼저 종료하세요(WAL 데이터베이스는 경고하지만 손상시키지는 않습니다).


## 지원 에이전트

| 에이전트 | 상태 위치 | 경로 키 |
|---|---|---|
| Claude Code | `~/.claude/projects/<dash>/`, `~/.claude.json` | 대시 디렉터리 + `projects` 키 + `cwd` |
| OpenAI Codex | `~/.codex/sessions/**/rollout-*.jsonl`, state_*.sqlite | `session_meta.payload.cwd`, `threads.cwd` |
| Gemini CLI | `~/.gemini/tmp/<slug>/`, projects.json | sha256(cwd) + slug(basename) |
| Qwen Code | `~/.qwen/projects/<dash>/`, `~/.qwen/tmp/<sha256>` | 대시 디렉터리 + sha256 + `cwd` |
| iFlow CLI | `~/.iflow/projects/<fromPath>/`, tmp/history/cache/snapshots `<sha256>` | 자체 인코딩 + sha256 |
| OpenCode | `~/.local/share/opencode/opencode.db` | session/project/workspace의 directory 컬럼 |
| Oh My Pi (omp) | `~/.omp/agent/sessions/<omp-bucket>/`, history.db | home 상대 대시 버킷 + `cwd` |
| ZCode | `~/.zcode/cli/db/db.sqlite`, memories/ | session.directory/path, workflow_run.cwd |
| Cursor (IDE+CLI) | `~/.config/Cursor/.../state.vscdb`, `~/.cursor/projects/<dash>/` | fsPath/file:// URI + composerData |
| Windsurf | `~/.codeium/windsurf/` + IDE state.vscdb | md5(path) + file:// URI |
| Antigravity | `~/.config/Antigravity/.../state.vscdb` + `~/.gemini/antigravity` | VS Code fork와 동일 |
| Crush | `<project>/.crush/crush.db` + 전역 projects.json | path/data_dir |
| Factory Droid | `~/.factory/sessions/<encoded>/` | realpath, 슬래시만 치환 |
| Continue | `~/.continue/sessions/*.json`, index.sqlite | file:// URI + tag_catalog.dir |
| pi / gsd | `~/.pi/agent/sessions/--<enc>--/` | `--encoded--` 버킷 + `cwd` |
| Zed | `~/.local/share/zed/threads/threads.db` | threads.folder_paths |
| Aider | `~/.aider.conf.yml` | 설정 안의 절대 경로 |
| cc-connect | `~/.cc-connect/dir_history.json`, `sessions/<name>_<sha256[:8]>.json` | 디렉터리 MRU + 파일명 해시 |
| Kimi Code | `~/.kimi-code/` workspaces.json, session_index.jsonl, sessions/, file-history/, workspace-trust/ | `wd_<basename>_<sha256[:12]>` 버킷 디렉터리+파일 + workDir |
| Goose (Block) | `~/.local/share/goose/sessions/sessions.db` + 레거시 `*.jsonl`, `~/.config/goose/` | `sessions.working_dir` + `working_dir` 메타데이터 + 권한 키 |
| Cline / Roo Code / Kilo Code | `~/.config/<IDE>/User/globalStorage/{claude-dev,roo-code,kilo-code}` | 태스크 `path` 필드 + `workspace`/`cwdOnTaskInitialization` + 체크포인트 `core.worktree` + cwdHash/sha256 버킷 |
| OpenHands | `~/.openhands/` | `working_dir` + `projects/<sha256(realpath)>/` |
| Codebuff / Freebuff | `~/.config/manicode/projects/<basename>/` | basename 버킷 + run-state `cwd` |
| gptme | `~/.local/share/gptme/logs/<date>-<name>/` | `config.toml [chat] workspace`(틸드 형식 포함) + `workspace` 심볼릭 링크 + `files` 목록 |
| Qoder / Lingma (CN) | `~/.config/Qoder` + `~/.qoder` + `~/.lingma/qoder-cn` | IDE state.vscdb + `memories/<account>/projects/<dash>/` |
| Trae (ByteDance) | `~/.config/Trae CN` + `~/.trae` | IDE state.vscdb + agents/mcp.json |
| GitHub Copilot CLI | `~/.copilot` | agents/hooks/skills 정의 |
| Warp | `~/.local/share/warp/warp.db` | 범용 텍스트 열 스윕(폐쇄 스키마) |

지원하지 않음(의도된 결정):

- **GitHub Copilot CLI** — 로컬 스키마가 비공개이며 클라우드가 출처.
- **Amp** — 스레드가 서버에 저장됨.
- **claude-code-router** — 경로로 키잉된 상태 없음(소스에서 확인).

## 기여

```bash
git clone https://github.com/fly88oj/movara && cd movara
cargo test --all                          # 32개 테스트 통과 필수
cargo clippy --all-targets -- -D warnings # 경고가 없어야 함
cargo fmt --all -- --check
pre-commit install                        # 로컬 훅 (CI와 동일)
```

어댑터 가이드·커밋 규칙·설정은 [CONTRIBUTING.md](CONTRIBUTING.md), 릴리스 역사는
> **크로스호스트 동기화(sync)는 다음 릴리스를 위해 개발 중입니다** —
> [CHANGELOG.md](CHANGELOG.md)의 Unreleased 항목을 참고하세요.
 [CHANGELOG.md](CHANGELOG.md), 보안 이슈 신고는 [SECURITY.md](SECURITY.md), 에이전트별 저장 포맷·인코딩·출처는 [docs/research.md](docs/research.md)를 참고하세요.

## 라이선스

Copyright (C) 2026 Movara contributors.

본 프로젝트는 Apache License 2.0 또는 MIT license 듀얼 라이선스입니다. 자세한 내용은 [LICENSE-APACHE](LICENSE-APACHE)와 [LICENSE-MIT](LICENSE-MIT)를 참고하세요.
