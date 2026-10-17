//! The brain service's binary (docs/boundary-api.md, 「Configuration」 and 「Runs launched apart」): `serve` runs the
//! migrations, stands the brain's clock and answers the routes on the configured address until it is told to stop;
//! the run words do one piece of work the brain launched apart, under the unit `--unit` names, and end with its exit
//! code.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use bmo_brain::api::router;
use bmo_brain::commands::{self, PeopleRun};
use bmo_brain::config::Config;
use bmo_brain::migration::migrate;
use bmo_brain::schedules::clock;
use bmo_brain::schedules::launches::Launches;
use bmo_brain::watch_runner::{IDLE_SLICE, WatchRun, watch};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
  name = "bmo-brain",
  version,
  about = "The brain service: memory rows, brain documents and their recollectors."
)]
struct Cli {
  /// The configuration file.
  #[arg(long)]
  config: PathBuf,
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// The migrations run, then the routes answered until the process is told to stop.
  Serve,
  /// The memo digest of every room's pending notes, or one room's.
  Digest {
    #[arg(long, allow_negative_numbers = true)]
    chat: Option<i64>,
    /// The unit this run was launched under.
    #[arg(long)]
    unit: Option<String>,
  },
  /// The person book, in the form its words name.
  PeopleDigest {
    #[command(flatten)]
    run: PeopleRun,
    /// The unit this run was launched under.
    #[arg(long)]
    unit: Option<String>,
  },
  /// One watch watched by the run holding it, until its check succeeds, its wait runs out or it stops being this
  /// run's.
  WatchRunner {
    /// The watch.
    id: i64,
    /// The unit whose run holds the watch.
    run: String,
    /// The unit this run was launched under.
    #[arg(long)]
    unit: Option<String>,
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
  migrate(&config.database).await.map_err(|why| format!("the migrations: {why}"))?;

  let db = config.connect().await.map_err(|why| format!("the database: {why}"))?;
  let api = Arc::new(config.api(db));
  let listener = tokio::net::TcpListener::bind(&config.listen)
    .await
    .map_err(|why| format!("{}: {why}", config.listen))?;

  println!("bmo-brain: answering on {}", config.listen);

  let clock = clock::stand(api.clone());
  let served = axum::serve(listener, router(api))
    .with_graceful_shutdown(stopped())
    .await
    .map_err(|why| why.to_string());

  for pass in clock {
    pass.abort();
  }

  served
}

async fn run(config: &Config, command: Command) -> Result<u8, String> {
  let db = config.connect().await.map_err(|why| format!("the database: {why}"))?;
  let api = config.api(db);

  match command {
    Command::Serve => unreachable!("serving is no run"),
    Command::Digest { chat, unit } => commands::memo(api.ground_under(unit.as_deref()), chat)
      .await
      .map_err(|why| why.to_string()),
    Command::PeopleDigest { run, unit } => {
      let launches = Launches {
        ground: api.ground_under(unit.as_deref()),
        program: &api.program,
      };

      commands::people(&launches, &run).await.map(u8::from).map_err(|why| why.to_string())
    }
    Command::WatchRunner { id, run, unit: _ } => {
      let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
      let watched = WatchRun {
        db: &api.db,
        agency: api.agency.as_ref(),
        receiver: api.receiver.as_ref(),
        cwd: &cwd,
        slice: IDLE_SLICE,
      };
      let ending = watch(&watched, id, &run)
        .await
        .map_err(|why| format!("watch #{id} run failed: {why}"))?;

      println!("watch #{id}: {ending}");

      Ok(0)
    }
  }
}

#[tokio::main]
async fn main() -> ExitCode {
  let cli = Cli::parse();
  let config = match Config::read(&cli.config) {
    Ok(config) => config,
    Err(why) => {
      eprintln!("bmo-brain: the configuration could not be read: {why}");
      return ExitCode::from(2);
    }
  };
  let ended = match cli.command {
    Command::Serve => serve(&config).await.map(|()| 0),
    command => run(&config, command).await,
  };

  match ended {
    Ok(code) => ExitCode::from(code),
    Err(why) => {
      eprintln!("bmo-brain: {why}");
      ExitCode::from(1)
    }
  }
}
