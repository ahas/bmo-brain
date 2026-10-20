//! The `bmo-brain` command (docs/boundary-api.md, 「Configuration」 and 「The harness plugin」): `serve` runs the
//! migrations, stands the clock and answers the routes until it is told to stop; `health` ends 0 only where the
//! service its configuration names answers ready; `mcp` serves the harness tools over MCP on standard input and
//! output; `openapi` prints the OpenAPI document.

use std::path::PathBuf;
use std::process::ExitCode;

use bmo_brain::api::{Brain, router};
use bmo_brain::clock;
use bmo_brain::config::{Config, HEALTH_PATIENCE};
use bmo_brain::mcp::BrainServer;
use bmo_brain::openapi::openapi_json;
use bmo_brain::store::Store;
use bmo_brain_client::BrainClient;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[derive(Parser)]
#[command(name = "bmo-brain", version, about = "The brain service and its harness plugin.")]
struct Command {
  /// The configuration file, which serving and the health ask read.
  #[arg(long, global = true)]
  config: Option<PathBuf>,
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

#[tokio::main]
async fn main() -> ExitCode {
  let Command { config, verb } = Command::parse();
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
  };
  match ended {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("bmo-brain: {why}");
      ExitCode::FAILURE
    }
  }
}
