use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

/// ShellBe - A comprehensive SSH management tool with plugin support
#[derive(Parser)]
#[command(name = "shellbe")]
#[command(author = "Arash")]
#[command(version = "2.0.0")]
#[command(about = "SSH management tool with plugin support", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Supported commands
#[derive(Subcommand)]
pub enum Commands {
    /// Add a new SSH connection profile
    Add(AddArgs),

    /// List all configured SSH profiles
    List,

    /// Connect to a saved profile
    Connect {
        /// Profile name or alias
        name: String,
    },

    /// Copy SSH key to a remote server
    #[command(name = "copy-id")]
    CopyId {
        /// Profile name or alias
        name: String,

        /// Path to the identity file (public key)
        #[arg(long, short)]
        identity: Option<PathBuf>,
    },

    /// Generate a new SSH key pair
    #[command(name = "generate-key")]
    GenerateKey {
        /// Key name (default: id_rsa)
        #[arg(default_value = "id_rsa")]
        name: String,

        /// Key comment (e.g., email)
        #[arg(long, short)]
        comment: Option<String>,
    },

    /// Create an alias for a connection
    Alias(AliasArgs),

    /// List all connection aliases
    Aliases,

    /// Remove a profile
    Remove {
        /// Profile name
        name: String,
    },

    /// Edit a profile
    Edit {
        /// Profile name
        name: String,
    },

    /// Test connection to a profile
    Test {
        /// Profile name or alias
        name: String,
    },

    /// Show connection history
    History {
        /// Number of entries to show
        #[arg(default_value = "10")]
        limit: usize,
    },

    /// Export profiles to SSH config
    Export {
        /// Replace existing SSH config
        #[arg(long, short)]
        replace: bool,
    },

    /// Import profiles from SSH config
    Import {
        /// Replace existing profiles
        #[arg(long, short)]
        replace: bool,
    },

    /// Plugin management commands
    Plugin(PluginArgs),
}

/// Arguments for the 'add' command
#[derive(Args)]
pub struct AddArgs {
    /// Profile name
    #[arg(long, short)]
    pub name: Option<String>,

    /// Hostname or IP address
    #[arg(long, short)]
    pub host: Option<String>,

    /// Username
    #[arg(long, short)]
    pub user: Option<String>,

    /// SSH port
    #[arg(long, short, default_value = "22")]
    pub port: u16,

    /// Path to identity file
    #[arg(long, short)]
    pub identity: Option<PathBuf>,

    /// Additional SSH options (key=value pairs)
    #[arg(long, short)]
    pub options: Vec<String>,

    /// Non-interactive mode
    #[arg(long, short)]
    pub non_interactive: bool,
}

/// Arguments for the 'alias' command
#[derive(Args)]
pub struct AliasArgs {
    /// Alias name
    pub name: String,

    /// Target profile name
    pub profile: String,

    /// Create shell alias in rc file
    #[arg(long, short)]
    pub shell_alias: bool,
}

/// Arguments for the 'plugin' command
#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommands,
}

/// Plugin subcommands
#[derive(Subcommand)]
pub enum PluginCommands {
    /// List all installed plugins
    List,

    /// List plugins available for download
    Available,

    /// Install plugin from GitHub URL
    Install {
        /// GitHub URL (username/repo or full URL)
        url: String,
    },

    /// Update an installed plugin
    Update {
        /// Plugin name
        name: String,
    },

    /// Remove an installed plugin
    Remove {
        /// Plugin name
        name: String,
    },

    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },

    /// Run a specific plugin command
    Run {
        /// Plugin name
        name: String,

        /// Command name
        command: String,

        /// Command arguments
        args: Vec<String>,
    },
}