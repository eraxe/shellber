use shellbe_plugin_sdk::{Plugin, PluginInfo, PluginCommand, Hook, Profile, PluginResult, declare_plugin};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::fs;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Default)]
pub struct StatsPlugin {
    stats: Arc<Mutex<Stats>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Stats {
    connections: HashMap<String, ProfileStats>,
    plugin_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ProfileStats {
    connection_count: usize,
    last_connected: Option<DateTime<Utc>>,
    success_count: usize,
    failure_count: usize,
    total_duration_secs: f64,
}

impl StatsPlugin {
    fn save_stats(&self) -> PluginResult {
        let stats = self.stats.lock().unwrap();
        if let Some(dir) = &stats.plugin_dir {
            let path = Path::new(dir).join("stats.json");
            let data = serde_json::to_string_pretty(&*stats)
                .map_err(|e| format!("Failed to serialize stats: {}", e))?;
            fs::write(&path, data)
                .map_err(|e| format!("Failed to write stats: {}", e))?;
        }
        Ok(())
    }

    fn load_stats(&self, dir: &Path) -> PluginResult {
        let path = dir.join("stats.json");
        if path.exists() {
            let mut file = fs::File::open(&path)
                .map_err(|e| format!("Failed to open stats file: {}", e))?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| format!("Failed to read stats file: {}", e))?;

            let mut stats = serde_json::from_str::<Stats>(&contents)
                .map_err(|e| format!("Failed to parse stats: {}", e))?;

            // Update plugin dir in case it changed
            stats.plugin_dir = Some(dir.to_string_lossy().to_string());

            let mut self_stats = self.stats.lock().unwrap();
            *self_stats = stats;
        } else {
            let mut self_stats = self.stats.lock().unwrap();
            self_stats.plugin_dir = Some(dir.to_string_lossy().to_string());
        }
        Ok(())
    }
}

#[async_trait]
impl Plugin for StatsPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "stats".to_string(),
            version: "1.0.0".to_string(),
            description: "Connection statistics tracking".to_string(),
            author: "ShellBe Team".to_string(),
            source_url: Some("https://github.com/arash/shellbe-stats".to_string()),
            api_version: shellbe_plugin_sdk::API_VERSION.to_string(),
        }
    }

    fn commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand {
                name: "show".to_string(),
                description: "Show connection statistics".to_string(),
                usage: "shellbe plugin run stats show [profile]".to_string(),
            },
            PluginCommand {
                name: "reset".to_string(),
                description: "Reset connection statistics".to_string(),
                usage: "shellbe plugin run stats reset [profile]".to_string(),
            },
        ]
    }

    async fn execute_hook(&self, hook: Hook, profile: Option<&Profile>) -> PluginResult {
        match hook {
            Hook::PreConnect => {
                if let Some(profile) = profile {
                    let mut stats = self.stats.lock().unwrap();
                    let profile_stats = stats.connections
                        .entry(profile.name.clone())
                        .or_insert_with(ProfileStats::default);

                    profile_stats.connection_count += 1;
                    profile_stats.last_connected = Some(Utc::now());

                    self.save_stats()?;
                }
            },
            Hook::TestSuccess => {
                if let Some(profile) = profile {
                    let mut stats = self.stats.lock().unwrap();
                    let profile_stats = stats.connections
                        .entry(profile.name.clone())
                        .or_insert_with(ProfileStats::default);

                    profile_stats.success_count += 1;

                    self.save_stats()?;
                }
            },
            Hook::TestFailure => {
                if let Some(profile) = profile {
                    let mut stats = self.stats.lock().unwrap();
                    let profile_stats = stats.connections
                        .entry(profile.name.clone())
                        .or_insert_with(ProfileStats::default);

                    profile_stats.failure_count += 1;

                    self.save_stats()?;
                }
            },
            Hook::PostDisconnect => {
                if let Some(profile) = profile {
                    let mut stats = self.stats.lock().unwrap();
                    let profile_stats = stats.connections
                        .entry(profile.name.clone())
                        .or_insert_with(ProfileStats::default);

                    // Update duration (estimate as 5 minutes if not tracked)
                    profile_stats.total_duration_secs += 300.0;

                    self.save_stats()?;
                }
            },
            _ => {}
        }

        Ok(())
    }

    async fn execute_command(&self, command: &str, args: &[String]) -> PluginResult {
        match command {
            "show" => {
                let stats = self.stats.lock().unwrap();

                if let Some(profile_name) = args.get(0) {
                    // Show stats for specific profile
                    if let Some(profile_stats) = stats.connections.get(profile_name) {
                        println!("Statistics for profile '{}':", profile_name);
                        println!("  Connections: {}", profile_stats.connection_count);
                        println!("  Successful tests: {}", profile_stats.success_count);
                        println!("  Failed tests: {}", profile_stats.failure_count);

                        let hours = profile_stats.total_duration_secs / 3600.0;
                        println!("  Total connection time: {:.2} hours", hours);

                        if let Some(last) = &profile_stats.last_connected {
                            println!("  Last connected: {}", last.format("%Y-%m-%d %H:%M:%S"));
                        }
                    } else {
                        println!("No statistics found for profile '{}'", profile_name);
                    }
                } else {
                    // Show summary for all profiles
                    println!("Connection Statistics Summary:");
                    println!("-----------------------------");

                    if stats.connections.is_empty() {
                        println!("No connection statistics recorded yet.");
                    } else {
                        // Sort profiles by connection count
                        let mut profiles: Vec<(&String, &ProfileStats)> =
                            stats.connections.iter().collect();
                        profiles.sort_by(|a, b| b.1.connection_count.cmp(&a.1.connection_count));

                        for (name, stats) in profiles {
                            println!("{}: {} connections", name, stats.connection_count);
                        }
                    }
                }
            },
            "reset" => {
                let mut stats = self.stats.lock().unwrap();

                if let Some(profile_name) = args.get(0) {
                    // Reset stats for specific profile
                    if stats.connections.remove(profile_name).is_some() {
                        println!("Statistics for profile '{}' have been reset.", profile_name);
                    } else {
                        println!("No statistics found for profile '{}'", profile_name);
                    }
                } else {
                    // Reset all stats
                    stats.connections.clear();
                    println!("All connection statistics have been reset.");
                }

                self.save_stats()?;
            },
            _ => {
                return Err(format!("Unknown command: {}", command).into());
            }
        }

        Ok(())
    }

    async fn on_enable(&self) -> PluginResult {
        println!("Stats plugin enabled. Connection statistics will be tracked.");
        Ok(())
    }

    async fn on_disable(&self) -> PluginResult {
        println!("Stats plugin disabled. Connection statistics will no longer be tracked.");
        Ok(())
    }

    async fn on_install(&self, plugin_dir: &Path) -> PluginResult {
        println!("Stats plugin installed. Connection statistics will be stored in {:?}", plugin_dir);
        self.load_stats(plugin_dir)?;
        Ok(())
    }

    async fn on_update(&self, plugin_dir: &Path) -> PluginResult {
        println!("Stats plugin updated. Your existing statistics have been preserved.");
        self.load_stats(plugin_dir)?;
        Ok(())
    }
}

// Declare the plugin factory function
declare_plugin!(StatsPlugin);