#![cfg(not(target_family = "wasm"))]

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::path::Path;
use std::str;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
use assert_cmd::Command;
#[cfg(unix)]
use clap::Parser;
#[cfg(unix)]
use path_absolutize::path_dedot;
#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
use ruff_cli::args::Args;
#[cfg(unix)]
use ruff_cli::run;

const BIN_NAME: &str = "ruff";

#[test]
fn stdin_success() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.args(["-", "--format", "text", "--isolated"])
        .write_stdin("")
        .assert()
        .success();
    Ok(())
}

#[test]
fn stdin_error() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["-", "--format", "text", "--isolated"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        r#"-:1:8: F401 [*] `os` imported but unused
Found 1 error.
[*] 1 potentially fixable with the --fix option.
"#
    );
    Ok(())
}

#[test]
fn stdin_filename() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args([
            "-",
            "--format",
            "text",
            "--stdin-filename",
            "F401.py",
            "--isolated",
        ])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        r#"F401.py:1:8: F401 [*] `os` imported but unused
Found 1 error.
[*] 1 potentially fixable with the --fix option.
"#
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn stdin_json() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args([
            "-",
            "--format",
            "json",
            "--stdin-filename",
            "F401.py",
            "--isolated",
        ])
        .write_stdin("import os\n")
        .assert()
        .failure();

    let directory = path_dedot::CWD.to_str().unwrap();
    let binding = Path::new(directory).join("F401.py");
    let file_path = binding.display();

    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        format!(
            r#"[
  {{
    "code": "F401",
    "end_location": {{
      "column": 10,
      "row": 1
    }},
    "filename": "{file_path}",
    "fix": {{
      "applicability": "Automatic",
      "edits": [
        {{
          "content": "",
          "end_location": {{
            "column": 1,
            "row": 2
          }},
          "location": {{
            "column": 1,
            "row": 1
          }}
        }}
      ],
      "message": "Remove unused import: `os`"
    }},
    "location": {{
      "column": 8,
      "row": 1
    }},
    "message": "`os` imported but unused",
    "noqa_row": 1,
    "url": "https://beta.ruff.rs/docs/rules/unused-import"
  }}
]"#
        )
    );
    Ok(())
}

#[test]
fn stdin_autofix() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["-", "--format", "text", "--fix", "--isolated"])
        .write_stdin("import os\nimport sys\n\nprint(sys.version)\n")
        .assert()
        .success();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "import sys\n\nprint(sys.version)\n"
    );
    Ok(())
}

#[test]
fn stdin_autofix_when_not_fixable_should_still_print_contents() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["-", "--format", "text", "--fix", "--isolated"])
        .write_stdin("import os\nimport sys\n\nif (1, 2):\n     print(sys.version)\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "import sys\n\nif (1, 2):\n     print(sys.version)\n"
    );
    Ok(())
}

#[test]
fn stdin_autofix_when_no_issues_should_still_print_contents() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["-", "--format", "text", "--fix", "--isolated"])
        .write_stdin("import sys\n\nprint(sys.version)\n")
        .assert()
        .success();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "import sys\n\nprint(sys.version)\n"
    );
    Ok(())
}

#[test]
fn show_source() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["-", "--format", "text", "--show-source", "--isolated"])
        .write_stdin("l = 1")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("l = 1"));
    Ok(())
}

#[test]
fn explain_status_codes() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.args(["--explain", "F401"]).assert().success();
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.args(["--explain", "RUF404"]).assert().failure();
    Ok(())
}

#[test]
fn show_statistics() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args([
            "-",
            "--format",
            "text",
            "--select",
            "F401",
            "--statistics",
            "--isolated",
        ])
        .write_stdin("import sys\nimport os\n\nprint(os.getuid())\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?
            .lines()
            .last()
            .unwrap(),
        "1\tF401\t[*] `sys` imported but unused"
    );
    Ok(())
}

#[test]
fn nursery_prefix() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;

    // `--select E` should detect E741, but not E225, which is in the nursery.
    let output = cmd
        .args(["-", "--format", "text", "--isolated", "--select", "E"])
        .write_stdin("I=42\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        r#"-:1:1: E741 Ambiguous variable name: `I`
Found 1 error.
"#
    );

    Ok(())
}

#[test]
fn nursery_all() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;

    // `--select ALL` should detect E741, but not E225, which is in the nursery.
    let output = cmd
        .args(["-", "--format", "text", "--isolated", "--select", "E"])
        .write_stdin("I=42\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        r#"-:1:1: E741 Ambiguous variable name: `I`
Found 1 error.
"#
    );

    Ok(())
}

#[test]
fn nursery_direct() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;

    // `--select E225` should detect E225.
    let output = cmd
        .args(["-", "--format", "text", "--isolated", "--select", "E225"])
        .write_stdin("I=42\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        r#"-:1:2: E225 Missing whitespace around operator
Found 1 error.
"#
    );

    Ok(())
}

/// An unreadable pyproject.toml in non-isolated mode causes ruff to hard-error trying to build up
/// configuration globs
#[cfg(unix)]
#[test]
fn unreadable_pyproject_toml() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    // Create an empty file with 000 permissions
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o000)
        .open(pyproject_toml)?;

    // Don't `--isolated` since the configuration discovery is where the error happens
    let args = Args::parse_from(["", "check", "--no-cache", tempdir.path().to_str().unwrap()]);
    let err = run(args).err().context("Unexpected success")?;
    assert_eq!(
        err.chain()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Permission denied (os error 13)".to_string()],
    );
    Ok(())
}

/// Check the output with an unreadable directory
#[cfg(unix)]
#[test]
fn unreadable_dir() -> Result<()> {
    // Create a directory with 000 (not iterable/readable) permissions
    let tempdir = TempDir::new()?;
    let unreadable_dir = tempdir.path().join("unreadable_dir");
    fs::create_dir(&unreadable_dir)?;
    fs::set_permissions(&unreadable_dir, Permissions::from_mode(0o000))?;

    // We (currently?) have to use a subcommand to check exit status (currently wrong) and logging
    // output
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd
        .args(["--no-cache", "--isolated"])
        .arg(&unreadable_dir)
        .assert()
        // TODO(konstin): This should be a failure, but we currently can't track that
        .success();
    assert_eq!(
        str::from_utf8(&output.get_output().stderr)?,
        "warning: Encountered error: Permission denied (os error 13)\n"
    );
    Ok(())
}
