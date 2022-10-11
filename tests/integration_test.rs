use std::str;

use anyhow::Result;
use assert_cmd::{crate_name, Command};

#[test]
fn test_stdin_success() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    cmd.args(&["-"]).write_stdin("").assert().success();
    Ok(())
}

#[test]
fn test_stdin_error() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(&["-"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("-:1:1: F401"));
    Ok(())
}

#[test]
fn test_stdin_filename() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(&["-", "--stdin-filename", "F401.py"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("F401.py:1:1: F401"));
    Ok(())
}

#[test]
fn test_stdin_autofix() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(&["-", "--fix"])
        .write_stdin("import os\n")
        .assert()
        .success();
    assert!(str::from_utf8(&output.get_output().stdout)?.contains("Found 0 error(s) (1 fixed)"));
    Ok(())
}
