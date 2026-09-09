use std::process::Command;

fn spx_bin() -> &'static str {
    env!("CARGO_BIN_EXE_spx")
}

#[test]
fn root_publish_not_logged_in_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp_dir.path().join("report.html"),
        "<!doctype html><html></html>",
    )
    .unwrap();

    let output = Command::new(spx_bin())
        .arg("report.html")
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx should have failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("spx login"),
        "stderr should tell the user to run `spx login`; got:\n{stderr}"
    );
}

#[test]
fn missing_path_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");

    let output = Command::new(spx_bin())
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx with no path should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("spx PATH") || stderr.contains("spx list"),
        "stderr should mention the new command shape; got:\n{stderr}"
    );
}

#[test]
fn deployment_commands_are_removed() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");

    for command in ["run", "new", "kill", "ps", "logs", "env", "uv", "pub"] {
        let output = Command::new(spx_bin())
            .arg(command)
            .current_dir(tmp_dir.path())
            .env("HOME", tmp_dir.path())
            .output()
            .expect("run spx");

        assert!(!output.status.success(), "spx {command} should fail");
    }
}

#[test]
fn help_shows_publishing_commands() {
    let output = Command::new(spx_bin())
        .arg("--help")
        .output()
        .expect("run spx");

    assert!(output.status.success(), "spx --help should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("standalone HTML file"));
    assert!(stdout.contains("create"));
    assert!(stdout.contains("update"));
    assert!(stdout.contains("delete"));
    assert!(stdout.contains("list"));
    assert!(!stdout.contains("  run "));
    assert!(!stdout.contains("  new "));
}

#[test]
fn create_accepts_subscribe_flag() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp_dir.path().join("report.html"),
        "<!doctype html><html></html>",
    )
    .unwrap();

    let output = Command::new(spx_bin())
        .args(["create", "report.html", "--subscribe"])
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())
        .output()
        .expect("run spx");

    // Not logged in, so it still fails - but on credentials, not on arg parsing.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--subscribe should be a valid flag; got:\n{stderr}"
    );
    assert!(
        stderr.contains("spx login"),
        "stderr should ask the user to log in; got:\n{stderr}"
    );
}
