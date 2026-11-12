use super::*;

/// A local file server plays the release host. Health on another port fails, so `current`
/// goes back to the version that was running.
#[test]
pub(super) fn a_local_release_server_installs_and_a_failed_health_check_rolls_back() {
    let home = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let version = "1.2.3";
    let name = asset_name(version, &target());
    let stage = tempfile::tempdir().unwrap();
    let binary = stage.path().join("hbots");
    std::fs::write(&binary, "#!/bin/sh\necho 'hbots 1.2.3'\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let archive = files.path().join(&name);
    let packed = std::process::Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(stage.path())
        .arg("hbots")
        .status()
        .unwrap();
    assert!(packed.success());
    let bytes = std::fs::read(&archive).unwrap();
    use sha2::Digest;
    let sum = hex::encode(sha2::Sha256::digest(&bytes));
    std::fs::write(files.path().join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let file_port = listener.local_addr().unwrap().port();
    drop(listener);
    let file_server = StopChild(
        std::process::Command::new("python3")
            .args([
                "-m",
                "http.server",
                &file_port.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .current_dir(files.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let _stop = file_server;
    let ready = std::time::Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", file_port)).is_err() {
        assert!(
            ready.elapsed() < Duration::from_secs(5),
            "the file server did not listen"
        );
        std::thread::sleep(Duration::from_millis(30));
    }

    let base = format!("http://127.0.0.1:{file_port}");
    let release = Release {
        version: version.into(),
        archive: format!("{base}/{name}"),
        sums: format!("{base}/SHA256SUMS"),
        notes: String::new(),
        url: base,
        published_at: None,
    };
    let client = http_client().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let installed = runtime
        .block_on(install_release(&client, home.path(), &release, &|_| {}))
        .unwrap();
    assert!(installed.join("hbots").is_file());
    assert!(installed.join(".install-complete").is_file());

    let previous = RuntimePaths::new(home.path()).versions.join("1.0.0");
    std::fs::create_dir_all(&previous).unwrap();
    activate(&previous, home.path()).unwrap();

    let health = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let health_port = health.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in health.incoming().take(8) {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 512];
            use std::io::{Read, Write};
            let _ = stream.read(&mut buf);
            let body = b"no";
            let head = format!(
                "HTTP/1.1 500 ERR\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    let answering = |want: Option<&str>| {
        let body = probe_health(health_port, Duration::from_millis(200));
        match want {
            Some(v) => body.as_ref().and_then(|b| b["version"].as_str()) == Some(v),
            None => body.is_some(),
        }
    };
    let log = |_: &str| {};
    let restart = || Ok(());
    let err = switch_to(
        &installed,
        version,
        &Switch {
            home: home.path(),
            managed: true,
            restart: &restart,
            answering: &answering,
            log: &log,
            start_timeout: Duration::from_millis(600),
            rollback_timeout: Duration::from_millis(600),
        },
    )
    .unwrap_err();
    let current = read_link(&RuntimePaths::new(home.path()).current).unwrap();
    assert_eq!(current, previous, "{err}");
}

pub(super) struct StopChild(std::process::Child);

impl Drop for StopChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
