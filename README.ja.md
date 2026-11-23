[English](README.md) | [한국어](README.ko.md) | **日本語** | [简体中文](README.zh-CN.md)

# brain

ブレインサービス `bmo-brain` は、エージェントの長期記憶をサービスとして提供します。一つのアカウントと一つの
エージェントに属するブレインを複数保持し、各ブレインはサブジェクト（その記憶が何についてのものか。タイプと
キーで引きます）、レイヤー（目次）、メモリーから成ります。メモリーはバージョンごとに一行で、置き換えや無効化は
されても削除はされません。ターンはブレインのリコールにトークン予算内のブロックを求め、ペルソナは構成要素が
変わるまで同じバイト列としてレンダリングされ、ブレインはブレインファイルとして出し入れできます。ブレインは
何も実行せず、他のユニットを呼び出さず、ブレインの外にあるものは保持しません。ハーネスプラグイン
`brain-harness` は、同じバイナリが `bmo-brain mcp` として起動する MCP サーバーと、ハーネスにその使い方を
教えるスキルから成ります。

`docs/design.md` はブレインの構造と守るべきルールを、`docs/boundary-api.md` はすべてのルート、イベント、
ツール、コードを説明します。`docs/openapi.json` は同じ契約を機械可読な形にしたもので、クライアントの型から
生成されます。

## 個人データ

このリポジトリに個人データはありません。人の名前、ハンドル、アカウント ID、メールアドレス、電話番号、
ホームパス、ホスト名、ドメイン、トークン、プライベートネットワークのアドレスは、コード、コメント、ドキュメント、
テストフィクスチャ、コミットメッセージのいずれにも含めません。ルールとそれを守るチェックは
`docs/PERSONAL-DATA.md` にあります。

## モデル

ブレインは HTTP で問い合わせる埋め込みモデルで意味検索を行い、リポジトリにモデルは含まれていません。`[embedding]` 設定に OpenAI 互換の `/v1/embeddings` のアドレス、キーを読むファイル、モデル名、ベクトルの長さ（既定 1024、`docs/boundary-api.md`）を書きます。その長さのベクトルを返すモデルなら何でもよく、LLM ゲートウェイの背後に置いた Qwen3-Embedding-0.6B（1024 次元）のような小さなローカルモデルもその一つです。モデルを変えるには設定を変えてから `bmo-brain embed` で生きている行をすべて埋め込み直します。`[embedding]` がなければ語だけで検索します。モデルはそれぞれのライセンスに従い、このリポジトリのコードは MIT です。

## 構成

| Folder | Holds |
|---|---|
| `src/` | サービス自身のパッケージ: ストア（ブレイン、サブジェクト、レイヤー、メモリー、リコール、シート、ブレインファイル）、ルート、クロック、MCP サーバー、OpenAPI ドキュメント、シート分割 |
| `crates/` | `client/`: クレート `bmo-brain-client`。すべてのボディとメンバーを Rust の型として持ち、ルートごとにメソッドを一つずつ提供します |
| `package/` | `docs/openapi.json` から生成される TypeScript パッケージ `bmo-brain-client`: すべてのボディの型と、ルートごとの型付き呼び出し |
| `client/` | ブレインの画面: ブレインが配信するコンソールバンドルにビルドされる、bmo-ui 上の Nuxt レイヤー。ページ、コンポーネント、文言、ビルド、テストを含みます |
| `migrations/` | バイナリに組み込まれるデータベースのマイグレーション |
| `skills/` | `brain-harness/`: プラグインのスキル |
| `docs/` | 設計、境界ドキュメント、OpenAPI ドキュメント、テスト用データベース、個人データのルール |
| `tests/` | サービスのテストと、リポジトリ全体にまたがるチェック |
| `deploy/` | デプロイ時にデプロイツリーからリンクされる、サービスのユーザーユニット `bmo-brain.service` |
| `tools/` | ツールホスト向けのツール: ブレインの `brain/` とメモリーツールの `memory/`。それぞれマニフェスト、エントリーのスキーマ、実行ファイルのクレートを持ちます |

テストベッドの宣言 `testbed.toml` には、テストベッドがこのリポジトリをどうテスト、チェック、ビルド、デプロイ
するかが書かれています。

## ビルドとテスト

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

テストは `BMO_TEST_POSTGRES` が指す PostgreSQL サーバー（指定がなければローカルソケットのサーバー）に自前の
データベースを作ります。`client/tests/browser/` 以下のコンソールバンドルのテストは、playwright-core を通じて
実際の Chromium で画面を操作します。画面は `client/tests/browser/host/` 以下の Nuxt ホストに単独で載せられ、
ブレインのルートにはテスト内のスタンドインが応答します。playwright-core が固定しているブラウザーがマシンに
なければ、これらのテストはスキップされます。

## クライアント

サービスは二つの言語でクライアントを公開しており、どちらも `docs/openapi.json` から作られます。`crates/client`
以下の Rust クレート `bmo-brain-client` と、`package/` 以下の TypeScript パッケージ `bmo-brain-client` です。

- 下流のチェックアウトは TypeScript パッケージをパックからインストールします。`package/` で `bun pm pack` を
  実行すると `bmo-brain-client-<version>.tgz` ができ、チェックアウトはそれをパスで追加します。パッケージの
  バージョンはサービスのバージョンと同じです。
- `brainClient(<address>, <token>)` は、ドキュメントのルートを呼び出しとし、そのドキュメントで型付けされた
  クライアントを返します。例: `brain.GET("/brains/{brainId}/memories/{memoryId}", { params: { path: { brainId, memoryId } } })`。
  すべての呼び出しはトークンを伴い、`GET /events` は一ページ分を返します。ライブストリームを読む呼び出し側は
  server-sent events を自分で読みます。
- `bun run --cwd package generate` はドキュメントからパッケージの型を生成し、コミット済みの型と異なる場合は
  パッケージのチェックが失敗します。

## 実行

- `bmo-brain --config <file> serve` はマイグレーションを実行してから listen アドレスで応答します。設定は TOML
  ファイルで、必須キーは `listen`、`token`、`database`、任意のものが `data_root`、`[rules]`、`[embedding]`、
  `[console]` です（`docs/boundary-api.md`、「Configuration」）。
- `bmo-brain --config <file> health` は、そのサービスが準備完了と応答した場合にのみ 0 で終了します。
- `bmo-brain mcp --brain <url>` は標準入出力上の MCP でハーネスツールを提供し、トークンは `BRAIN_TOKEN` から
  読みます。
- `bmo-brain openapi` は OpenAPI ドキュメントを出力します。`docs/openapi.json` はその出力です。
- `bmo-brain --config <file> embed` は、設定されたモデルのベクトルを持たないすべての有効な行を埋め込みます。
- `bmo-brain --config <file> tool-env` は、ブレインのツールがサービスのアドレスとトークンを読む env ファイルを
  データルート以下に書き出します。
- `bmo-brain --config <file> sheets split` は有効なシートの行をファクトに分割します。`--dry-run` を付けると何も
  書き込みません。
