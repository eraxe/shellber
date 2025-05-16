use crate::application::{
    ProfileService, ConnectionService, AliasService,
    PluginService, SshConfigService, PluginError, UpdateService
};
use crate::domain::{Profile, Alias, DomainError};
use crate::interface::cli::commands::{Commands, AddArgs, AliasArgs, PluginCommands};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use dialoguer::{Input, Select, Confirm};
use console::{style, Term};

pub struct CommandHandler {
    profile_service: Arc<ProfileService>,
    connection_service: Arc<ConnectionService>,
    alias_service: Arc<AliasService>,
    plugin_service: Arc<PluginService>,
    ssh_config_service: Arc<SshConfigService>,
    update_service: UpdateService,
}

impl CommandHandler {
    /// Create a new command handler with the provided services
    pub fn new(
        profile_service: Arc<ProfileService>,
        connection_service: Arc<ConnectionService>,
        alias_service: Arc<AliasService>,
        plugin_service: Arc<PluginService>,
        ssh_config_service: Arc<SshConfigService>,
    ) -> Self {
        Self {
            profile_service,
            connection_service,
            alias_service,
            plugin_service,
            ssh_config_service,
            update_service: UpdateService::new(),
        }
    }

    /// Handle a CLI command
    pub async fn handle_command(&self, command: Commands) -> anyhow::Result<()> {
        match command {
            Commands::Add(args) => self.handle_add(args).await?,
            Commands::List => self.handle_list().await?,
            Commands::Connect { name } => self.handle_connect(name).await?,
            Commands::CopyId { name, identity } => self.handle_copy_id(name, identity).await?,
            Commands::GenerateKey { name, comment } => self.handle_generate_key(name, comment).await?,
            Commands::Alias(args) => self.handle_alias(args).await?,
            Commands::Aliases => self.handle_aliases().await?,
            Commands::Remove { name } => self.handle_remove(name).await?,
            Commands::Edit { name } => self.handle_edit(name).await?,
            Commands::Test { name } => self.handle_test(name).await?,
            Commands::History { limit } => self.handle_history(limit).await?,
            Commands::Export { replace } => self.handle_export(replace).await?,
            Commands::Import { replace } => self.handle_import(replace).await?,
            Commands::Plugin(args) => self.handle_plugin(args).await?,
            Commands::Update { check } => self.handle_update(check).await?,
        }

        Ok(())
    }
    /// Handle the 'update' command
    async fn handle_update(&self, check_only: bool) -> anyhow::Result<()> {
        println!("{} Checking for updates...", style("→").cyan().bold());

        match self.update_service.check_for_update() {
            Ok(Some(version)) => {
                println!("{} A new version {} is available (current: {})",
                         style("✓").green().bold(),
                         style(&version).green(),
                         style(crate::application::update_service::CURRENT_VERSION).yellow());

                if !check_only {
                    // Ask for confirmation
                    let confirm = Confirm::new()
                        .with_prompt("Do you want to update now?")
                        .default(true)
                        .interact()?;

                    if confirm {
                        // Backup the executable
                        match self.update_service.backup_executable() {
                            Ok(path) => {
                                println!("{} Created backup at {}",
                                         style("✓").green().bold(),
                                         path.display());
                            },
                            Err(e) => {
                                println!("{} Failed to create backup: {}",
                                         style("!").yellow().bold(), e);

                                // Ask to continue without backup
                                let continue_anyway = Confirm::new()
                                    .with_prompt("Continue without backup?")
                                    .default(false)
                                    .interact()?;

                                if !continue_anyway {
                                    println!("{} Update cancelled", style("!").yellow().bold());
                                    return Ok(());
                                }
                            }
                        }

                        // Perform the update
                        match self.update_service.update() {
                            Ok(_) => {
                                println!("{} Successfully updated to {}!",
                                         style("✓").green().bold(),
                                         style(&version).green());
                            },
                            Err(e) => {
                                println!("{} Update failed: {}",
                                         style("✗").red().bold(), e);
                            }
                        }
                    } else {
                        println!("{} Update cancelled", style("!").yellow().bold());
                    }
                }
            },
            Ok(None) => {
                println!("{} You are already using the latest version ({})",
                         style("✓").green().bold(),
                         style(crate::application::update_service::CURRENT_VERSION).green());
            },
            Err(e) => {
                println!("{} Failed to check for updates: {}",
                         style("✗").red().bold(), e);
            }
        }

        Ok(())
    }
    async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.loaded_plugins.write().await;
        let idx = plugins.iter().position(|(n, _, _)| n == name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        // Remove the plugin
        plugins.remove(idx);

        Ok(())
    }
}

// Helper functions

/// Parse a GitHub URL into owner and repo
    /// Handle the 'add' command
    async fn handle_add(&self, args: AddArgs) -> anyhow::Result<()> {
        println!("{}", style("Adding a new SSH profile...").cyan().bold());

        // Collect profile information
        let name = if let Some(name) = args.name {
            name
        } else if args.non_interactive {
            return Err(anyhow::anyhow!("Profile name is required in non-interactive mode"));
        } else {
            Input::<String>::new()
                .with_prompt("Enter profile name")
                .interact()?
        };

        let hostname = if let Some(host) = args.host {
            host
        } else if args.non_interactive {
            return Err(anyhow::anyhow!("Hostname is required in non-interactive mode"));
        } else {
            Input::<String>::new()
                .with_prompt("Enter hostname or IP address")
                .interact()?
        };

        let username = if let Some(user) = args.user {
            user
        } else if args.non_interactive {
            return Err(anyhow::anyhow!("Username is required in non-interactive mode"));
        } else {
            Input::<String>::new()
                .with_prompt("Enter username")
                .interact()?
        };

        let port = if args.non_interactive {
            args.port
        } else {
            Input::<u16>::new()
                .with_prompt("Enter port")
                .default(args.port)
                .interact()?
        };

        let identity_file = if let Some(identity) = args.identity {
            Some(identity)
        } else if !args.non_interactive {
            let use_identity = Confirm::new()
                .with_prompt("Use identity file?")
                .default(false)
                .interact()?;

            if use_identity {
                Some(Input::<PathBuf>::new()
                    .with_prompt("Enter identity file path")
                    .interact()?)
            } else {
                None
            }
        } else {
            None
        };

        // Create a new profile
        let mut profile = Profile::new(name, hostname, username);
        profile.port = port;

        if let Some(identity) = identity_file {
            profile.identity_file = Some(identity);
        }

        // Parse options
        for option in args.options {
            if let Some(idx) = option.find('=') {
                let key = option[..idx].to_string();
                let value = option[idx+1..].to_string();
                profile.options.insert(key, value);
            } else {
                profile.options.insert(option, "".to_string());
            }
        }

        // Add the profile
        match self.profile_service.add_profile(profile.clone()).await {
            Ok(_) => {
                println!("{} Profile '{}' added successfully!", style("✓").green().bold(), profile.name);

                // Ask if user wants to add to SSH config
                if !args.non_interactive {
                    let add_to_ssh_config = Confirm::new()
                        .with_prompt("Add this profile to SSH config?")
                        .default(false)
                        .interact()?;

                    if add_to_ssh_config {
                        match self.ssh_config_service.add_profile_to_ssh_config(&profile).await {
                            Ok(_) => println!("{} Profile added to SSH config", style("✓").green().bold()),
                            Err(e) => println!("{} Failed to add profile to SSH config: {}", style("✗").red().bold(), e),
                        }
                    }

                    let copy_key = Confirm::new()
                        .with_prompt("Copy SSH key to this server?")
                        .default(false)
                        .interact()?;

                    if copy_key {
                        let key_path = if let Some(identity) = profile.identity_file {
                            // Use the specified identity file
                            identity
                        } else {
                            // Use default identity file
                            dirs::home_dir()
                                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                                .join(".ssh")
                                .join("id_rsa.pub")
                        };

                        match self.connection_service.copy_ssh_key(&profile.name, &key_path).await {
                            Ok(_) => println!("{} SSH key copied successfully", style("✓").green().bold()),
                            Err(e) => println!("{} Failed to copy SSH key: {}", style("✗").red().bold(), e),
                        }
                    }
                }
            },
            Err(e) => {
                println!("{} Failed to add profile: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'list' command
    async fn handle_list(&self) -> anyhow::Result<()> {
        println!("{}", style("Available SSH profiles:").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());
        println!("{:<15} {:<20} {:<15} {:<5}",
                 style("NAME").cyan().bold(),
                 style("HOST").cyan().bold(),
                 style("USER").cyan().bold(),
                 style("PORT").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());

        let profiles = self.profile_service.list_profiles().await?;

        if profiles.is_empty() {
            println!("{} No profiles found. Use 'add' command to create one.", style("!").yellow().bold());
            return Ok(());
        }

        for profile in profiles {
            println!("{:<15} {:<20} {:<15} {:<5}",
                     style(&profile.name).green(),
                     profile.hostname,
                     profile.username,
                     profile.port);
        }

        Ok(())
    }

    /// Handle the 'connect' command
    async fn handle_connect(&self, name: String) -> anyhow::Result<()> {
        // Resolve alias first
        let profile_name = match self.alias_service.resolve_alias(&name).await {
            Ok(resolved) => {
                if resolved != name {
                    println!("{} Connecting via alias '{}' -> '{}'", style("→").cyan().bold(), name, resolved);
                }
                resolved
            },
            Err(_) => name.clone(),
        };

        // Get the profile for display
        match self.profile_service.get_profile(&profile_name).await {
            Ok(profile) => {
                println!("{} Connecting to {} ({}@{})...",
                         style("→").green().bold(),
                         style(&profile.name).green(),
                         profile.username,
                         profile.hostname);

                // Connect to the profile
                match self.connection_service.connect(&name).await {
                    Ok(exit_code) => {
                        if exit_code == 0 {
                            println!("{} Connection closed successfully", style("✓").green().bold());
                        } else {
                            println!("{} Connection closed with exit code {}", style("!").yellow().bold(), exit_code);
                        }
                    },
                    Err(e) => {
                        println!("{} Connection failed: {}", style("✗").red().bold(), e);
                    },
                }
            },
            Err(e) => {
                println!("{} Profile not found: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'copy-id' command
    async fn handle_copy_id(&self, name: String, identity: Option<PathBuf>) -> anyhow::Result<()> {
        // Get the key path
        let key_path = if let Some(identity) = identity {
            identity
        } else {
            // Use default identity file
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                .join(".ssh")
                .join("id_rsa.pub")
        };

        // Check if key exists
        if !key_path.exists() {
            println!("{} Key file not found: {}", style("✗").red().bold(), key_path.display());

            // Ask if user wants to generate a key
            let generate_key = Confirm::new()
                .with_prompt("Generate a new SSH key?")
                .default(true)
                .interact()?;

            if generate_key {
                let key_name = key_path.file_stem()
                    .ok_or_else(|| anyhow::anyhow!("Invalid key file name"))?
                    .to_string_lossy();

                self.handle_generate_key(key_name.to_string(), None).await?;
            } else {
                return Ok(());
            }
        }

        println!("{} Copying SSH key {} to {}...",
                 style("→").cyan().bold(),
                 key_path.display(),
                 style(&name).green());

        match self.connection_service.copy_ssh_key(&name, &key_path).await {
            Ok(_) => {
                println!("{} SSH key copied successfully", style("✓").green().bold());
            },
            Err(e) => {
                println!("{} Failed to copy SSH key: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'generate-key' command
    async fn handle_generate_key(&self, name: String, comment: Option<String>) -> anyhow::Result<()> {
        println!("{} Generating a new SSH key pair...", style("→").cyan().bold());

        // Get or create SSH directory
        let ssh_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
            .join(".ssh");

        if !ssh_dir.exists() {
            std::fs::create_dir_all(&ssh_dir)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = std::fs::metadata(&ssh_dir)?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o700);
                std::fs::set_permissions(&ssh_dir, permissions)?;
            }
        }

        let ssh_service = crate::infrastructure::ThrushSshService::new();

        match ssh_service.generate_key(&name, comment.as_deref()).await {
            Ok((private_key, public_key)) => {
                println!("{} SSH key pair generated successfully:", style("✓").green().bold());
                println!("  Private key: {}", style(private_key.display()).cyan());
                println!("  Public key: {}", style(public_key.display()).cyan());
            },
            Err(e) => {
                println!("{} Failed to generate SSH key: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'alias' command
    async fn handle_alias(&self, args: AliasArgs) -> anyhow::Result<()> {
        // Create alias
        match self.alias_service.create_alias(&args.name, &args.profile).await {
            Ok(_) => {
                println!("{} Alias '{}' created for profile '{}'",
                         style("✓").green().bold(),
                         style(&args.name).green(),
                         style(&args.profile).green());

                // Create shell alias if requested
                if args.shell_alias {
                    self.create_shell_alias(&args.name, &args.profile)?;
                }
            },
            Err(e) => {
                println!("{} Failed to create alias: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Helper method to create a shell alias
    fn create_shell_alias(&self, alias_name: &str, profile_name: &str) -> anyhow::Result<()> {
        // Detect user's shell and corresponding rc file
        let shell_rc_file = if let Ok(shell) = std::env::var("SHELL") {
            if shell.contains("zsh") {
                dirs::home_dir().map(|h| h.join(".zshrc"))
            } else if shell.contains("bash") {
                dirs::home_dir().map(|h| h.join(".bashrc"))
            } else {
                dirs::home_dir().map(|h| h.join(".profile"))
            }
        } else {
            dirs::home_dir().map(|h| h.join(".bashrc"))
        };

        let shell_rc_file = shell_rc_file.ok_or_else(|| anyhow::anyhow!("Could not determine shell configuration file"))?;

        // Check if alias already exists
        let mut content = String::new();
        if shell_rc_file.exists() {
            content = std::fs::read_to_string(&shell_rc_file)?;
        }

        let alias_line = format!("alias {}='shellbe connect {}'", alias_name, profile_name);

        if content.contains(&alias_line) {
            println!("{} Shell alias '{}' already exists in {}",
                     style("!").yellow().bold(),
                     alias_name,
                     shell_rc_file.display());
            return Ok(());
        }

        // Add alias to shell config
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&shell_rc_file)?;

        writeln!(file, "\n# ShellBe alias added on {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(file, "{}", alias_line)?;

        println!("{} Shell alias '{}' added to {}",
                 style("✓").green().bold(),
                 alias_name,
                 shell_rc_file.display());
        println!("{} To use this alias, restart your shell or run: source {}",
                 style("!").yellow().bold(),
                 shell_rc_file.display());

        Ok(())
    }

    /// Handle the 'aliases' command
    async fn handle_aliases(&self) -> anyhow::Result<()> {
        println!("{}", style("Available connection aliases:").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());
        println!("{:<15} {:<15}",
                 style("ALIAS").cyan().bold(),
                 style("PROFILE").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());

        let aliases = self.alias_service.list_aliases().await?;

        if aliases.is_empty() {
            println!("{} No aliases found. Use 'alias' command to create one.", style("!").yellow().bold());
            return Ok(());
        }

        for alias in aliases {
            println!("{:<15} {:<15}",
                     style(&alias.name).green(),
                     alias.target);
        }

        Ok(())
    }

    /// Handle the 'remove' command
    async fn handle_remove(&self, name: String) -> anyhow::Result<()> {
        // Ask for confirmation
        let confirm = Confirm::new()
            .with_prompt(format!("Are you sure you want to remove profile '{}'?", name))
            .default(false)
            .interact()?;

        if !confirm {
            println!("{} Operation cancelled", style("!").yellow().bold());
            return Ok(());
        }

        // Remove profile
        match self.profile_service.remove_profile(&name).await {
            Ok(_) => {
                println!("{} Profile '{}' removed successfully", style("✓").green().bold(), name);

                // Ask if user wants to remove from SSH config
                let remove_from_ssh_config = Confirm::new()
                    .with_prompt("Remove this profile from SSH config?")
                    .default(false)
                    .interact()?;

                if remove_from_ssh_config {
                    match self.ssh_config_service.remove_profile_from_ssh_config(&name).await {
                        Ok(_) => println!("{} Profile removed from SSH config", style("✓").green().bold()),
                        Err(e) => println!("{} Failed to remove profile from SSH config: {}", style("✗").red().bold(), e),
                    }
                }

                // List and remove aliases pointing to this profile
                match self.alias_service.get_aliases_for_profile(&name).await {
                    Ok(aliases) => {
                        if !aliases.is_empty() {
                            println!("{} Found aliases pointing to this profile:", style("!").yellow().bold());

                            for alias in &aliases {
                                println!("  - {}", style(&alias.name).yellow());
                            }

                            let remove_aliases = Confirm::new()
                                .with_prompt("Remove these aliases?")
                                .default(true)
                                .interact()?;

                            if remove_aliases {
                                for alias in aliases {
                                    match self.alias_service.remove_alias(&alias.name).await {
                                        Ok(_) => println!("{} Removed alias '{}'", style("✓").green().bold(), alias.name),
                                        Err(e) => println!("{} Failed to remove alias '{}': {}", style("✗").red().bold(), alias.name, e),
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        println!("{} Error checking for aliases: {}", style("!").yellow().bold(), e);
                    },
                }
            },
            Err(e) => {
                println!("{} Failed to remove profile: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'edit' command
    async fn handle_edit(&self, name: String) -> anyhow::Result<()> {
        // Get the profile
        let profile = match self.profile_service.get_profile(&name).await {
            Ok(p) => p,
            Err(e) => {
                println!("{} Failed to get profile: {}", style("✗").red().bold(), e);
                return Ok(());
            }
        };

        println!("{} Editing profile '{}'", style("→").cyan().bold(), style(&profile.name).green());
        println!("{} (Press Enter to keep current value)", style("Tip").yellow().italic());

        // Edit each field
        let hostname = Input::<String>::new()
            .with_prompt("Hostname")
            .with_initial_text(&profile.hostname)
            .allow_empty(true)
            .interact()?;

        let username = Input::<String>::new()
            .with_prompt("Username")
            .with_initial_text(&profile.username)
            .allow_empty(true)
            .interact()?;

        let port = Input::<u16>::new()
            .with_prompt("Port")
            .with_initial_text(&profile.port.to_string())
            .allow_empty(true)
            .interact()?;

        let identity_file = Input::<String>::new()
            .with_prompt("Identity file")
            .with_initial_text(profile.identity_file.as_ref().map_or("", |p| p.to_str().unwrap_or("")))
            .allow_empty(true)
            .interact()?;

        // Create updated profile
        let mut updated_profile = profile.clone();

        if !hostname.is_empty() {
            updated_profile.hostname = hostname;
        }

        if !username.is_empty() {
            updated_profile.username = username;
        }

        updated_profile.port = port;

        if !identity_file.is_empty() {
            updated_profile.identity_file = Some(PathBuf::from(identity_file));
        } else {
            updated_profile.identity_file = None;
        }

        // Update options
        let update_options = Confirm::new()
            .with_prompt("Update SSH options?")
            .default(false)
            .interact()?;

        if update_options {
            // Show current options
            if !updated_profile.options.is_empty() {
                println!("{} Current options:", style("→").cyan());
                for (key, value) in &updated_profile.options {
                    println!("  {} = {}", key, value);
                }
            }

            // Clear or add options
            let clear_options = Confirm::new()
                .with_prompt("Clear all options?")
                .default(false)
                .interact()?;

            if clear_options {
                updated_profile.options.clear();
            }

            let add_options = Confirm::new()
                .with_prompt("Add new options?")
                .default(true)
                .interact()?;

            if add_options {
                loop {
                    let key = Input::<String>::new()
                        .with_prompt("Option key (empty to finish)")
                        .allow_empty(true)
                        .interact()?;

                    if key.is_empty() {
                        break;
                    }

                    let value = Input::<String>::new()
                        .with_prompt("Option value")
                        .allow_empty(true)
                        .interact()?;

                    updated_profile.options.insert(key, value);
                }
            }
        }

        // Update the profile
        match self.profile_service.update_profile(updated_profile.clone()).await {
            Ok(_) => {
                println!("{} Profile '{}' updated successfully", style("✓").green().bold(), name);

                // Ask if user wants to update SSH config
                let update_ssh_config = Confirm::new()
                    .with_prompt("Update this profile in SSH config?")
                    .default(false)
                    .interact()?;

                if update_ssh_config {
                    match self.ssh_config_service.remove_profile_from_ssh_config(&name).await {
                        Ok(_) => {
                            match self.ssh_config_service.add_profile_to_ssh_config(&updated_profile).await {
                                Ok(_) => println!("{} Profile updated in SSH config", style("✓").green().bold()),
                                Err(e) => println!("{} Failed to update profile in SSH config: {}", style("✗").red().bold(), e),
                            }
                        },
                        Err(e) => println!("{} Failed to remove profile from SSH config: {}", style("✗").red().bold(), e),
                    }
                }
            },
            Err(e) => {
                println!("{} Failed to update profile: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'test' command
    async fn handle_test(&self, name: String) -> anyhow::Result<()> {
        println!("{} Testing connection to {}...", style("→").cyan().bold(), style(&name).green());

        match self.connection_service.test_connection(&name).await {
            Ok(true) => {
                println!("{} Connection successful!", style("✓").green().bold());
            },
            Ok(false) => {
                println!("{} Connection failed!", style("✗").red().bold());
                println!("{} Troubleshooting tips:", style("!").yellow().bold());
                println!("  - Check if the server is running and accessible");
                println!("  - Verify your username and host are correct");
                println!("  - Make sure your SSH key is properly set up");
                println!("  - Check if the port is open and SSH is running on it");
            },
            Err(e) => {
                println!("{} Error testing connection: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'history' command
    async fn handle_history(&self, limit: usize) -> anyhow::Result<()> {
        println!("{}", style("Connection history:").cyan().bold());
        println!("{}", style("------------------------------------------").yellow());
        println!("{:<20} {:<8} {:<15} {:<15}",
                 style("DATE").cyan().bold(),
                 style("TIME").cyan().bold(),
                 style("PROFILE").cyan().bold(),
                 style("HOST").cyan().bold());
        println!("{}", style("------------------------------------------").yellow());

        let history = self.connection_service.get_recent_history(limit).await?;

        if history.is_empty() {
            println!("{} No connection history found.", style("!").yellow().bold());
            return Ok(());
        }

        for entry in history {
            let date = entry.timestamp.format("%Y-%m-%d").to_string();
            let time = entry.timestamp.format("%H:%M:%S").to_string();

            println!("{:<20} {:<8} {:<15} {:<15}",
                     date,
                     time,
                     style(&entry.profile_name).green(),
                     entry.hostname);
        }

        // Show stats
        println!("\n{}", style("Connection statistics:").cyan().bold());
        println!("{}", style("------------------------------------------").yellow());
        println!("{:<15} {:<10}",
                 style("PROFILE").cyan().bold(),
                 style("CONNECTIONS").cyan().bold());
        println!("{}", style("------------------------------------------").yellow());

        let stats = self.connection_service.get_connection_stats().await?;

        for (profile, count) in stats {
            println!("{:<15} {:<10}",
                     style(profile).green(),
                     count);
        }

        Ok(())
    }

    /// Handle the 'export' command
    async fn handle_export(&self, replace: bool) -> anyhow::Result<()> {
        println!("{} Exporting profiles to SSH config...", style("→").cyan().bold());

        // Get all profiles
        let profiles = self.profile_service.list_profiles().await?;

        if profiles.is_empty() {
            println!("{} No profiles found to export.", style("!").yellow().bold());
            return Ok(());
        }

        // Confirm export mode if not specified
        let replace = if replace {
            true
        } else {
            let options = vec!["Replace existing SSH config", "Append to existing SSH config"];
            let selection = Select::new()
                .with_prompt("Export mode")
                .items(&options)
                .default(1)  // Default to append
                .interact()?;

            selection == 0  // true if "Replace" was selected
        };

        // Export profiles
        match self.ssh_config_service.export_profiles(&profiles, replace).await {
            Ok(_) => {
                println!("{} Profiles successfully exported to SSH config", style("✓").green().bold());

                // Get SSH config path
                let ssh_config_path = dirs::home_dir()
                    .map(|h| h.join(".ssh").join("config"))
                    .unwrap_or_else(|| PathBuf::from("~/.ssh/config"));

                println!("{} SSH config location: {}", style("→").cyan(), ssh_config_path.display());
            },
            Err(e) => {
                println!("{} Failed to export profiles: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'import' command
    async fn handle_import(&self, replace: bool) -> anyhow::Result<()> {
        println!("{} Importing profiles from SSH config...", style("→").cyan().bold());

        // Confirm import mode if not specified
        let replace = if replace {
            true
        } else {
            let options = vec!["Replace existing profiles", "Append new profiles"];
            let selection = Select::new()
                .with_prompt("Import mode")
                .items(&options)
                .default(1)  // Default to append
                .interact()?;

            selection == 0  // true if "Replace" was selected
        };

        // Import profiles
        match self.ssh_config_service.import_profiles().await {
            Ok(profiles) => {
                if profiles.is_empty() {
                    println!("{} No profiles found to import.", style("!").yellow().bold());
                    return Ok(());
                }

                println!("{} Found {} profiles in SSH config", style("→").cyan(), profiles.len());

                // Display profiles to import
                for profile in &profiles {
                    println!("  - {}: {}@{}",
                             style(&profile.name).green(),
                             profile.username,
                             profile.hostname);
                }

                // Confirm import
                let confirm = Confirm::new()
                    .with_prompt(format!("Import {} profiles?", profiles.len()))
                    .default(true)
                    .interact()?;

                if !confirm {
                    println!("{} Import cancelled", style("!").yellow().bold());
                    return Ok(());
                }

                // Import each profile
                let mut imported = 0;
                let mut skipped = 0;

                for profile in profiles {
                    // Check if profile already exists
                    let exists = self.profile_service.get_profile(&profile.name).await.is_ok();

                    if exists && !replace {
                        println!("{} Skipping existing profile: {}", style("→").yellow(), profile.name);
                        skipped += 1;
                        continue;
                    }

                    // Add or update profile
                    let result = if exists {
                        println!("{} Updating existing profile: {}", style("→").cyan(), profile.name);
                        self.profile_service.update_profile(profile).await
                    } else {
                        println!("{} Adding new profile: {}", style("→").cyan(), profile.name);
                        self.profile_service.add_profile(profile).await
                    };

                    match result {
                        Ok(_) => imported += 1,
                        Err(e) => {
                            println!("{} Failed to import profile: {}", style("✗").red().bold(), e);
                            skipped += 1;
                        },
                    }
                }

                println!("{} Successfully imported {} profiles, skipped {}",
                         style("✓").green().bold(),
                         imported,
                         skipped);
            },
            Err(e) => {
                println!("{} Failed to import profiles: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin' command
    async fn handle_plugin(&self, args: PluginArgs) -> anyhow::Result<()> {
        match args.command {
            PluginCommands::List => self.handle_plugin_list().await?,
            PluginCommands::Available => self.handle_plugin_available().await?,
            PluginCommands::Install { url } => self.handle_plugin_install(url).await?,
            PluginCommands::Update { name } => self.handle_plugin_update(name).await?,
            PluginCommands::Remove { name } => self.handle_plugin_remove(name).await?,
            PluginCommands::Enable { name } => self.handle_plugin_enable(name).await?,
            PluginCommands::Disable { name } => self.handle_plugin_disable(name).await?,
            PluginCommands::Run { name, command, args } => self.handle_plugin_run(name, command, args).await?,
        }

        Ok(())
    }

    /// Handle the 'plugin list' command
    async fn handle_plugin_list(&self) -> anyhow::Result<()> {
        println!("{}", style("Installed plugins:").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());
        println!("{:<15} {:<10} {:<10} {:<20}",
                 style("NAME").cyan().bold(),
                 style("VERSION").cyan().bold(),
                 style("STATUS").cyan().bold(),
                 style("DESCRIPTION").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());

        let plugins = self.plugin_service.list_plugins().await?;

        if plugins.is_empty() {
            println!("{} No plugins installed.", style("!").yellow().bold());
            println!("Use '{}' to install a plugin.", style("shellbe plugin install <url>").cyan());
            return Ok(());
        }

        for plugin in plugins {
            let status = match plugin.status {
                crate::domain::PluginStatus::Enabled => style("enabled").green(),
                crate::domain::PluginStatus::Disabled => style("disabled").yellow(),
            };

            println!("{:<15} {:<10} {:<10} {:<20}",
                     style(&plugin.info.name).green(),
                     style(&plugin.info.version).blue(),
                     status,
                     plugin.info.description);
        }

        Ok(())
    }

    /// Handle the 'plugin available' command
    async fn handle_plugin_available(&self) -> anyhow::Result<()> {
        println!("{} Checking for available plugins...", style("→").cyan().bold());

        // This would normally be implemented by querying a plugin registry
        // For now, display a list of example plugins
        println!("{}", style("-------------------------------------").yellow());
        println!("{:<20} {:<15} {:<25}",
                 style("NAME").cyan().bold(),
                 style("AUTHOR").cyan().bold(),
                 style("DESCRIPTION").cyan().bold());
        println!("{}", style("-------------------------------------").yellow());

        println!("{:<20} {:<15} {:<25}",
                 style("shellbe-stats").green(),
                 "arash",
                 "Connection statistics and graphs");

        println!("{:<20} {:<15} {:<25}",
                 style("shellbe-sync").green(),
                 "arash",
                 "Sync profiles across devices");

        println!("{:<20} {:<15} {:<25}",
                 style("shellbe-menu").green(),
                 "arash",
                 "Interactive terminal menu");

        println!("\n{} To install a plugin, use:", style("→").yellow());
        println!("  {}", style("shellbe plugin install <github-username>/<repository-name>").cyan());
        println!("For example: {}", style("shellbe plugin install arash/shellbe-stats").cyan());

        Ok(())
    }

    /// Handle the 'plugin install' command
    async fn handle_plugin_install(&self, url: String) -> anyhow::Result<()> {
        println!("{} Installing plugin from {}...", style("→").cyan().bold(), style(&url).blue());

        match self.plugin_service.install_from_github(&url).await {
            Ok(metadata) => {
                println!("{} Plugin '{}' (version {}) installed successfully!",
                         style("✓").green().bold(),
                         style(&metadata.info.name).green(),
                         metadata.info.version);
                println!("{} Description: {}", style("→").cyan(), metadata.info.description);

                // Ask if user wants to enable the plugin
                let enable_plugin = Confirm::new()
                    .with_prompt("Enable this plugin now?")
                    .default(true)
                    .interact()?;

                if enable_plugin {
                    match self.plugin_service.enable_plugin(&metadata.info.name).await {
                        Ok(_) => println!("{} Plugin enabled", style("✓").green().bold()),
                        Err(e) => println!("{} Failed to enable plugin: {}", style("✗").red().bold(), e),
                    }
                } else {
                    println!("{} Plugin installed but not enabled.", style("!").yellow().bold());
                    println!("Use '{}' to enable it.",
                             style(format!("shellbe plugin enable {}", metadata.info.name)).cyan());
                }
            },
            Err(e) => {
                println!("{} Failed to install plugin: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin update' command
    async fn handle_plugin_update(&self, name: String) -> anyhow::Result<()> {
        println!("{} Updating plugin '{}'...", style("→").cyan().bold(), style(&name).green());

        match self.plugin_service.update_plugin(&name).await {
            Ok(metadata) => {
                println!("{} Plugin '{}' updated successfully to version {}!",
                         style("✓").green().bold(),
                         style(&metadata.info.name).green(),
                         metadata.info.version);
            },
            Err(e) => {
                println!("{} Failed to update plugin: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin remove' command
    async fn handle_plugin_remove(&self, name: String) -> anyhow::Result<()> {
        // Confirm removal
        let confirm = Confirm::new()
            .with_prompt(format!("Are you sure you want to remove plugin '{}'?", name))
            .default(false)
            .interact()?;

        if !confirm {
            println!("{} Removal cancelled", style("!").yellow().bold());
            return Ok(());
        }

        println!("{} Removing plugin '{}'...", style("→").cyan().bold(), style(&name).green());

        match self.plugin_service.remove_plugin(&name).await {
            Ok(_) => {
                println!("{} Plugin '{}' removed successfully", style("✓").green().bold(), name);
            },
            Err(e) => {
                println!("{} Failed to remove plugin: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin enable' command
    async fn handle_plugin_enable(&self, name: String) -> anyhow::Result<()> {
        println!("{} Enabling plugin '{}'...", style("→").cyan().bold(), style(&name).green());

        match self.plugin_service.enable_plugin(&name).await {
            Ok(_) => {
                println!("{} Plugin '{}' enabled successfully", style("✓").green().bold(), name);
            },
            Err(e) => {
                println!("{} Failed to enable plugin: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin disable' command
    async fn handle_plugin_disable(&self, name: String) -> anyhow::Result<()> {
        println!("{} Disabling plugin '{}'...", style("→").cyan().bold(), style(&name).green());

        match self.plugin_service.disable_plugin(&name).await {
            Ok(_) => {
                println!("{} Plugin '{}' disabled successfully", style("✓").green().bold(), name);
            },
            Err(e) => {
                println!("{} Failed to disable plugin: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }

    /// Handle the 'plugin run' command
    async fn handle_plugin_run(&self, name: String, command: String, args: Vec<String>) -> anyhow::Result<()> {
        println!("{} Running plugin command: {} {}",
                 style("→").cyan().bold(),
                 style(format!("{} {}", name, command)).green(),
                 args.join(" "));

        match self.plugin_service.execute_command(&name, &command, &args).await {
            Ok(_) => {
                println!("{} Command executed successfully", style("✓").green().bold());
            },
            Err(e) => {
                println!("{} Failed to execute command: {}", style("✗").red().bold(), e);
            },
        }

        Ok(())
    }
}