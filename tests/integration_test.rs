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
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "Found 1 error(s).\n-:1:8: F401 `os` imported but unused\n1 potentially fixable with the \
         --fix option.\n"
    );
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
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "Found 1 error(s).\nF401.py:1:8: F401 `os` imported but unused\n1 potentially fixable \
         with the --fix option.\n"
    );
    Ok(())
}

#[test]
fn test_stdin_json() -> Result<()> {
    let mut cmd = Command::cargo_bin(crate_name!())?;
    let output = cmd
        .args(["-", "--format", "json", "--stdin-filename", "F401.py"])
        .write_stdin("import os\n")
        .assert()
        .failure();
    assert_eq!(
        str::from_utf8(&output.get_output().stdout)?,
        "[\n  {\n    \"code\": \"F401\",\n    \"message\": \"`os` imported but unused\",\n    \
         \"fix\": {\n      \"content\": \"\",\n      \"location\": {\n        \"row\": 1,\n        \
         \"column\": 0\n      },\n      \"end_location\": {\n        \"row\": 2,\n        \
         \"column\": 0\n      }\n    },\n    \"location\": {\n      \"row\": 1,\n      \
         \"column\": 8\n    },\n    \"end_location\": {\n      \"row\": 1,\n      \"column\": \
         10\n    },\n    \"filename\": \"F401.py\"\n  }\n]\n"
    );
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
