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
    assert!(!stdout.contains("subscribe"));
}

#[test]
fn create_rejects_subscribe_flag() {
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

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument '--subscribe'"),
        "--subscribe should be rejected; got:\n{stderr}"
    );
}

#[test]
fn subscription_command_and_create_help_are_removed() {
    for args in [["subscribe", "--help"], ["create", "--help"]] {
        let output = Command::new(spx_bin()).args(args).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains("Purchase a subscription"));
        assert!(!stdout.contains("--subscribe"));
        assert!(!stdout.contains("SPX_AUTO_SUBSCRIBE"));
    }
}

fn publish_against_mock(args: &[&str], status: &str, body: &str) -> std::process::Output {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};

    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".spx")).unwrap();
    std::fs::write(
        tmp.path().join(".spx/credentials.json"),
        r#"{"username":"test","token":"test-token"}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("report.html"), "<html>test</html>").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let stop = finished.clone();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::new();
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(err) => panic!("accept: {err}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(request).unwrap());
            stream.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    let output = Command::new(spx_bin())
        .args(args)
        .current_dir(tmp.path())
        .env("HOME", tmp.path())
        .env("SPX_API_URL", url)
        .env("SPX_AUTO_SUBSCRIBE", "1")
        .output()
        .unwrap();
    finished.store(true, Ordering::SeqCst);
    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "unexpected billing request or retry: {requests:?}"
    );
    assert!(requests[0].starts_with("POST /pub HTTP/1.1\r\n"));
    assert!(requests[0].contains("<html>test</html>"));
    output
}

#[test]
fn publishing_succeeds_without_billing() {
    let body = r#"{"slug":"abc123","url":"https://abc123.runspx.dev/","original_filename":"report.html","size_bytes":17,"sha256":"test","content_type":"text/html","created_at":"now","updated_at":"now"}"#;
    for args in [vec!["report.html"], vec!["create", "report.html"]] {
        let output = publish_against_mock(&args, "201 Created", body);
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "https://abc123.runspx.dev/"
        );
    }
}

#[test]
fn unexpected_402_is_an_error_without_checkout_or_retry() {
    for (body, expected) in [
        (
            r#"{"detail":"Publishing unavailable"}"#,
            "Publishing unavailable",
        ),
        ("upstream error", "returned 402: upstream error"),
    ] {
        let output = publish_against_mock(&["create", "report.html"], "402 Payment Required", body);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!stderr.contains("subscribe"), "{stderr}");
        assert!(!stderr.contains("checkout"), "{stderr}");
    }
}
