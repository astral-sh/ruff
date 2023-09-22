#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::process::Command;
use std::str;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";

#[test]
fn default_options() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print('Should\'t change quotes')


if condition:

    print('Hy "Micha"') # Should not change quotes

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo(
        arg1,
        arg2,
    ):
        print("Should't change quotes")


    if condition:
        print('Hy "Micha"')  # Should not change quotes

    ----- stderr -----
    warning: `ruff format` is a work-in-progress, subject to change at any time, and intended only for experimentation.
    "###);
}

#[test]
fn format_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[format]
indent-style = "tab"
quote-style = "single"
skip-magic-trailing-comma = true
line-ending = "cr-lf"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't change quotes")


if condition:

    print("Should change quotes")

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo(arg1, arg2):
    	print("Shouldn't change quotes")


    if condition:
    	print('Should change quotes')

    ----- stderr -----
    warning: `ruff format` is a work-in-progress, subject to change at any time, and intended only for experimentation.
    "###);
    Ok(())
}

#[test]
fn format_option_inheritance() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    let base_toml = tempdir.path().join("base.toml");
    fs::write(
        &ruff_toml,
        r#"
extend = "base.toml"

[format]
quote-style = "single"
"#,
    )?;

    fs::write(
        base_toml,
        r#"
[format]
indent-style = "tab"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't change quotes")


if condition:

    print("Should change quotes")

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo(
    	arg1,
    	arg2,
    ):
    	print("Shouldn't change quotes")


    if condition:
    	print('Should change quotes')

    ----- stderr -----
    warning: `ruff format` is a work-in-progress, subject to change at any time, and intended only for experimentation.
    "###);
    Ok(())
}

/// Tests that the legacy `format` option continues to work but emits a warning.
#[test]
fn legacy_format_option() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
format = "json"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--select", "F401", "--no-cache", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
import os
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    [
      {
        "code": "F401",
        "end_location": {
          "column": 10,
          "row": 2
        },
        "filename": "-",
        "fix": {
          "applicability": "Automatic",
          "edits": [
            {
              "content": "",
              "end_location": {
                "column": 1,
                "row": 3
              },
              "location": {
                "column": 1,
                "row": 2
              }
            }
          ],
          "message": "Remove unused import: `os`"
        },
        "location": {
          "column": 8,
          "row": 2
        },
        "message": "`os` imported but unused",
        "noqa_row": 2,
        "url": "https://docs.astral.sh/ruff/rules/unused-import"
      }
    ]
    ----- stderr -----
    warning: The option `format` has been deprecated to avoid ambiguity with Ruff's upcoming formatter. Use `output-format` instead.
    "###);
    Ok(())
}
