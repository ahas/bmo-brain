[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | **简体中文**

# brain

大脑服务 `bmo-brain` 以服务的形式提供智能体的长期记忆。它保存多个大脑，每个大脑属于一个账户和一个智能体，
由主题（subject：记忆是关于什么的，按类型和键查找）、层（目录）和记忆组成。每个版本的记忆占一行，可以被取代
或作废，但永远不会被删除。一轮对话向大脑的回忆（recall）请求一个不超过 token 预算的块；人设在其组成部分变化
之前都会渲染成相同的字节；大脑可以以大脑文件的形式导出和导入。大脑不执行任何东西，不调用其他单元，也不保存
大脑之外的任何内容。它的 harness 插件 `brain-harness` 由同一个二进制以 `bmo-brain mcp` 启动的 MCP 服务器，
以及告诉 harness 如何使用它的技能组成。

`docs/design.md` 说明大脑的结构及其遵守的规则；`docs/boundary-api.md` 说明每条路由、事件、工具和代码；
`docs/openapi.json` 是同一契约的机器可读形式，由客户端的类型生成。

## 个人数据

本仓库不含任何个人数据：代码、注释、文档、测试夹具和提交信息中都没有任何人的姓名、昵称、账户 ID、邮箱地址、
电话号码、主目录路径、主机名、域名、令牌或私有网络地址。规则及守住它的检查见 `docs/PERSONAL-DATA.md`。

## 模型

大脑通过 HTTP 调用嵌入模型进行语义检索，仓库本身不含模型。`[embedding]` 设置中写明 OpenAI 兼容的 `/v1/embeddings` 地址、读取密钥的文件、模型名称以及向量长度（默认 1024，见 `docs/boundary-api.md`）。只要能返回该长度向量的模型都可以，例如放在 LLM 网关后面的小型本地模型 Qwen3-Embedding-0.6B（1024 维）。更换模型只需修改设置，再运行 `bmo-brain embed` 重新嵌入所有在用的记录。没有 `[embedding]` 时只按词检索。模型遵循各自的许可证，本仓库代码采用 MIT 许可。

## 结构

| Folder | Holds |
|---|---|
| `src/` | 服务自身的包：存储（大脑、主题、层、记忆、回忆、表单和大脑文件）、路由、时钟、MCP 服务器、OpenAPI 文档以及表单拆分 |
| `crates/` | `client/`，即 crate `bmo-brain-client`：每个请求体和成员都对应一个 Rust 类型，每条路由对应一个方法 |
| `package/` | 由 `docs/openapi.json` 生成的 TypeScript 包 `bmo-brain-client`：每个请求体的类型，以及每条路由的类型化调用 |
| `client/` | 大脑的界面：基于 bmo-ui 的 Nuxt 层，构建进大脑所提供的控制台包中，包含页面、组件、文案、构建和测试 |
| `migrations/` | 内置于二进制中的数据库迁移 |
| `skills/` | `brain-harness/`，插件的技能 |
| `docs/` | 设计、边界文档、OpenAPI 文档、测试数据库说明以及个人数据规则 |
| `tests/` | 服务的测试以及横跨整个仓库的检查 |
| `deploy/` | 服务的用户单元 `bmo-brain.service`，部署时从部署树链接 |
| `tools/` | 供工具宿主使用的工具：大脑的 `brain/` 和记忆工具 `memory/`，各自包含清单、条目的 schema 以及可执行文件的 crate |

testbed 的声明 `testbed.toml` 说明了 testbed 如何测试、检查、构建和部署本仓库。

## 构建与测试

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

测试会在 `BMO_TEST_POSTGRES` 指定的 PostgreSQL 服务器上创建自己的数据库；未指定时使用本地 socket 上的服务器。
`client/tests/browser/` 下的控制台包测试通过 playwright-core 在真实的 Chromium 中操作界面：界面单独运行在
`client/tests/browser/host/` 下的 Nuxt 宿主上，大脑的路由由测试中的替身应答。如果机器上没有 playwright-core
所固定的浏览器，这些测试会被跳过。

## 客户端

服务以两种语言发布客户端，二者都由 `docs/openapi.json` 生成：`crates/client` 下的 Rust crate
`bmo-brain-client`，以及 `package/` 下的 TypeScript 包 `bmo-brain-client`。

- 下游检出从打包文件安装 TypeScript 包：在 `package/` 中运行 `bun pm pack` 会生成
  `bmo-brain-client-<version>.tgz`，检出再按路径添加它。包的版本与服务的版本一致。
- `brainClient(<address>, <token>)` 返回一个客户端，它的调用就是文档中的路由，并由文档提供类型：
  `brain.GET("/brains/{brainId}/memories/{memoryId}", { params: { path: { brainId, memoryId } } })`。每次调用都会携带令牌，
  `GET /events` 返回一页结果；需要读取实时流的调用方自行读取 server-sent events。
- `bun run --cwd package generate` 根据文档生成包的类型；若与已提交的类型不一致，包的检查就会失败。

## 运行

- `bmo-brain --config <file> serve` 先执行迁移，然后在监听地址上应答。配置是一个 TOML 文件，必需的键为
  `listen`、`token` 和 `database`；`data_root`、`[rules]`、`[embedding]` 和 `[console]` 为可选项
  （`docs/boundary-api.md`，「Configuration」）。
- `bmo-brain --config <file> health` 仅在该服务应答就绪时以 0 退出。
- `bmo-brain mcp --brain <url>` 通过标准输入输出以 MCP 提供 harness 工具，令牌取自 `BRAIN_TOKEN`。
- `bmo-brain openapi` 输出 OpenAPI 文档；`docs/openapi.json` 就是它的输出。
- `bmo-brain --config <file> embed` 为所有缺少所配置模型向量的有效行生成嵌入。
- `bmo-brain --config <file> tool-env` 在数据根目录下写入 env 文件，大脑的工具从中读取服务的地址和令牌。
- `bmo-brain --config <file> sheets split` 将有效的表单行拆分为事实；加上 `--dry-run` 则不写入任何内容。
