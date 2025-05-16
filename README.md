# ShellBe | Don't use. Problematic.

A comprehensive SSH management tool with plugin support, implemented in Rust.

## Features

- **Profile Management**: Create, edit, and remove SSH connection profiles
- **Connection Handling**: Connect to profiles, test connections, and copy SSH keys
- **Alias System**: Create aliases for connection profiles
- **SSH Config Integration**: Import from and export to SSH config
- **Connection History**: Track and display connection history and statistics
- **Plugin System**: Extend functionality through plugins
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Installation

### From Source

To build and install ShellBe from source, you'll need Rust and Cargo installed.

```bash
git clone https://github.com/arash/shellbe.git
cd shellbe
cargo install --path .
```

### Using Cargo

```bash
cargo install shellbe
```

## Usage

```
ShellBe - SSH management tool with plugin support

Usage: shellbe [COMMAND]

Commands:
  add          Add a new SSH connection profile
  list         List all configured SSH profiles
  connect      Connect to a saved profile
  copy-id      Copy SSH key to a remote server
  generate-key Generate a new SSH key pair
  alias        Create an alias for a connection
  aliases      List all connection aliases
  remove       Remove a profile
  edit         Edit a profile
  test         Test connection to a profile
  history      Show connection history
  export       Export profiles to SSH config
  import       Import profiles from SSH config
  plugin       Plugin management commands
  update       Update ShellBe to the latest version
  uninstall    Uninstall ShellBe
  help         Print this message or the help of the given subcommand(s)
```

### Examples

```bash
# Add a new profile
shellbe add

# Connect to a profile
shellbe connect work-server

# Copy SSH key to server
shellbe copy-id work-server

# Create an alias
shellbe alias ws work-server

# Test a connection
shellbe test work-server

# List all profiles
shellbe list

# Show connection history
shellbe history

# Install a plugin
shellbe plugin install username/shellbe-plugin
```

## Plugin Development

ShellBe provides a plugin SDK for developing plugins. To create a plugin:

1. Create a new Rust project
2. Add the ShellBe plugin SDK as a dependency:

```toml
[dependencies]
shellbe-plugin-sdk = "2.0.0"
```

3. Implement the `Plugin` trait:

```rust
use shellbe_plugin_sdk::{Plugin, PluginInfo, PluginCommand, Hook, Profile, PluginResult, declare_plugin};
use async_trait::async_trait;

#[derive(Default)]
struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "my-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "My awesome plugin".to_string(),
            author: "Your Name".to_string(),
            source_url: Some("https://github.com/username/my-plugin".to_string()),
            api_version: shellbe_plugin_sdk::API_VERSION.to_string(),
        }
    }
    
    fn commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand {
                name: "hello".to_string(),
                description: "Say hello".to_string(),
                usage: "shellbe plugin run my-plugin hello [name]".to_string(),
            },
        ]
    }
    
    async fn execute_hook(&self, hook: Hook, profile: Option<&Profile>) -> PluginResult {
        // Handle hook
        Ok(())
    }
    
    async fn execute_command(&self, command: &str, args: &[String]) -> PluginResult {
        match command {
            "hello" => {
                let name = args.get(0).map(|s| s.as_str()).unwrap_or("world");
                println!("Hello, {}!", name);
                Ok(())
            },
            _ => Err(format!("Unknown command: {}", command).into()),
        }
    }
}

// Declare the plugin factory function
declare_plugin!(MyPlugin);
```

4. Build the plugin as a dynamic library:

```toml
[lib]
name = "my_plugin"
crate-type = ["cdylib"]
```

5. Install the plugin:

```bash
shellbe plugin install username/my-plugin
```

## Security

ShellBe takes security seriously, especially with its plugin system. All plugins undergo security validation before loading to help prevent potentially harmful code execution. The plugin sandboxing restricts file system access, network access, and resource usage to enhance security.

## Configuration

ShellBe stores its configuration in `~/.shellbe/`:

- `profiles.json`: SSH connection profiles
- `aliases.json`: Profile aliases
- `history.json`: Connection history
- `plugins.json`: Plugin metadata
- `plugins/`: Plugin libraries

## System Requirements

- SSH tools (ssh, ssh-keygen, ssh-copy-id)
- Git (for plugin management)
- 10MB minimum disk space

## License

MIT
