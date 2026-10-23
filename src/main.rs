//! The `bmo-brain` command (docs/boundary-api.md, 「Configuration」, 「The command」 and 「The harness plugin」):
//! `serve` runs the migrations, stands the clock and answers the routes until it is told to stop; `health`
//! ends 0 only where the service its configuration names answers ready; `mcp` serves the harness tools over
//! MCP on standard input and output; `openapi` prints the OpenAPI document; `sheets split` splits the live
//! sheet rows of the configuration's database into facts; `nouns` declares the nouns `bmo` dispatches to this
//! binary, and each noun asks the brain's own routes.

use std::path::PathBuf;
use std::process::ExitCode;

use bmo_brain::api::console::{Console, bundle_beside_binary};
use bmo_brain::api::{Brain, router};
use bmo_brain::clock;
use bmo_brain::command::{self, NOUNS};
use bmo_brain::config::{self, Config, HEALTH_PATIENCE};
use bmo_brain::mcp::BrainServer;
use bmo_brain::openapi::openapi_json;
use bmo_brain::store::Store;
use bmo_brain::store::sheets::SheetSplit;
use bmo_brain_client::BrainClient;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[derive(Parser)]
#[command(name = "bmo-brain", version, about = "The brain service and its command.")]
struct Command {
  /// The configuration file; `BMO_BRAIN_CONFIG` where none is given, and the one a deployment links
  /// beside the binary for a noun command.
  #[arg(long, global = true, env = "BMO_BRAIN_CONFIG")]
  config: Option<PathBuf>,
  /// The road the caller's turn is read on, handed on by `bmo`; the brain reads no turn and passes it over.
  #[arg(long, global = true, value_name = "road", hide = true)]
  via: Option<String>,
  #[command(subcommand)]
  verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
  /// Runs the migrations, then answers the routes until the process is told to stop.
  Serve,
  /// Whether the service the configuration names answers ready: exit code 0 where it does, 1 where not.
  Health,
  /// Serves the harness tools over MCP on standard input and output, calling the brain at one address.
  Mcp {
    /// The base address the brain's routes stand at.
    #[arg(long, value_name = "url", env = "BRAIN_URL")]
    brain: String,
    /// The brain's bearer token.
    #[arg(long, env = "BRAIN_TOKEN", hide_env_values = true, default_value = "")]
    token: String,
  },
  /// Prints the OpenAPI document.
  Openapi,
  /// Prints the nouns `bmo` dispatches to this command, one a line.
  Nouns,
  /// Writes the env file the brain's tool reads the service's address and token from, under the data root.
  ToolEnv {
    /// The data root; the configuration's where none is given.
    #[arg(long, value_name = "dir")]
    data_root: Option<PathBuf>,
  },
  /// The live sheet rows of the configuration's database, split into facts.
  Sheets {
    #[command(subcommand)]
    word: SheetsWord,
  },
  /// The memory rows.
  Memory {
    #[command(subcommand)]
    word: command::memory::Word,
  },
  /// The fragment table.
  Fragment {
    #[command(subcommand)]
    word: command::fragment::Word,
  },
}

#[derive(Subcommand)]
enum SheetsWord {
  /// Splits every live sheet row into facts, one per line, and moves the sheet to done; prints ids and
  /// counts alone.
  Split {
    /// Prints what each sheet would come to, and writes nothing.
    #[arg(long)]
    dry_run: bool,
  },
}

async fn stopped() {
  let interrupted = tokio::signal::ctrl_c();
  let terminated = async {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
      Ok(mut terminate) => {
        terminate.recv().await;
      }
      Err(_) => std::future::pending::<()>().await,
    }
  };
  tokio::select! {
    _ = interrupted => {}
    () = terminated => {}
  }
}

async fn serve(config: &Config) -> Result<(), String> {
  let store = Store::lazy(&config.database)?.by(config.rules.clone());
  store.migrate().await.map_err(|why| format!("the migrations: {why}"))?;
  let listener = tokio::net::TcpListener::bind(&config.listen)
    .await
    .map_err(|why| format!("{}: {why}", config.listen))?;
  println!("bmo-brain: answering on {}", config.listen);
  let clock = clock::stand(store.clone());
  let console = config.console.as_ref().map(|console| {
    let folder = console.bundle.clone().or_else(bundle_beside_binary).unwrap_or_default();
    Console::over(console.token.clone(), &folder, console.calls.clone())
  });
  let brain = Brain {
    store,
    token: config.token.clone(),
    console,
  };
  let served = axum::serve(listener, router(brain))
    .with_graceful_shutdown(stopped())
    .await
    .map_err(|why| why.to_string());
  clock.abort();
  served
}

async fn health(config: &Config) -> Result<(), String> {
  let base = config.base_address();
  let http = reqwest_client()?;
  let client = BrainClient::new(&base, config.token.clone())?.with_http(http);
  let health = client.health().await.map_err(|why| format!("{base}: {why}"))?;
  if health.ready {
    Ok(())
  } else {
    Err(format!(
      "the service is not ready: {}",
      health.reason.as_deref().unwrap_or("it gave no reason")
    ))
  }
}

fn reqwest_client() -> Result<bmo_brain_client::HttpClient, String> {
  bmo_brain_client::HttpClient::builder()
    .no_proxy()
    .timeout(HEALTH_PATIENCE)
    .build()
    .map_err(|why| format!("no HTTP client: {why}"))
}

async fn mcp(brain: &str, token: String) -> Result<(), String> {
  let server = BrainServer::new(BrainClient::new(brain, token)?);
  let served = server
    .serve(stdio())
    .await
    .map_err(|why| format!("the MCP session did not start: {why}"))?;
  served
    .waiting()
    .await
    .map(drop)
    .map_err(|why| format!("the MCP session ended in a failure: {why}"))
}

fn configured(path: Option<&PathBuf>) -> Result<Config, String> {
  let path = path.ok_or("--config names no configuration file")?;
  Config::read(path).map_err(|why| format!("the configuration could not be read: {why}"))
}

/// The sheets split, or what they would come to, as lines of ids and counts: a line per sheet, then a line per
/// brain.
async fn sheets_split(config: &Config, dry_run: bool) -> Result<(), String> {
  let store = Store::lazy(&config.database)?.by(config.rules.clone());
  let splits = store.sheets_split(dry_run).await.map_err(|why| why.to_string())?;
  let mut brains: std::collections::BTreeMap<i64, (i64, i64, i64)> = std::collections::BTreeMap::new();
  for SheetSplit {
    sheet_id,
    brain_id,
    subject_ids,
    lines,
    facts,
    headings,
    empty,
  } in &splits
  {
    let subjects = subject_ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    println!(
      "sheet {sheet_id} brain {brain_id} subjects {subjects} lines {lines} facts {facts} headings {headings} \
       empty {empty}"
    );
    let brain = brains.entry(*brain_id).or_default();
    brain.0 += 1;
    brain.1 += facts;
    brain.2 += headings + empty;
  }
  for (brain_id, (sheets, facts, skipped)) in brains {
    println!("brain {brain_id} sheets {sheets} facts {facts} skipped {skipped}");
  }
  Ok(())
}

/// The service a noun command asks: the configuration `--config` or `BMO_BRAIN_CONFIG` names, else the one
/// a deployment links beside the binary's directory.
fn command_client(path: Option<&PathBuf>) -> Result<BrainClient, String> {
  let path = path
    .cloned()
    .or_else(config::beside_binary)
    .ok_or("--config names no configuration file, and none stands beside the binary")?;
  let config = Config::read(&path).map_err(|why| format!("the configuration could not be read: {why}"))?;
  BrainClient::new(&config.base_address(), config.token)
    .map_err(|why| format!("the brain's address did not read: {why}"))
}

/// Runs one noun's command and prints its lines.
async fn noun<F, Fut>(config: Option<&PathBuf>, run: F) -> Result<(), String>
where
  F: FnOnce(BrainClient) -> Fut,
  Fut: std::future::Future<Output = Result<Vec<String>, String>>,
{
  let client = command_client(config)?;
  for line in run(client).await? {
    println!("{line}");
  }
  Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
  let Command {
    config,
    via: _,
    verb,
  } = Command::parse();
  let ended = match verb {
    Verb::Serve => match configured(config.as_ref()) {
      Ok(config) => serve(&config).await,
      Err(why) => Err(why),
    },
    Verb::Health => match configured(config.as_ref()) {
      Ok(config) => health(&config).await,
      Err(why) => Err(why),
    },
    Verb::Mcp { brain, token } => mcp(&brain, token).await,
    Verb::Openapi => {
      print!("{}", openapi_json());
      Ok(())
    }
    Verb::Nouns => {
      NOUNS.iter().for_each(|noun| println!("{noun}"));
      Ok(())
    }
    Verb::ToolEnv { data_root } => match configured(config.as_ref()) {
      Ok(config) => match data_root.or_else(|| config.data_root.clone()) {
        Some(root) => bmo_brain::tool_env::write_env(&config, &root).map(|written| match written {
          bmo_brain::tool_env::Written::Written(path) => println!("written {}", path.display()),
          bmo_brain::tool_env::Written::Unchanged(path) => println!("unchanged {}", path.display()),
        }),
        None => Err("--data-root names no data root, and the configuration names none".to_owned()),
      },
      Err(why) => Err(why),
    },
    Verb::Sheets {
      word: SheetsWord::Split { dry_run },
    } => match configured(config.as_ref()) {
      Ok(config) => sheets_split(&config, dry_run).await,
      Err(why) => Err(why),
    },
    Verb::Memory { word } => {
      noun(config.as_ref(), |client| async move { command::memory::run(&client, word).await }).await
    }
    Verb::Fragment { word } => {
      noun(config.as_ref(), |client| async move { command::fragment::run(&client, word).await }).await
    }
  };
  match ended {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("bmo-brain: {why}");
      ExitCode::FAILURE
    }
  }
}
