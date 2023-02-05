#![cfg(not(target_family = "wasm"))]

use std::io::{ErrorKind, Read};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread::sleep;
use std::time::Duration;
use std::{fs, process, str};

use anyhow::{anyhow, Context, Result};
use assert_cmd::Command;
use log::info;
use ruff::logging::{set_up_logging, LogLevel};
use walkdir::WalkDir;

/// Handles `blackd` process and allows submitting code to it for formatting.
struct Blackd {
    address: SocketAddr,
    server: process::Child,
    client: ureq::Agent,
}

const BIN_NAME: &str = "ruff";

impl Blackd {
    pub fn new() -> Result<Self> {
        // Get free TCP port to run on
        let address = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?.local_addr()?;

        let server = process::Command::new("blackd")
            .args([
                "--bind-host",
                &address.ip().to_string(),
                "--bind-port",
                &address.port().to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Starting blackd")?;

        // Wait up to four seconds for `blackd` to be ready.
        for _ in 0..20 {
            match TcpStream::connect(address) {
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                    info!("`blackd` not ready yet; retrying...");
                    sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e.into()),
                Ok(_) => {
                    info!("`blackd` ready");
                    break;
                }
            }
        }

        Ok(Self {
            address,
            server,
            client: ureq::agent(),
        })
    }

    /// Format given code with blackd.
    pub fn check(&self, code: &[u8]) -> Result<Vec<u8>> {
        match self
            .client
            .post(&format!("http://{}/", self.address))
            .set("X-Line-Length", "88")
            .send_bytes(code)
        {
            // 204 indicates the input wasn't changed during formatting, so
            // we return the original.
            Ok(response) => {
                if response.status() == 204 {
                    Ok(code.to_vec())
                } else {
                    let mut buf = vec![];
                    response
                        .into_reader()
                        .take((1024 * 1024) as u64)
                        .read_to_end(&mut buf)?;
                    Ok(buf)
                }
            }
            Err(ureq::Error::Status(_, response)) => Err(anyhow::anyhow!(
                "Formatting with `black` failed: {}",
                response.into_string()?
            )),
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for Blackd {
    fn drop(&mut self) {
        self.server.kill().expect("Couldn't end `blackd` process");
    }
}

fn run_test(path: &Path, blackd: &Blackd, ruff_args: &[&str]) -> Result<()> {
    let input = fs::read(path)?;

    // Step 1: Run `ruff` on the input.
    let step_1 = &Command::cargo_bin(BIN_NAME)?
        .args(ruff_args)
        .write_stdin(input)
        .assert()
        .append_context("step", "running input through ruff");
    if !step_1.get_output().status.success() {
        return Err(anyhow!(
            "Running input through ruff failed:\n{}",
            str::from_utf8(&step_1.get_output().stderr)?
        ));
    }
    let step_1_output = step_1.get_output().stdout.clone();

    // Step 2: Run `blackd` on the input.
    let step_2_output = blackd.check(&step_1_output)?;

    // Step 3: Re-run `ruff` on the input.
    let step_3 = &Command::cargo_bin(BIN_NAME)?
        .args(ruff_args)
        .write_stdin(step_2_output.clone())
        .assert();
    if !step_3.get_output().status.success() {
        return Err(anyhow!(
            "Running input through ruff after black failed:\n{}",
            str::from_utf8(&step_3.get_output().stderr)?
        ));
    }
    let step_3_output = step_3.get_output().stdout.clone();

    assert_eq!(
        str::from_utf8(&step_2_output),
        str::from_utf8(&step_3_output),
        "Mismatch found for {}",
        path.display()
    );

    Ok(())
}

#[test]
#[ignore]
fn test_ruff_black_compatibility() -> Result<()> {
    set_up_logging(&LogLevel::Default)?;

    let blackd = Blackd::new()?;

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("test")
        .join("fixtures");

    // Ignore some fixtures that currently trigger errors. `E999.py` especially, as
    // that is triggering a syntax error on purpose.
    let excludes = ["E999.py", "W605_1.py"];

    let paths = WalkDir::new(fixtures_dir)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map_or(false, |ext| ext == "py" || ext == "pyi")
                && !excludes.contains(&entry.path().file_name().unwrap().to_str().unwrap())
        });

    let ruff_args = [
        "-",
        "--silent",
        "--exit-zero",
        "--fix",
        "--line-length",
        "88",
        "--select ALL",
        // Exclude ruff codes, specifically RUF100, because it causes differences that are not a
        // problem. Ruff would add a `# noqa: W292`  after the first run, black introduces a
        // newline, and ruff removes the `# noqa: W292` again.
        "--ignore RUF",
    ];

    for entry in paths {
        let path = entry.path();
        run_test(path, &blackd, &ruff_args).context(format!("Testing {}", path.display()))?;
    }

    Ok(())
}
