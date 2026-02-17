pub mod account;
pub mod forget;
pub mod graph;
pub mod init;
pub mod init_auto;
pub mod init_tui;
pub mod login;
pub mod logout;
pub mod memories;
pub mod recall;
pub mod reindex;
pub mod remember;
pub mod status;
pub mod update;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ctxovrflw", about = "Universal AI context layer. One memory, every tool.")]
#[command(version, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// First-time setup — detect tools, download models, configure integrations
    Init {
        /// Non-interactive mode: accept all defaults, configure all detected tools
        #[arg(short = 'y', long = "yes", alias = "non-interactive")]
        non_interactive: bool,
    },

    /// Start the ctxovrflw daemon (MCP server + HTTP API)
    Start {
        /// HTTP port for REST API (default: 7437)
        #[arg(short, long, default_value = "7437")]
        port: u16,

        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Show daemon status, memory count, connected tools
    Status,

    /// Store a memory
    Remember {
        /// The content to remember
        text: String,

        /// Memory type: semantic, episodic, procedural, preference
        #[arg(short = 'T', long, alias = "type")]
        r#type: Option<String>,

        /// Tags (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        tags: Vec<String>,

        /// Subject entity (e.g., "user", "project:myapp", "person:sarah")
        #[arg(short, long)]
        subject: Option<String>,
    },

    /// Semantic search across all memories
    Recall {
        /// Search query
        query: String,

        /// Max results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Delete a memory
    Forget {
        /// Memory ID to delete
        id: String,

        /// Show what would be deleted without deleting
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Browse, search, and manage memories in an interactive TUI
    Memories,

    /// Knowledge graph commands
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },

    /// Rebuild embeddings for all memories (fixes missing semantic search results)
    Reindex,

    /// Sync memories to cloud
    Sync,

    /// Show cloud account status, tier, usage
    Account,

    /// Authenticate for cloud features
    Login {
        /// Authenticate directly with an API key
        #[arg(long)]
        key: Option<String>,
    },

    /// Log out and disable cloud sync
    Logout,

    /// Check for updates and self-update the binary
    Update {
        /// Just check for updates without installing
        #[arg(long)]
        check: bool,
    },

    /// Show current version and check for updates
    Version,

    /// Manage the ctxovrflw systemd service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Run as MCP server (stdio transport) — used by Cursor/Claude Desktop
    #[command(hide = true)]
    Mcp,
}

#[derive(Subcommand)]
pub enum GraphAction {
    /// Build knowledge graph from existing memories (extracts entities from subjects and tags)
    Build,
    /// Show graph statistics
    Stats,
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install ctxovrflw as a systemd user service
    Install,
    /// Remove the systemd service
    Uninstall,
    /// Show service status
    Status,
}
