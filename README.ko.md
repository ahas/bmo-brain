[English](README.md) | **한국어** | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# brain

브레인 서비스 `bmo-brain`은 에이전트의 장기 기억을 서비스로 제공합니다. 계정 하나와 에이전트 하나에 속하는
브레인을 여러 개 보관하며, 각 브레인은 주제(subject: 기억이 무엇에 관한 것인지, 타입과 키로 찾습니다),
레이어(목차), 기억으로 이루어집니다. 기억은 버전마다 한 행이고, 대체되거나 무효화될 수는 있어도 삭제되지는
않습니다. 턴은 브레인의 회상(recall)에 토큰 예산 안의 블록을 요청하고, 페르소나는 구성 요소가 바뀌기 전까지
같은 바이트로 렌더링되며, 브레인은 브레인 파일로 내보내고 들여올 수 있습니다. 브레인은 아무것도 실행하지
않고, 다른 유닛을 호출하지 않으며, 브레인 바깥의 것은 보관하지 않습니다. 하네스 플러그인 `brain-harness`는
같은 바이너리가 `bmo-brain mcp`로 띄우는 MCP 서버와, 하네스에 그 사용법을 알려 주는 스킬로 이루어집니다.

`docs/design.md`는 브레인의 구조와 브레인이 지키는 규칙을, `docs/boundary-api.md`는 모든 라우트, 이벤트,
도구, 코드를 설명합니다. `docs/openapi.json`은 같은 계약을 기계가 읽는 형태로 담은 것으로, 클라이언트의
타입에서 생성됩니다.

## 개인 데이터

이 저장소에는 개인 데이터가 없습니다. 사람의 이름, 핸들, 계정 ID, 메일 주소, 전화번호, 홈 경로, 호스트 이름,
도메인, 토큰, 사설 네트워크 주소 중 어느 것도 코드, 주석, 문서, 테스트 픽스처, 커밋 메시지에 들어가지
않습니다. 규칙과 이를 지키는 검사는 `docs/PERSONAL-DATA.md`에 있습니다.

## 모델

브레인은 HTTP로 묻는 임베딩 모델로 의미 검색을 하며, 저장소에는 모델이 없습니다. `[embedding]` 설정에 OpenAI 호환 `/v1/embeddings` 주소, 키를 읽을 파일, 모델 이름, 벡터 길이(기본 1024, `docs/boundary-api.md`)를 적습니다. 그 길이의 벡터를 내는 모델이면 무엇이든 되며, LLM 게이트웨이 뒤의 Qwen3-Embedding-0.6B(1024차원) 같은 작은 로컬 모델도 그중 하나입니다. 모델을 바꾸려면 설정을 고친 뒤 `bmo-brain embed`로 살아 있는 행을 모두 다시 임베딩합니다. `[embedding]`이 없으면 낱말로만 검색합니다. 모델은 저마다의 라이선스를 따르고, 이 저장소의 코드는 MIT입니다.

## 구성

| Folder | Holds |
|---|---|
| `src/` | 서비스 자체 패키지: 저장소(브레인, 주제, 레이어, 기억, 회상, 시트, 브레인 파일), 라우트, 클록, MCP 서버, OpenAPI 문서, 시트 분할 |
| `crates/` | `client/`: 크레이트 `bmo-brain-client`. 모든 본문과 멤버를 Rust 타입으로, 라우트마다 메서드 하나씩 제공합니다 |
| `package/` | `docs/openapi.json`에서 생성한 TypeScript 패키지 `bmo-brain-client`: 모든 본문의 타입과 라우트마다 타입이 붙은 호출 |
| `client/` | 브레인 화면: 브레인이 제공하는 콘솔 번들에 빌드되는 bmo-ui 기반 Nuxt 레이어로, 페이지, 컴포넌트, 문구, 빌드, 테스트를 담습니다 |
| `migrations/` | 바이너리에 포함되는 데이터베이스 마이그레이션 |
| `skills/` | `brain-harness/`: 플러그인의 스킬 |
| `docs/` | 설계, 경계 문서, OpenAPI 문서, 테스트 데이터베이스, 개인 데이터 규칙 |
| `tests/` | 서비스의 테스트와 저장소 전체에 걸친 검사 |
| `deploy/` | 배포 시 배포 트리에서 링크되는 서비스의 사용자 유닛 `bmo-brain.service` |
| `tools/` | 도구 호스트용 도구: 브레인의 `brain/`과 메모리 도구 `memory/`. 각각 매니페스트, 엔트리 스키마, 실행 파일의 크레이트를 담습니다 |

테스트베드 선언 `testbed.toml`에는 테스트베드가 이 저장소를 어떻게 테스트하고, 검사하고, 빌드하고, 배포하는지가
적혀 있습니다.

## 빌드와 테스트

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
bun install --cwd package --frozen-lockfile
bun run --cwd package check
bun install --cwd client --frozen-lockfile
bun run --cwd client check
```

테스트는 `BMO_TEST_POSTGRES`가 가리키는 PostgreSQL 서버에, 지정이 없으면 로컬 소켓의 서버에 자체
데이터베이스를 만듭니다. `client/tests/browser/` 아래의 콘솔 번들 테스트는 playwright-core로 실제 Chromium에서
화면을 조작합니다. 화면은 `client/tests/browser/host/` 아래의 Nuxt 호스트에 단독으로 올라가고, 브레인의 라우트는
테스트 안의 대역이 응답합니다. playwright-core가 고정한 브라우저가 머신에 없으면 이 테스트들은 건너뜁니다.

## 클라이언트

서비스는 두 언어로 된 클라이언트를 배포하며, 둘 다 `docs/openapi.json`에서 만들어집니다. `crates/client` 아래의
Rust 크레이트 `bmo-brain-client`와 `package/` 아래의 TypeScript 패키지 `bmo-brain-client`입니다.

- 다운스트림 체크아웃은 TypeScript 패키지를 팩으로 설치합니다. `package/`에서 `bun pm pack`을 실행하면
  `bmo-brain-client-<version>.tgz`가 만들어지고, 체크아웃은 이를 경로로 추가합니다. 패키지 버전은 서비스 버전과
  같습니다.
- `brainClient(<address>, <token>)`는 문서의 라우트를 호출로 삼고 그 문서로 타입이 매겨진 클라이언트를
  돌려줍니다. 예: `brain.GET("/brains/{brainId}/memories/{memoryId}", { params: { path: { brainId, memoryId } } })`.
  모든 호출에는 토큰이 실리고, `GET /events`는 한 페이지를 돌려줍니다. 실시간 스트림을 읽으려는 호출자는
  server-sent events를 직접 읽습니다.
- `bun run --cwd package generate`는 문서에서 패키지의 타입을 생성하며, 커밋된 타입과 다르면 패키지의 검사가
  실패합니다.

## 실행

- `bmo-brain --config <file> serve`는 마이그레이션을 실행한 뒤 listen 주소에서 응답합니다. 설정은 TOML
  파일로, 필수 키는 `listen`, `token`, `database`이고 `data_root`, `[rules]`, `[embedding]`, `[console]`은
  선택입니다(`docs/boundary-api.md`, 「Configuration」).
- `bmo-brain --config <file> health`는 해당 서비스가 준비 완료로 응답할 때만 0으로 끝납니다.
- `bmo-brain mcp --brain <url>`은 표준 입출력으로 MCP를 통해 하네스 도구를 제공하며, 토큰은 `BRAIN_TOKEN`에서
  읽습니다.
- `bmo-brain openapi`는 OpenAPI 문서를 출력합니다. `docs/openapi.json`이 그 출력입니다.
- `bmo-brain --config <file> embed`는 설정된 모델의 벡터가 없는 모든 활성 행을 임베딩합니다.
- `bmo-brain --config <file> tool-env`는 브레인의 도구가 서비스 주소와 토큰을 읽어 갈 env 파일을 데이터 루트
  아래에 씁니다.
- `bmo-brain --config <file> sheets split`은 활성 시트 행을 사실(fact)들로 나눕니다. `--dry-run`을 주면 아무것도
  쓰지 않습니다.
