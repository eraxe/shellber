use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::*;
use shellbe::utils::ensure_directory;
use std::sync::Once;

static INIT: Once = Once::new();

/// Setup function that runs once for all tests
fn setup() {
    INIT.call_once(|| {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .try_init();
    });
}

#[test]
fn test_cli_version() {
    setup();
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.arg("--version");
    cmd.assert().success().stdout(predicate::str::contains("shellbe"));
}

#[test]
fn test_cli_help() {
    setup();
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("SSH management tool"));
}

#[test]
fn test_cli_list_empty() {
    setup();
    let temp = assert_fs::TempDir::new().unwrap();
    let config_dir = temp.path().join(".shellbe");

    // Create empty config
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        ensure_directory(&config_dir).await.unwrap();
    });

    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No profiles found"));
}

#[test]
fn test_cli_add_non_interactive() {
    setup();
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("add")
        .arg("--name").arg("test-server")
        .arg("--host").arg("example.com")
        .arg("--user").arg("testuser")
        .arg("--port").arg("2222")
        .arg("--non-interactive");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("added successfully"));

    // Check that the profile file exists
    let profiles_file = temp.path().join(".shellbe").join("profiles.json");
    assert!(profiles_file.exists());

    // Verify the content of the file
    let content = std::fs::read_to_string(profiles_file).unwrap();
    assert!(content.contains("test-server"));
    assert!(content.contains("example.com"));
    assert!(content.contains("testuser"));
    assert!(content.contains("2222"));
}

#[test]
fn test_cli_list_with_profile() {
    setup();
    let temp = assert_fs::TempDir::new().unwrap();

    // First add a profile
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("add")
        .arg("--name").arg("test-server")
        .arg("--host").arg("example.com")
        .arg("--user").arg("testuser")
        .arg("--port").arg("2222")
        .arg("--non-interactive");

    cmd.assert().success();

    // Now list profiles
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test-server"))
        .stdout(predicate::str::contains("example.com"))
        .stdout(predicate::str::contains("testuser"));
}

#[test]
fn test_cli_remove() {
    setup();
    let temp = assert_fs::TempDir::new().unwrap();

    // First add a profile
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("add")
        .arg("--name").arg("test-server")
        .arg("--host").arg("example.com")
        .arg("--user").arg("testuser")
        .arg("--non-interactive");

    cmd.assert().success();

    // Remove the profile with auto-confirmation
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("remove")
        .arg("test-server")
        .write_stdin("y\n"); // Confirm removal

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("removed successfully"));

    // Verify profile is gone
    let mut cmd = Command::cargo_bin("shellbe").unwrap();
    cmd.env("HOME", temp.path())
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No profiles found"));
}