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
    warning: `ruff format` is not yet stable, and subject to change in future versions.
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
    warning: `ruff format` is not yet stable, and subject to change in future versions.
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
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    "###);
    Ok(())
}

/// Since 0.1.0 the legacy format option is no longer supported
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

    insta::with_settings!({filters => vec![
        (&*regex::escape(ruff_toml.to_str().unwrap()), "[RUFF-TOML-PATH]"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(["check", "--select", "F401", "--no-cache", "--config"])
            .arg(&ruff_toml)
            .arg("-")
            .pass_stdin(r#"
    import os
    "#), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to parse `[RUFF-TOML-PATH]`: TOML parse error at line 2, column 10
          |
        2 | format = "json"
          |          ^^^^^^
        invalid type: string "json", expected struct FormatOptions

        "###);
    });
    Ok(())
}
