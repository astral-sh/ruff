#![cfg(not(target_family = "wasm"))]

#[cfg(unix)]
use std::path::Path;
use std::str;

use anyhow::Result;
use assert_cmd::Command;
#[cfg(unix)]
use path_absolutize::path_dedot;

const BIN_NAME: &str = "ruff";

#[test]
fn test_stdin_success() -> Result<()> {
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.args(["-", "--format", "text", "--isolated"])
        .write_stdin("")
        .assert()
        .success();
    Ok(())
}

#[test]
fn test_stdin_error() -> Result<()> {
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
fn test_stdin_filename() -> Result<()> {
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
fn test_stdin_json() -> Result<()> {
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
    "message": "`os` imported but unused",
    "fix": {{
      "content": "",
      "message": "Remove unused import: `os`",
      "location": {{
        "row": 1,
        "column": 0
      }},
      "end_location": {{
        "row": 2,
        "column": 0
      }}
    }},
    "location": {{
      "row": 1,
      "column": 8
    }},
    "end_location": {{
      "row": 1,
      "column": 10
    }},
    "filename": "{file_path}"
  }}
]
"#
        )
    );
    Ok(())
}

#[test]
fn test_stdin_autofix() -> Result<()> {
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
fn test_stdin_autofix_when_not_fixable_should_still_print_contents() -> Result<()> {
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
fn test_stdin_autofix_when_no_issues_should_still_print_contents() -> Result<()> {
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
fn test_show_source() -> Result<()> {
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
        .args(["-", "--format", "text", "--select", "F401", "--statistics"])
        .write_stdin("import sys\nimport os\n\nprint(os.getuid())\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?
            .lines()
            .last()
            .unwrap(),
        "1\tF401\t`sys` imported but unused"
    );
    Ok(())
}
