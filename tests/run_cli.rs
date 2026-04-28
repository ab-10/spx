use std::process::Command;

#[test]
fn not_logged_in_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp_dir.path().join("hi.py"), "print('hi')").unwrap();

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run", "hi.py"])
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())  // no credentials.json here
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx run should have failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("spx login"),
        "stderr should tell the user to run `spx login`; got:\n{stderr}"
    );
}

#[test]
fn missing_filename_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run"])
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx run with no filename should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FILENAME") || stderr.contains("required"),
        "stderr should mention FILENAME; got:\n{stderr}"
    );
}
