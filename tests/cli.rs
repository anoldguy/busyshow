//! Drives the built binary. `show` talks to a throwaway fake bar.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

use busybar_anim::{Target, decode};

fn fixture(path: &str) -> String {
    format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))
}

fn busyshow() -> Command {
    Command::new(env!("CARGO_BIN_EXE_busyshow"))
}

/// Runs `convert` on the fixture with `extra` flags and decodes what it wrote.
fn convert_fixture(extra: &[&str]) -> busybar_anim::Animation {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tracks.anim");
    let status = busyshow()
        .args(["convert", &fixture("tracks_72x16.gif"), "-o"])
        .arg(&out)
        .args(extra)
        .status()
        .unwrap();
    assert!(status.success());
    decode(&std::fs::read(&out).unwrap()).unwrap()
}

#[test]
fn convert_writes_an_anim_for_the_front_screen() {
    let anim = convert_fixture(&[]);
    assert_eq!(anim.target(), Target::FRONT);
    assert!(!anim.frames().is_empty());
}

#[test]
fn convert_sizes_the_anim_for_the_back_screen() {
    let anim = convert_fixture(&["--screen", "back"]);
    assert_eq!(anim.target(), Target::BACK);
}

#[test]
fn convert_defaults_the_output_to_the_input_name_with_anim() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("tracks.gif");
    std::fs::copy(fixture("tracks_72x16.gif"), &input).unwrap();
    let status = busyshow().arg("convert").arg(&input).status().unwrap();
    assert!(status.success());
    let written = std::fs::read(dir.path().join("tracks.anim")).unwrap();
    assert_eq!(decode(&written).unwrap().target(), Target::FRONT);
}

#[test]
fn convert_reports_a_missing_file() {
    let output = busyshow()
        .args(["convert", "/nonexistent/nope.gif"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("could not read /nonexistent/nope.gif"),
        "{stderr}"
    );
}

#[test]
fn convert_reports_an_undecodable_file() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.gif");
    std::fs::write(&junk, b"not an image").unwrap();
    let output = busyshow().arg("convert").arg(&junk).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let want = format!("could not convert {}", junk.display());
    assert!(stderr.contains(&want), "{stderr}");
}

/// (request line, lowercased header lines, body)
type Seen = Vec<(String, Vec<String>, Vec<u8>)>;

/// Accepts `count` HTTP requests, answers each with OK, and returns what it saw.
fn fake_bar(count: usize) -> (String, thread::JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        (0..count)
            .map(|_| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut content_length = 0;
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    let line = line.trim().to_ascii_lowercase();
                    if let Some(value) = line.strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap();
                    }
                    headers.push(line);
                }
                let mut body = vec![0; content_length];
                reader.read_exact(&mut body).unwrap();
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                          Content-Length: 15\r\nConnection: close\r\n\r\n{\"result\":\"OK\"}",
                    )
                    .unwrap();
                (request_line, headers, body)
            })
            .collect()
    });
    (url, handle)
}

#[test]
fn show_uploads_the_anim_then_draws_it() {
    let (url, bar) = fake_bar(2);
    let status = busyshow()
        .args(["show", &fixture("tracks_72x16.gif"), "--url", &url])
        .args([
            "--seconds",
            "7",
            "--once",
            "--app",
            "demo",
            "--priority",
            "60",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let seen = bar.join().unwrap();
    let (upload, _, anim) = &seen[0];
    assert!(upload.starts_with("POST /api/assets/upload?"), "{upload}");
    assert!(upload.contains("application_name=demo"), "{upload}");
    assert!(upload.contains("file=busyshow.anim"), "{upload}");
    assert_eq!(decode(anim).unwrap().target(), Target::FRONT);

    let (draw, _, body) = &seen[1];
    assert!(draw.starts_with("POST /api/display/draw "), "{draw}");
    let body = String::from_utf8(body.clone()).unwrap();
    for needle in [
        r#""application_name":"demo""#,
        r#""priority":60"#,
        r#""type":"animation""#,
        r#""path":"busyshow.anim""#,
        r#""timeout":7"#,
        r#""loop":false"#,
        r#""display":"front""#,
    ] {
        assert!(body.contains(needle), "missing {needle} in {body}");
    }
}

#[test]
fn show_sends_the_local_api_token_header_on_every_request() {
    let (url, bar) = fake_bar(2);
    let status = busyshow()
        .args(["show", &fixture("tracks_72x16.gif"), "--url", &url])
        .args(["--api-token", "hunter2"])
        .status()
        .unwrap();
    assert!(status.success());

    for (line, headers, _) in bar.join().unwrap() {
        assert!(
            headers.contains(&"x-api-token: hunter2".to_string()),
            "{line}: {headers:?}"
        );
    }
}

#[test]
fn show_draws_on_the_back_screen() {
    let (url, bar) = fake_bar(2);
    let status = busyshow()
        .args(["show", &fixture("tracks_72x16.gif"), "--url", &url])
        .args(["--screen", "back"])
        .status()
        .unwrap();
    assert!(status.success());

    let seen = bar.join().unwrap();
    assert_eq!(decode(&seen[0].2).unwrap().target(), Target::BACK);
    let body = String::from_utf8(seen[1].2.clone()).unwrap();
    assert!(body.contains(r#""display":"back""#), "{body}");
}

#[test]
fn show_with_zero_seconds_says_until_cleared() {
    let (url, bar) = fake_bar(2);
    let output = busyshow()
        .args(["show", &fixture("tracks_72x16.gif"), "--url", &url])
        .args(["--seconds", "0"])
        .output()
        .unwrap();
    assert!(output.status.success());
    bar.join().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("until cleared"), "{stdout}");
}
