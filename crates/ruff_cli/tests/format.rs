#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";

#[test]
fn default_options() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py"])
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
tab-size = 8
line-width = 84

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
    print("Shouldn't change quotes. It exceeds the line width with the tab size 8")


if condition:

    print("Should change quotes")

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo(arg1, arg2):
    	print(
    		"Shouldn't change quotes. It exceeds the line width with the tab size 8"
    	)


    if condition:
    	print('Should change quotes')

    ----- stderr -----
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    "###);
    Ok(())
}

#[test]
fn mixed_line_endings() -> Result<()> {
    let tempdir = TempDir::new()?;

    fs::write(
        tempdir.path().join("main.py"),
        "from test import say_hy\n\nif __name__ == \"__main__\":\n    say_hy(\"dear Ruff contributor\")\n",
    )?;

    fs::write(
        tempdir.path().join("test.py"),
        "def say_hy(name: str):\r\n    print(f\"Hy {name}\")\r\n",
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--diff", "--isolated"])
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    2 files left unchanged
    "###);
    Ok(())
}

#[test]
fn exclude() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-exclude = ["out"]

[format]
exclude = ["test.py", "generated.py"]
"#,
    )?;

    fs::write(
        tempdir.path().join("main.py"),
        r#"
from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#,
    )?;

    // Excluded file but passed to the CLI directly, should be formatted
    let test_path = tempdir.path().join("test.py");
    fs::write(
        &test_path,
        r#"
def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    fs::write(
        tempdir.path().join("generated.py"),
        r#"NUMBERS = [
     0,  1,  2,  3,  4,  5,  6,  7,  8,  9,
    10, 11, 12, 13, 14, 15, 16, 17, 18, 19
]
OTHER = "OTHER"
"#,
    )?;

    let out_dir = tempdir.path().join("out");
    fs::create_dir(&out_dir)?;

    fs::write(out_dir.join("a.py"), "a = a")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--check", "--config"])
        .arg(ruff_toml.file_name().unwrap())
        // Explicitly pass test.py, should be formatted regardless of it being excluded by format.exclude
        .arg(test_path.file_name().unwrap())
        // Format all other files in the directory, should respect the `exclude` and `format.exclude` options
        .arg("."), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    Would reformat: test.py
    2 files would be reformatted

    ----- stderr -----
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    "###);
    Ok(())
}

#[test]
fn exclude_stdin() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]

[format]
exclude = ["generated.py"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--config", &ruff_toml.file_name().unwrap().to_string_lossy(), "--stdin-filename", "generated.py", "-"])
        .pass_stdin(r#"
from test import say_hy

if __name__ == '__main__':
    say_hy("dear Ruff contributor")
"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

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

#[test]
fn test_diff() {
    let args = ["format", "--isolated", "--diff"];
    let fixtures = Path::new("resources").join("test").join("fixtures");
    let paths = [
        fixtures.join("unformatted.py"),
        fixtures.join("formatted.py"),
        fixtures.join("unformatted.ipynb"),
    ];
    insta::with_settings!({filters => vec![
        // Replace windows paths
        (r"\\", "/"),
    ]}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME)).args(args).args(paths),
            @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        --- resources/test/fixtures/unformatted.ipynb
        +++ resources/test/fixtures/unformatted.ipynb
        @@ -1,3 +1,4 @@
         import numpy
        -maths = (numpy.arange(100)**2).sum()
        -stats= numpy.asarray([1,2,3,4]).median()
        +
        +maths = (numpy.arange(100) ** 2).sum()
        +stats = numpy.asarray([1, 2, 3, 4]).median()
        --- resources/test/fixtures/unformatted.py
        +++ resources/test/fixtures/unformatted.py
        @@ -1,3 +1,3 @@
         x = 1
        -y=2
        +y = 2
         z = 3

        ----- stderr -----
        warning: `ruff format` is not yet stable, and subject to change in future versions.
        2 files would be reformatted, 1 file left unchanged
        "###);
    });
}

#[test]
fn test_diff_no_change() {
    let args = ["format", "--isolated", "--diff"];
    let fixtures = Path::new("resources").join("test").join("fixtures");
    let paths = [fixtures.join("unformatted.py")];
    insta::with_settings!({filters => vec![
        // Replace windows paths
        (r"\\", "/"),
    ]}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME)).args(args).args(paths),
            @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        --- resources/test/fixtures/unformatted.py
        +++ resources/test/fixtures/unformatted.py
        @@ -1,3 +1,3 @@
         x = 1
        -y=2
        +y = 2
         z = 3

        ----- stderr -----
        warning: `ruff format` is not yet stable, and subject to change in future versions.
        1 file would be reformatted
        "###
        );
    });
}

#[test]
fn test_diff_stdin_unformatted() {
    let args = [
        "format",
        "--isolated",
        "--diff",
        "-",
        "--stdin-filename",
        "unformatted.py",
    ];
    let fixtures = Path::new("resources").join("test").join("fixtures");
    let unformatted = fs::read(fixtures.join("unformatted.py")).unwrap();
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).args(args).pass_stdin(unformatted),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    --- unformatted.py
    +++ unformatted.py
    @@ -1,3 +1,3 @@
     x = 1
    -y=2
    +y = 2
     z = 3

    ----- stderr -----
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    "###);
}

#[test]
fn test_diff_stdin_formatted() {
    let args = ["format", "--isolated", "--diff", "-"];
    let fixtures = Path::new("resources").join("test").join("fixtures");
    let unformatted = fs::read(fixtures.join("formatted.py")).unwrap();
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).args(args).pass_stdin(unformatted),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `ruff format` is not yet stable, and subject to change in future versions.
    "###);
}
