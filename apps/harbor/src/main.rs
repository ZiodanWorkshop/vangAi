//! Harbor CLI — entrypoint for the agent harness.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use harbor_core::{CompletionRequest, IdError, Message, SessionId, WorkspaceId};
use harbor_infer::{Catalog, Inference};
use harbor_sandbox_linux::platform_info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "harbor",
    version,
    about = "Harbor agent harness (Linux-focused)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show platform / sandbox capabilities.
    Info,
    /// Send one user message to the catalog's provider chain.
    Run {
        /// User message.
        prompt: String,
        /// Catalog path. Defaults to `harbor.toml` in the working directory.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match Cli::parse().command {
        Command::Info => info(),
        Command::Run { prompt, config } => run(prompt, config).await,
    }
}

fn info() -> ExitCode {
    let ws = match require_id("workspace", WorkspaceId::new()) {
        Ok(id) => id,
        Err(code) => return code,
    };
    let sess = match require_id("session", SessionId::new()) {
        Ok(id) => id,
        Err(code) => return code,
    };
    let plat = platform_info();
    println!("harbor 0.1.0");
    println!("workspace: {ws}");
    println!("session: {sess}");
    println!(
        "platform: {} (landlock_supported={})",
        plat.name, plat.landlock_supported
    );
    ExitCode::SUCCESS
}

fn require_id<T>(label: &str, generated: Result<T, IdError>) -> Result<T, ExitCode> {
    generated.map_err(|err| {
        eprintln!("{label}: {err}");
        ExitCode::from(1)
    })
}

async fn run(prompt: String, config: Option<PathBuf>) -> ExitCode {
    if prompt.trim().is_empty() {
        eprintln!("prompt is empty");
        return ExitCode::from(1);
    }
    let path = config.unwrap_or_else(|| PathBuf::from("harbor.toml"));
    let catalog = match Catalog::load(&path) {
        Ok(catalog) => catalog,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    let chain = match catalog.open_ai_chain() {
        Ok(chain) => chain,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    let model = chain
        .steps()
        .first()
        .map(harbor_infer::OpenAiChatClient::model_id)
        .unwrap_or_default();
    let request = CompletionRequest {
        model,
        messages: vec![Message::user(prompt)],
    };
    match chain.complete(&request).await {
        Ok(completion) => {
            println!("{}", completion.text);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
