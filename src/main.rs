mod capability;
mod cli;
mod config;
mod crypto;
mod daemon;
mod db;
mod embed;
mod http;
mod mcp;
mod sync;
#[cfg(feature = "pro")]
mod webhooks;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // In MCP mode, stdout is the JSON-RPC transport — no logging to stdout/stderr
    // to avoid corrupting the protocol stream
    if !matches!(cli.command, Command::Mcp) {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "ctxovrflw=info".into()),
            )
            .init();
    }

    let cfg = config::Config::load()?;

    match cli.command {
        Command::Init { non_interactive } => {
            if non_interactive {
                cli::init_auto::run(&cfg).await
            } else if atty::is(atty::Stream::Stdout) {
                cli::init_tui::run(&cfg).await
            } else {
                cli::init::run(&cfg).await
            }
        }
        Command::Start { port, foreground } => daemon::start(&cfg, port, foreground).await,
        Command::Stop => daemon::stop(&cfg).await,
        Command::Status => cli::status::run(&cfg).await,
        Command::Remember { text, r#type, tags, subject } => {
            cli::remember::run(&cfg, &text, r#type.as_deref(), tags, subject.as_deref()).await
        }
        Command::Recall { query, limit } => cli::recall::run(&cfg, &query, limit).await,
        Command::Forget { id, dry_run } => cli::forget::run(&cfg, &id, dry_run).await,
        Command::Memories => cli::memories::run(&cfg).await,
        Command::Reindex => {
            cli::reindex::run()?;
            Ok(())
        }
        Command::Sync => sync::run(&cfg).await,
        Command::Account => cli::account::run(&cfg).await,
        Command::Login { key } => {
            match key {
                Some(k) => cli::login::run_with_key(&cfg, &k).await,
                None => cli::login::run(&cfg).await,
            }
        }
        Command::Logout => cli::logout::run(&cfg).await,
        Command::Service { action } => {
            match action {
                cli::ServiceAction::Install => daemon::service_install(),
                cli::ServiceAction::Uninstall => daemon::service_uninstall(),
                cli::ServiceAction::Status => {
                    if daemon::is_service_installed() {
                        let running = daemon::is_service_running();
                        println!("Service: installed");
                        println!("Status:  {}", if running { "running ✓" } else { "stopped" });
                        if running {
                            println!("Logs:    journalctl --user -u ctxovrflw -f");
                        }
                    } else {
                        println!("Service: not installed");
                        println!("Install: ctxovrflw service install");
                    }
                    Ok(())
                }
            }
        }
        Command::Update { check } => cli::update::run(check).await,
        Command::Version => cli::update::version().await,
        Command::Mcp => mcp::serve_stdio(&cfg).await,
    }
}
