use std::str;

use anyhow::Result;
use assert_cmd::{crate_name, Command};

#[test]
fn test_stdin_success() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    cmd.args(["-", "--format", "text"])
        .write_stdin("")
        .assert()
        .success();
    Ok(())
}

#[test]
fn test_stdin_error() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("-:1:8: F401"));
    Ok(())
}

#[test]
fn test_stdin_filename() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text", "--stdin-filename", "F401.py"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("F401.py:1:8: F401"));
    Ok(())
}

#[test]
fn test_stdin_autofix() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text", "--fix"])
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
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text", "--fix"])
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
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text", "--fix"])
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
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "text", "--show-source"])
        .write_stdin("l = 1")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("l = 1"));
    Ok(())
}
