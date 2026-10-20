//! The `bmo-brain` command (docs/boundary-api.md, 「Configuration」, 「The command」 and 「The harness plugin」):
//! `serve` runs the migrations, stands the clock and answers the routes until it is told to stop; `health`
//! ends 0 only where the service its configuration names answers ready; `mcp` serves the harness tools over
//! MCP on standard input and output; `openapi` prints the OpenAPI document; `nouns` declares the nouns `bmo`
//! dispatches to this binary, and each noun asks the brain's own routes.

use std::path::PathBuf;
use std::process::ExitCode;

use bmo_brain::api::{Brain, router};
use bmo_brain::clock;
use bmo_brain::command::{self, NOUNS};
use bmo_brain::config::{self, Config, HEALTH_PATIENCE};
use bmo_brain::mcp::BrainServer;
use bmo_brain::openapi::openapi_json;
use bmo_brain::store::Store;
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
  /// The memory rows.
  Memory {
    #[command(subcommand)]
    word: command::memory::Word,
  },
  /// A note kept for a later reading.
  Note(command::note::Asked),
  /// The fragment table and the libraries it is synced from.
  Fragment {
    #[command(subcommand)]
    word: command::fragment::Word,
  },
  /// The watches.
  Watch {
    #[command(subcommand)]
    word: command::watch::Word,
  },
  /// The reminders.
  Reminder {
    #[command(subcommand)]
    word: command::reminder::Word,
  },
  /// The locks that say who is editing the store.
  Brain {
    #[command(subcommand)]
    word: command::lock::Word,
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
  let store = Store::lazy(&config.database)?;
  store.migrate().await.map_err(|why| format!("the migrations: {why}"))?;
  let listener = tokio::net::TcpListener::bind(&config.listen)
    .await
    .map_err(|why| format!("{}: {why}", config.listen))?;
  println!("bmo-brain: answering on {}", config.listen);
  let clock = clock::stand(store.clone());
  let brain = Brain {
    store,
    token: config.token.clone(),
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
    Verb::Memory { word } => {
      noun(config.as_ref(), |client| async move { command::memory::run(&client, word).await }).await
    }
    Verb::Note(asked) => {
      noun(config.as_ref(), |client| async move { command::note::run(&client, asked).await }).await
    }
    Verb::Fragment { word } => {
      noun(config.as_ref(), |client| async move { command::fragment::run(&client, word).await }).await
    }
    Verb::Watch { word } => {
      noun(config.as_ref(), |client| async move { command::watch::run(&client, word).await }).await
    }
    Verb::Reminder { word } => {
      noun(config.as_ref(), |client| async move { command::reminder::run(&client, word).await }).await
    }
    Verb::Brain { word } => {
      noun(config.as_ref(), |client| async move { command::lock::run(&client, word).await }).await
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
