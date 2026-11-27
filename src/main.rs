//! The `bmo-brain` command (docs/boundary-api.md, 「Configuration」 and 「The harness plugin」): `serve` runs
//! the migrations, reads the vectors in, stands the clock and the embedder and answers the routes until it is
//! told to stop; `embed` embeds every live row lacking a vector of the configuration's model; `health` ends 0 only
//! where the service its configuration names answers ready; `mcp` serves the harness tools over MCP on
//! standard input and output; `openapi` prints the OpenAPI document; `sheets split` splits the live sheet rows
//! of the configuration's database into facts; `chain declare` declares the brain's kinds of the delegation
//! chain; `base candidates` raises the candidates of the base from a source brain.

use std::path::PathBuf;
use std::process::ExitCode;

use bmo_brain::api::console::{Console, bundle_beside_binary};
use bmo_brain::api::{Brain, Stopping, router};
use bmo_brain::clock;
use bmo_brain::config::Config;
use bmo_brain::mcp::BrainServer;
use bmo_brain::openapi::openapi_json;
use bmo_brain::store::Store;
use bmo_brain::store::sheets::SheetSplit;
use bmo_brain_client::BrainClient;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[derive(Parser)]
#[command(name = "bmo-brain", version = bmo_brain::VERSION, about = "The brain service and its command.")]
struct Command {
  /// The configuration file; `BMO_BRAIN_CONFIG` where none is given.
  #[arg(long, global = true, env = "BMO_BRAIN_CONFIG")]
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
  /// Writes the env file the brain's tool reads the service's address and token from, under the data root.
  ToolEnv {
    /// The data root; the configuration's where none is given.
    #[arg(long, value_name = "dir")]
    data_root: Option<PathBuf>,
  },
  /// Embeds every live row lacking a vector of the configuration's model, and deletes the other models' vectors
  /// once none lacks one.
  Embed,
  /// The live sheet rows of the configuration's database, split into facts.
  Sheets {
    #[command(subcommand)]
    word: SheetsWord,
  },
  /// The brain's kinds of the delegation chain.
  Chain {
    #[command(subcommand)]
    word: ChainWord,
  },
  /// The base every agent may read.
  Base {
    #[command(subcommand)]
    word: BaseWord,
  },
  /// A holder's standing set, declared from a file or withdrawn.
  Holder {
    #[command(subcommand)]
    word: HolderWord,
  },
}

#[derive(Subcommand)]
enum HolderWord {
  /// Stands the holder's set the TOML file declares.
  Declare { file: PathBuf },
  /// Withdraws the holder from every brain.
  Withdraw { key: String },
}

#[derive(Subcommand)]
enum ChainWord {
  /// Declares whether the base and the org brains are read, on the account service.
  Declare,
}

#[derive(Subcommand)]
enum BaseWord {
  /// Raises the candidates of the base from a source brain: sifted by subject, judged by the classifier and
  /// written as pending rows; prints ids and counts alone.
  Candidates {
    /// The source brain.
    #[arg(long, value_name = "brainId")]
    from: i64,
    /// The subject types a memory is passed over for, beside a person and a room; comma-separated.
    #[arg(long, value_name = "types", value_delimiter = ',')]
    pass_over_types: Vec<String>,
    /// The beginnings of subject keys a memory is passed over for, case set aside; comma-separated.
    #[arg(long, value_name = "prefixes", value_delimiter = ',')]
    pass_over_keys: Vec<String>,
    /// Reads at most this many memories of the source, the oldest.
    #[arg(long)]
    limit: Option<usize>,
    /// Judges and prints the same, and writes nothing.
    #[arg(long)]
    dry_run: bool,
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
  let store = Store::lazy(&config.database)?
    .by(config.rules.clone())
    .embedding(config.embedding.clone());
  store.migrate().await.map_err(|why| format!("the migrations: {why}"))?;
  // The vectors of the current model read in, then the embedder standing: it first fills every live row lacking
  // one (docs/design.md, 「Vectors」).
  let vectorised = store.load_vectors().await.map_err(|why| format!("the vectors: {why}"))?;
  if store.vectors.configured() {
    println!("bmo-brain: {vectorised} vectors read in");
  }
  let embedder = bmo_brain::store::vectors::stand(store.clone());
  let listener = tokio::net::TcpListener::bind(&config.listen)
    .await
    .map_err(|why| format!("{}: {why}", config.listen))?;
  println!("bmo-brain: answering on {}", config.listen);
  let clock = clock::stand(store.clone());
  let console = config.console.as_ref().map(|console| {
    let folder = console.bundle.clone().or_else(bundle_beside_binary).unwrap_or_default();
    Console::over(console.token.clone(), &folder, console.calls.clone())
  });
  let (raise, stopping) = Stopping::new();
  let brain = Brain {
    store,
    token: config.token.clone(),
    console,
    stopping,
  };
  let served = axum::serve(listener, router(brain))
    .with_graceful_shutdown(async move {
      stopped().await;
      let _ = raise.send(true);
    })
    .await
    .map_err(|why| why.to_string());
  clock.abort();
  embedder.abort();
  served
}

/// Every live row lacking a vector of the configuration's model embedded, the other models' vectors deleted once
/// none lacks one; prints how many it embedded.
async fn embed(config: &Config) -> Result<(), String> {
  if config.embedding.is_none() {
    return Err("the configuration names no [embedding]".to_owned());
  }
  let store = Store::lazy(&config.database)?
    .by(config.rules.clone())
    .embedding(config.embedding.clone());
  let embedded = store.embed_backlog().await?;
  println!("embedded {embedded}");
  Ok(())
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

/// The kinds declared on the account service the environment or the data root names; nothing where none is
/// named.
async fn chain_declare(config: &Config) -> Result<(), String> {
  match bmo_brain::chain::account_service(config.data_root.as_deref())? {
    None => {
      println!("no account service is named; nothing declared");
      Ok(())
    }
    Some(service) => {
      for key in bmo_brain::chain::declare(&service).await? {
        println!("declared {key}");
      }
      Ok(())
    }
  }
}

/// The candidates of the base raised, or judged alone in a dry run, as lines of ids and counts.
async fn base_candidates(config: &Config, run: bmo_brain::base::Run) -> Result<(), String> {
  let store = Store::lazy(&config.database)?.by(config.rules.clone());
  let came = bmo_brain::base::raise(&store, config.classifier.as_ref(), &run).await?;
  for (source, written) in &came.candidates {
    let base = written.map_or_else(|| "-".to_owned(), |id| id.to_string());
    println!("candidate {source} base {base}");
  }
  let sifted = &came.sifted;
  println!(
    "considered {} person-or-room {} type {} key {} raised {} judged {} fits {} does-not-fit {} unjudged {} \
     written {}",
    sifted.considered,
    sifted.person_or_room,
    sifted.by_type,
    sifted.by_key,
    sifted.raised,
    came.judged,
    came.fits,
    came.does_not_fit,
    came.unjudged,
    came.written
  );
  for (kind, count) in &sifted.passed_over {
    println!("passed-over {kind} {count}");
  }
  for failure in &came.failures {
    eprintln!("bmo-brain: {failure}");
  }
  Ok(())
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
    Verb::Embed => match configured(config.as_ref()) {
      Ok(config) => embed(&config).await,
      Err(why) => Err(why),
    },
    Verb::Mcp { brain, token } => mcp(&brain, token).await,
    Verb::Holder { word } => {
      let outcome = match word {
        HolderWord::Declare { file } => bmo_brain::holder::declare(&file).await,
        HolderWord::Withdraw { key } => bmo_brain::holder::withdraw(&key).await,
      };

      println!("{}", outcome.line);

      return std::process::ExitCode::from(outcome.code);
    }
    Verb::Openapi => {
      print!("{}", openapi_json());
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
    Verb::Chain {
      word: ChainWord::Declare,
    } => match configured(config.as_ref()) {
      Ok(config) => chain_declare(&config).await,
      Err(why) => Err(why),
    },
    Verb::Base {
      word:
        BaseWord::Candidates {
          from,
          pass_over_types,
          pass_over_keys,
          limit,
          dry_run,
        },
    } => match configured(config.as_ref()) {
      Ok(config) => {
        let run = bmo_brain::base::Run {
          from,
          pass_over: bmo_brain::store::base::PassOver {
            types: pass_over_types,
            key_prefixes: pass_over_keys,
          },
          limit,
          dry_run,
        };
        base_candidates(&config, run).await
      }
      Err(why) => Err(why),
    },
  };
  match ended {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("bmo-brain: {why}");
      ExitCode::FAILURE
    }
  }
}
