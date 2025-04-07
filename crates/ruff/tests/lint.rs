//! Tests the interaction of the `lint` configuration section

#![cfg(not(target_family = "wasm"))]

use regex::escape;
use std::process::Command;
use std::str;
use std::{fs, path::Path};

use anyhow::Result;
use assert_fs::fixture::{ChildPath, FileTouch, PathChild};
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";
const STDIN_BASE_OPTIONS: &[&str] = &["check", "--no-cache", "--output-format", "concise"];

fn tempdir_filter(path: impl AsRef<Path>) -> String {
    format!(r"{}\\?/?", escape(path.as_ref().to_str().unwrap()))
}

#[test]
fn top_level_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]

[flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"a = "abcba".strip("aba")"#), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:5: Q000 [*] Double quotes found but single quotes preferred
        test.py:1:5: B005 Using `.strip()` with multi-character strings is misleading
        test.py:1:19: Q000 [*] Double quotes found but single quotes preferred
        Found 3 errors.
        [*] 2 fixable with the `--fix` option.

        ----- stderr -----
        warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `[TMP]/ruff.toml`:
          - 'extend-select' -> 'lint.extend-select'
          - 'flake8-quotes' -> 'lint.flake8-quotes'
        ");
    });

    Ok(())
}

#[test]
fn lint_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:5: Q000 [*] Double quotes found but single quotes preferred
    -:1:5: B005 Using `.strip()` with multi-character strings is misleading
    -:1:19: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    ");
    });

    Ok(())
}

/// Tests that configurations from the top-level and `lint` section are merged together.
#[test]
fn mixed_levels() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:5: Q000 [*] Double quotes found but single quotes preferred
    -:1:5: B005 Using `.strip()` with multi-character strings is misleading
    -:1:19: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `[TMP]/ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");
    });

    Ok(())
}

/// Tests that options in the `lint` section have higher precedence than top-level options (because they are more specific).
#[test]
fn precedence() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
extend-select = ["B", "Q"]

[flake8-quotes]
inline-quotes = "double"

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:5: Q000 [*] Double quotes found but single quotes preferred
    -:1:5: B005 Using `.strip()` with multi-character strings is misleading
    -:1:19: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `[TMP]/ruff.toml`:
      - 'flake8-quotes' -> 'lint.flake8-quotes'
    ");
    });

    Ok(())
}

#[test]
fn exclude() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]
extend-exclude = ["out"]

[lint]
exclude = ["test.py", "generated.py"]

[lint.flake8-quotes]
inline-quotes = "single"
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

    // Excluded file but passed to the CLI directly, should be linted
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

    fs::write(out_dir.join("a.py"), r#"a = "a""#)?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        // Explicitly pass test.py, should be linted regardless of it being excluded by lint.exclude
        .arg(test_path.file_name().unwrap())
        // Lint all other files in the directory, should respect the `exclude` and `lint.exclude` options
        .arg("."), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:4:16: Q000 [*] Double quotes found but single quotes preferred
    main.py:5:12: Q000 [*] Double quotes found but single quotes preferred
    test.py:3:15: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 3 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");
    });

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

[lint]
exclude = ["generated.py"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .args(["--stdin-filename", "generated.py"])
        .arg("-")
        .pass_stdin(r#"
from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    generated.py:4:16: Q000 [*] Double quotes found but single quotes preferred
    generated.py:5:12: Q000 [*] Double quotes found but single quotes preferred
    Found 2 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");
    });

    Ok(())
}

#[test]
fn line_too_long_width_override() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
line-length = 80
select = ["E501"]

[pycodestyle]
max-line-length = 100
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .args(["--stdin-filename", "test.py"])
        .arg("-")
        .pass_stdin(r#"
# longer than 80, but less than 100
_ = "---------------------------------------------------------------------------亜亜亜亜亜亜"
# longer than 100
_ = "---------------------------------------------------------------------------亜亜亜亜亜亜亜亜亜亜亜亜亜亜"
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:5:91: E501 Line too long (109 > 100)
    Found 1 error.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `[TMP]/ruff.toml`:
      - 'select' -> 'lint.select'
      - 'pycodestyle' -> 'lint.pycodestyle'
    ");
    });

    Ok(())
}

#[test]
fn per_file_ignores_stdin() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .args(["--stdin-filename", "generated.py"])
        .args(["--per-file-ignores", "generated.py:Q"])
        .arg("-")
        .pass_stdin(r#"
import os

from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    generated.py:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");
    });

    Ok(())
}

#[test]
fn extend_per_file_ignores_stdin() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .args(["--stdin-filename", "generated.py"])
        .args(["--extend-per-file-ignores", "generated.py:Q"])
        .arg("-")
        .pass_stdin(r#"
import os

from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    generated.py:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");
    });

    Ok(())
}

/// Regression test for [#8858](https://github.com/astral-sh/ruff/issues/8858)
#[test]
fn parent_configuration_override() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root_ruff = tempdir.path().join("ruff.toml");
    fs::write(
        root_ruff,
        r#"
[lint]
select = ["ALL"]
"#,
    )?;

    let sub_dir = tempdir.path().join("subdirectory");
    fs::create_dir(&sub_dir)?;

    let subdirectory_ruff = sub_dir.join("ruff.toml");
    fs::write(
        subdirectory_ruff,
        r#"
[lint]
ignore = ["D203", "D212"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(sub_dir)
        .args(STDIN_BASE_OPTIONS)
        , @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: No Python files found under the given path(s)
    ");
    });

    Ok(())
}

#[test]
fn nonexistent_config_file() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "foo.toml", "."]), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'foo.toml' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    It looks like you were trying to pass a path to a configuration file.
    The path `foo.toml` does not point to a configuration file

    For more information, try '--help'.
    ");
}

#[test]
fn config_override_rejected_if_invalid_toml() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "foo = bar", "."]), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'foo = bar' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    The supplied argument is not valid TOML:

    TOML parse error at line 1, column 7
      |
    1 | foo = bar
      |       ^
    invalid string
    expected `"`, `'`

    For more information, try '--help'.
    "#);
}

#[test]
fn too_many_config_files() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_dot_toml = tempdir.path().join("ruff.toml");
    let ruff2_dot_toml = tempdir.path().join("ruff2.toml");
    fs::File::create(&ruff_dot_toml)?;
    fs::File::create(&ruff2_dot_toml)?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_dot_toml)
        .arg("--config")
        .arg(&ruff2_dot_toml)
        .arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: You cannot specify more than one configuration file on the command line.

      tip: remove either `--config=[TMP]/ruff.toml` or `--config=[TMP]/ruff2.toml`.
           For more information, try `--help`.
    ");
    });
    Ok(())
}

#[test]
fn extend_passed_via_config_argument() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "extend = 'foo.toml'", "."]), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'extend = 'foo.toml'' for '--config <CONFIG_OPTION>'

      tip: Cannot include `extend` in a --config flag value

    For more information, try '--help'.
    ");
}

#[test]
fn nonexistent_extend_file() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    fs::write(
        project_dir.join("ruff.toml"),
        r#"
extend = "ruff2.toml"
"#,
    )?;

    fs::write(
        project_dir.join("ruff2.toml"),
        r#"
extend = "ruff3.toml"
"#,
    )?;

    insta::with_settings!({
        filters => vec![
            (tempdir_filter(&project_dir).as_str(), "[TMP]/"),
            ("The system cannot find the file specified.", "No such file or directory")
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check"]).current_dir(project_dir), @r"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to load extended configuration `[TMP]/ruff3.toml` (`[TMP]/ruff.toml` extends `[TMP]/ruff2.toml` extends `[TMP]/ruff3.toml`)
          Cause: Failed to read [TMP]/ruff3.toml
          Cause: No such file or directory (os error 2)
        ");
    });

    Ok(())
}

#[test]
fn circular_extend() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_path = tempdir.path().canonicalize()?;

    fs::write(
        project_path.join("ruff.toml"),
        r#"
extend = "ruff2.toml"
"#,
    )?;
    fs::write(
        project_path.join("ruff2.toml"),
        r#"
extend = "ruff3.toml"
"#,
    )?;
    fs::write(
        project_path.join("ruff3.toml"),
        r#"
extend = "ruff.toml"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_path).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(["check"])
            .current_dir(project_path),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Circular configuration detected: `[TMP]/ruff.toml` extends `[TMP]/ruff2.toml` extends `[TMP]/ruff3.toml` extends `[TMP]/ruff.toml`
    ");
    });

    Ok(())
}

#[test]
fn parse_error_extends() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_path = tempdir.path().canonicalize()?;

    fs::write(
        project_path.join("ruff.toml"),
        r#"
extend = "ruff2.toml"
"#,
    )?;
    fs::write(
        project_path.join("ruff2.toml"),
        r#"
[lint]
select = [E501]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_path).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(["check"])
            .current_dir(project_path),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Failed to load extended configuration `[TMP]/ruff2.toml` (`[TMP]/ruff.toml` extends `[TMP]/ruff2.toml`)
      Cause: Failed to parse [TMP]/ruff2.toml
      Cause: TOML parse error at line 3, column 11
      |
    3 | select = [E501]
      |           ^
    invalid array
    expected `]`
    ");
    });

    Ok(())
}

#[test]
fn config_file_and_isolated() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_dot_toml = tempdir.path().join("ruff.toml");
    fs::File::create(&ruff_dot_toml)?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_dot_toml)
        .arg("--isolated")
        .arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: The argument `--config=[TMP]/ruff.toml` cannot be used with `--isolated`

      tip: You cannot specify a configuration file and also specify `--isolated`,
           as `--isolated` causes ruff to ignore all configuration files.
           For more information, try `--help`.
    ");
    });
    Ok(())
}

#[test]
fn config_override_via_cli() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
line-length = 100

[lint]
select = ["I"]

[lint.isort]
combine-as-imports = true
        "#,
    )?;
    let fixture = r#"
from foo import (
    aaaaaaaaaaaaaaaaaaa,
    bbbbbbbbbbb as bbbbbbbbbbbbbbbb,
    cccccccccccccccc,
    ddddddddddd as ddddddddddddd,
    eeeeeeeeeeeeeee,
    ffffffffffff as ffffffffffffff,
    ggggggggggggg,
    hhhhhhh as hhhhhhhhhhh,
    iiiiiiiiiiiiii,
    jjjjjjjjjjjjj as jjjjjj,
)

x = "longer_than_90_charactersssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss"
"#;
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .args(["--config", "line-length=90"])
        .args(["--config", "lint.extend-select=['E501', 'F841']"])
        .args(["--config", "lint.isort.combine-as-imports = false"])
        .arg("-")
        .pass_stdin(fixture), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:1: I001 [*] Import block is un-sorted or un-formatted
    -:15:91: E501 Line too long (97 > 90)
    Found 2 errors.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn valid_toml_but_nonexistent_option_provided_via_config_argument() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args([".", "--config", "extend-select=['F481']"]),  // No such code as F481!
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'extend-select=['F481']' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    Could not parse the supplied argument as a `ruff.toml` configuration option:

    Unknown rule selector: `F481`

    For more information, try '--help'.
    ");
}

#[test]
fn each_toml_option_requires_a_new_flag_1() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        // commas can't be used to delimit different config overrides;
        // you need a new --config flag for each override
        .args([".", "--config", "extend-select=['F841'], line-length=90"]),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'extend-select=['F841'], line-length=90' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    The supplied argument is not valid TOML:

    TOML parse error at line 1, column 23
      |
    1 | extend-select=['F841'], line-length=90
      |                       ^
    expected newline, `#`

    For more information, try '--help'.
    ");
}

#[test]
fn each_toml_option_requires_a_new_flag_2() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        // spaces *also* can't be used to delimit different config overrides;
        // you need a new --config flag for each override
        .args([".", "--config", "extend-select=['F841'] line-length=90"]),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'extend-select=['F841'] line-length=90' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    The supplied argument is not valid TOML:

    TOML parse error at line 1, column 24
      |
    1 | extend-select=['F841'] line-length=90
      |                        ^
    expected newline, `#`

    For more information, try '--help'.
    ");
}

#[test]
fn value_given_to_table_key_is_not_inline_table_1() {
    // https://github.com/astral-sh/ruff/issues/13995
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args([".", "--config", r#"lint.flake8-pytest-style="csv""#]),
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'lint.flake8-pytest-style="csv"' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    `lint.flake8-pytest-style` is a table of configuration options.
    Did you want to override one of the table's subkeys?

    Possible choices:

    - `lint.flake8-pytest-style.fixture-parentheses`
    - `lint.flake8-pytest-style.parametrize-names-type`
    - `lint.flake8-pytest-style.parametrize-values-type`
    - `lint.flake8-pytest-style.parametrize-values-row-type`
    - `lint.flake8-pytest-style.raises-require-match-for`
    - `lint.flake8-pytest-style.raises-extend-require-match-for`
    - `lint.flake8-pytest-style.mark-parentheses`
    - `lint.flake8-pytest-style.warns-require-match-for`
    - `lint.flake8-pytest-style.warns-extend-require-match-for`

    For more information, try '--help'.
    "#);
}

#[test]
fn value_given_to_table_key_is_not_inline_table_2() {
    // https://github.com/astral-sh/ruff/issues/13995
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args([".", "--config", r#"lint=123"#]),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'lint=123' for '--config <CONFIG_OPTION>'

      tip: A `--config` flag must either be a path to a `.toml` configuration file
           or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
           option

    `lint` is a table of configuration options.
    Did you want to override one of the table's subkeys?

    Possible choices:

    - `lint.allowed-confusables`
    - `lint.dummy-variable-rgx`
    - `lint.extend-ignore`
    - `lint.extend-select`
    - `lint.extend-fixable`
    - `lint.external`
    - `lint.fixable`
    - `lint.ignore`
    - `lint.extend-safe-fixes`
    - `lint.extend-unsafe-fixes`
    - `lint.ignore-init-module-imports`
    - `lint.logger-objects`
    - `lint.select`
    - `lint.explicit-preview-rules`
    - `lint.task-tags`
    - `lint.typing-modules`
    - `lint.unfixable`
    - `lint.per-file-ignores`
    - `lint.extend-per-file-ignores`
    - `lint.exclude`
    - `lint.preview`

    For more information, try '--help'.
    ");
}

#[test]
fn config_doubly_overridden_via_cli() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
line-length = 100

[lint]
select=["E501"]
"#,
    )?;
    let fixture = "x = 'longer_than_90_charactersssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss'";
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        // The --line-length flag takes priority over both the config file
        // and the `--config="line-length=110"` flag,
        // despite them both being specified after this flag on the command line:
        .args(["--line-length", "90"])
        .arg("--config")
        .arg(&ruff_toml)
        .args(["--config", "line-length=110"])
        .arg("-")
        .pass_stdin(fixture), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:91: E501 Line too long (97 > 90)
    Found 1 error.

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn complex_config_setting_overridden_via_cli() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(&ruff_toml, "lint.select = ['N801']")?;
    let fixture = "class violates_n801: pass";
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .args(["--config", "lint.per-file-ignores = {'generated.py' = ['N801']}"])
        .args(["--stdin-filename", "generated.py"])
        .arg("-")
        .pass_stdin(fixture), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn deprecated_config_option_overridden_via_cli() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "select=['N801']", "-"])
        .pass_stdin("class lowercase: ..."),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:7: N801 Class name `lowercase` should use CapWords convention
    Found 1 error.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in your `--config` CLI arguments:
      - 'select' -> 'lint.select'
    ");
}

#[test]
fn extension() -> Result<()> {
    let tempdir = TempDir::new()?;

    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
include = ["*.ipy"]
"#,
    )?;

    fs::write(
        tempdir.path().join("main.ipy"),
        r#"
{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "ad6f36d9-4b7d-4562-8d00-f15a0f1fbb6d",
   "metadata": {},
   "outputs": [],
   "source": [
    "import os"
   ]
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3 (ipykernel)",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "codemirror_mode": {
    "name": "ipython",
    "version": 3
   },
   "file_extension": ".py",
   "mimetype": "text/x-python",
   "name": "python",
   "nbconvert_exporter": "python",
   "pygments_lexer": "ipython3",
   "version": "3.12.0"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .args(["--extension", "ipy:ipynb"])
        .arg("."), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.ipy:cell 1:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    });

    Ok(())
}

#[test]
fn warn_invalid_noqa_with_no_diagnostics() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--isolated"])
        .arg("--select")
        .arg("F401")
        .arg("-")
        .pass_stdin(
            r#"
# ruff: noqa: AAA101
print("Hello world!")
"#
        ));
}

#[test]
fn file_noqa_external() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
external = ["AAA"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
# flake8: noqa: AAA101, BBB102
import os
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:3:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: Invalid rule code provided to `# ruff: noqa` at -:2: BBB102
    ");
    });

    Ok(())
}

#[test]
fn required_version_exact_mismatch() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
required-version = "0.1.0"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/"), (version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
import os
"#), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Required version `==0.1.0` does not match the running version `[VERSION]`
    ");
    });

    Ok(())
}

#[test]
fn required_version_exact_match() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        format!(
            r#"
required-version = "{version}"
"#
        ),
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/"), (version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
import os
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    });

    Ok(())
}

#[test]
fn required_version_bound_mismatch() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        format!(
            r#"
required-version = ">{version}"
"#
        ),
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/"), (version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
import os
"#), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Required version `>[VERSION]` does not match the running version `[VERSION]`
    ");
    });

    Ok(())
}

#[test]
fn required_version_bound_match() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
required-version = ">=0.1.0"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/"), (version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
import os
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    });

    Ok(())
}

/// Expand environment variables in `--config` paths provided via the CLI.
#[test]
fn config_expand() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        ruff_toml,
        r#"
[lint]
select = ["F"]
ignore = ["F841"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg("${NAME}.toml")
        .env("NAME", "ruff")
        .arg("-")
        .current_dir(tempdir.path())
        .pass_stdin(r#"
import os

def func():
    x = 1
"#), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");

    Ok(())
}

/// Per-file selects via ! negation in per-file-ignores
#[test]
fn negated_per_file_ignores() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.per-file-ignores]
"!selected.py" = ["RUF"]
"#,
    )?;
    let selected = tempdir.path().join("selected.py");
    fs::write(selected, "")?;
    let ignored = tempdir.path().join("ignored.py");
    fs::write(ignored, "")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("--select")
        .arg("RUF901")
        .current_dir(&tempdir)
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----
    selected.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn negated_per_file_ignores_absolute() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.per-file-ignores]
"!src/**.py" = ["RUF"]
"#,
    )?;
    let src_dir = tempdir.path().join("src");
    fs::create_dir(&src_dir)?;
    let selected = src_dir.join("selected.py");
    fs::write(selected, "")?;
    let ignored = tempdir.path().join("ignored.py");
    fs::write(ignored, "")?;

    insta::with_settings!({filters => vec![(r"\\", "/")]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
            .arg("--select")
            .arg("RUF901")
            .current_dir(&tempdir)
            , @r"
        success: false
        exit_code: 1
        ----- stdout -----
        src/selected.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        ");
    });
    Ok(())
}

/// patterns are additive, can't use negative patterns to "un-ignore"
#[test]
fn negated_per_file_ignores_overlap() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.per-file-ignores]
"*.py" = ["RUF"]
"!foo.py" = ["RUF"]
"#,
    )?;
    let foo_file = tempdir.path().join("foo.py");
    fs::write(foo_file, "")?;
    let bar_file = tempdir.path().join("bar.py");
    fs::write(bar_file, "")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(&ruff_toml)
        .arg("--select")
        .arg("RUF901")
        .current_dir(&tempdir)
        , @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn unused_interaction() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["F"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("--fix")
            .arg("-")
            .pass_stdin(r#"
import os  # F401

def function():
    import os  # F811
    print(os.name)
"#), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        import os  # F401

        def function():
            print(os.name)

        ----- stderr -----
        Found 1 error (1 fixed, 0 remaining).
        ");
    });

    Ok(())
}

#[test]
fn add_noqa() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["RUF015"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .arg(&test_path)
        .args(["--add-noqa"])
        .arg("-")
        .pass_stdin(r#"

"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def first_square():
        return [x * x for x in range(20)][0]  # noqa: RUF015
    ");

    Ok(())
}

#[test]
fn add_noqa_multiple_codes() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["ANN001", "ANN201", "ARG001", "D103"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
def unused(x):
    pass
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .arg(&test_path)
        .arg("--preview")
        .args(["--add-noqa"])
        .arg("-")
        .pass_stdin(r#"

"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def unused(x):  # noqa: ANN001, ANN201, D103
        pass
    ");

    Ok(())
}

#[test]
fn add_noqa_multiline_diagnostic() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["I"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
import z
import c
import a
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .arg(&test_path)
        .args(["--add-noqa"])
        .arg("-")
        .pass_stdin(r#"

"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    import z  # noqa: I001
    import c
    import a
    ");

    Ok(())
}

#[test]
fn add_noqa_existing_noqa() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["ANN001", "ANN201", "ARG001", "D103"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
def unused(x):  # noqa: ANN001, ARG001, D103
    pass
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .arg(&test_path)
        .arg("--preview")
        .args(["--add-noqa"])
        .arg("-")
        .pass_stdin(r#"

"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def unused(x):  # noqa: ANN001, ANN201, ARG001, D103
        pass
    ");

    Ok(())
}

#[test]
fn add_noqa_multiline_comment() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["UP031"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
print(
    """First line
    second line
    third line
      %s"""
    % name
)
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .arg(&test_path)
        .arg("--preview")
        .args(["--add-noqa"])
        .arg("-")
        .pass_stdin(r#"

"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r#"
    print(
        """First line
        second line
        third line
          %s"""  # noqa: UP031
        % name
    )
    "#);

    Ok(())
}

#[test]
fn add_noqa_exclude() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
exclude = ["excluded.py"]
select = ["RUF015"]
"#,
    )?;

    let test_path = tempdir.path().join("noqa.py");

    fs::write(
        &test_path,
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    let exclude_path = tempdir.path().join("excluded.py");

    fs::write(
        &exclude_path,
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .current_dir(tempdir.path())
            .args(STDIN_BASE_OPTIONS)
            .args(["--add-noqa"]), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Added 1 noqa directive.
        ");
    });

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &ruff_toml,
        r#"[project]
requires-python = ">= 3.11"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        ");
    });

    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.8"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&pyproject_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        All checks passed!

        ----- stderr -----
        ");
    });

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_patch() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.11.4"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&pyproject_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        ");
    });

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_equals() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = "== 3.11"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&pyproject_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        ");
    });

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_equals_patch() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = "== 3.11.4"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&pyproject_toml)
            .args(["--stdin-filename", "test.py"])
            .arg("-")
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        ");
    });

    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &ruff_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .args(["--select","UP007"])
            .arg("test.py")
            .arg("-")
            .current_dir(project_dir)
            , @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/test.py"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.11
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.11
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.11
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool_target_version_override() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &ruff_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .args(["--select","UP007"])
            .args(["--target-version","py310"])
            .arg("test.py")
            .arg("-")
            .current_dir(project_dir)
            , @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/test.py"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.10
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.10
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.10
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}
/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool_with_check() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &ruff_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .args(["--select","UP007"])
            .arg(".")
            .current_dir(project_dir)
            , @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP007 [*] Use `X | Y` for type annotations
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<-- no [tool.ruff]
/// ├── ruff.toml #<-- no `target-version`
/// └── test.py
/// ```
#[test]
fn requires_python_ruff_toml_no_target_fallback() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"[lint]
select = ["UP007"]
"#,
    )?;

    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("test.py")
            .arg("--show-settings")
            .current_dir(project_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/test.py"
        Settings path: "[TMP]/ruff.toml"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.11
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.11
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.11
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<-- no [tool.ruff]
/// ├── ruff.toml #<-- no `target-version`
/// └── test.py
/// ```
#[test]
fn requires_python_ruff_toml_no_target_fallback_check() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"[lint]
select = ["UP007"]
"#,
    )?;

    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg(".")
            .current_dir(project_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:2:31: UP007 [*] Use `X | Y` for type annotations
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── foo
/// │  ├── pyproject.toml #<-- no [tool.ruff], no `requires-python`
/// │  └── test.py
/// └── pyproject.toml #<-- no [tool.ruff], has `requires-python`
/// ```
#[test]
fn requires_python_pyproject_toml_above() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let outer_pyproject = tempdir.path().join("pyproject.toml");
    fs::write(
        &outer_pyproject,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let foodir = tempdir.path().join("foo");
    fs::create_dir(foodir)?;

    let inner_pyproject = tempdir.path().join("foo/pyproject.toml");
    fs::write(
        &inner_pyproject,
        r#"[project]
"#,
    )?;

    let testpy = tempdir.path().join("foo/test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    let testpy_canon = testpy.canonicalize()?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&testpy_canon).as_str(), "[TMP]/foo/test.py"),(tempdir_filter(&project_dir).as_str(), "[TMP]/"),(r"(?m)^foo\\test","foo/test")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .args(["--select","UP007"])
            .arg("foo/test.py")
            .current_dir(&project_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/foo/test.py"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.11
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.11
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.11
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── foo
/// │  ├── pyproject.toml #<-- has [tool.ruff], no `requires-python`
/// │  └── test.py
/// └── pyproject.toml #<-- no [tool.ruff], has `requires-python`
/// ```
#[test]
fn requires_python_pyproject_toml_above_with_tool() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let outer_pyproject = tempdir.path().join("pyproject.toml");
    fs::write(
        &outer_pyproject,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let foodir = tempdir.path().join("foo");
    fs::create_dir(foodir)?;

    let inner_pyproject = tempdir.path().join("foo/pyproject.toml");
    fs::write(
        &inner_pyproject,
        r#"
[tool.ruff]
target-version = "py310"        
"#,
    )?;

    let testpy = tempdir.path().join("foo/test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    let testpy_canon = testpy.canonicalize()?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&testpy_canon).as_str(), "[TMP]/foo/test.py"),(tempdir_filter(&project_dir).as_str(), "[TMP]/"),(r"foo\\","foo/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .args(["--select","UP007"])
            .arg("foo/test.py")
            .current_dir(&project_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/foo/test.py"

        # General Settings
        cache_dir = "[TMP]/foo/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/foo"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/foo"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.10
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/foo",
        	"[TMP]/foo/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.10
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.10
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── foo
/// │  ├── pyproject.toml #<-- no [tool.ruff]
/// │  └── test.py
/// └── ruff.toml #<-- no `target-version`
/// ```
#[test]
fn requires_python_ruff_toml_above() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["UP007"]
"#,
    )?;

    let foodir = tempdir.path().join("foo");
    fs::create_dir(foodir)?;

    let pyproject_toml = tempdir.path().join("foo/pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    let testpy = tempdir.path().join("foo/test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    let testpy_canon = testpy.canonicalize()?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&testpy_canon).as_str(), "[TMP]/foo/test.py"),(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .arg("foo/test.py")
            .current_dir(&project_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/foo/test.py"
        Settings path: "[TMP]/ruff.toml"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.9
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.9
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.9
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });

    insta::with_settings!({
        filters => vec![(tempdir_filter(&testpy_canon).as_str(), "[TMP]/foo/test.py"),(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .arg("test.py")
            .current_dir(project_dir.join("foo")), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/foo/test.py"
        Settings path: "[TMP]/ruff.toml"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.9
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.9
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.9
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });
    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml <-- requires >=3.10
/// ├── ruff.toml <--- extends base
/// ├── shared
/// │   └── base_config.toml <-- targets 3.11
/// └── test.py
/// ```
#[test]
fn requires_python_extend_from_shared_config() -> Result<()> {
    let tempdir = TempDir::new()?;
    let project_dir = tempdir.path().canonicalize()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend = "./shared/base_config.toml"
[lint]
select = ["UP007"]
"#,
    )?;

    let shared_dir = tempdir.path().join("shared");
    fs::create_dir(shared_dir)?;

    let pyproject_toml = tempdir.path().join("pyproject.toml");
    fs::write(
        &pyproject_toml,
        r#"[project]
requires-python = ">= 3.10"
"#,
    )?;

    let shared_toml = tempdir.path().join("shared/base_config.toml");
    fs::write(
        &shared_toml,
        r#"
target-version = "py311"
"#,
    )?;

    let testpy = tempdir.path().join("test.py");
    fs::write(
        &testpy,
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    let testpy_canon = testpy.canonicalize()?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&testpy_canon).as_str(), "[TMP]/test.py"),(tempdir_filter(&project_dir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--show-settings")
            .arg("test.py")
            .current_dir(&project_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Resolved settings for: "[TMP]/test.py"
        Settings path: "[TMP]/ruff.toml"

        # General Settings
        cache_dir = "[TMP]/.ruff_cache"
        fix = false
        fix_only = false
        output_format = concise
        show_fixes = false
        unsafe_fixes = hint

        # File Resolver Settings
        file_resolver.exclude = [
        	".bzr",
        	".direnv",
        	".eggs",
        	".git",
        	".git-rewrite",
        	".hg",
        	".ipynb_checkpoints",
        	".mypy_cache",
        	".nox",
        	".pants.d",
        	".pyenv",
        	".pytest_cache",
        	".pytype",
        	".ruff_cache",
        	".svn",
        	".tox",
        	".venv",
        	".vscode",
        	"__pypackages__",
        	"_build",
        	"buck-out",
        	"dist",
        	"node_modules",
        	"site-packages",
        	"venv",
        ]
        file_resolver.extend_exclude = []
        file_resolver.force_exclude = false
        file_resolver.include = [
        	"*.py",
        	"*.pyi",
        	"*.ipynb",
        	"**/pyproject.toml",
        ]
        file_resolver.extend_include = []
        file_resolver.respect_gitignore = true
        file_resolver.project_root = "[TMP]/"

        # Linter Settings
        linter.exclude = []
        linter.project_root = "[TMP]/"
        linter.rules.enabled = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.rules.should_fix = [
        	non-pep604-annotation-union (UP007),
        ]
        linter.per_file_ignores = {}
        linter.safety_table.forced_safe = []
        linter.safety_table.forced_unsafe = []
        linter.unresolved_target_version = 3.10
        linter.per_file_target_version = {}
        linter.preview = disabled
        linter.explicit_preview_rules = false
        linter.extension = ExtensionMapping({})
        linter.allowed_confusables = []
        linter.builtins = []
        linter.dummy_variable_rgx = ^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$
        linter.external = []
        linter.ignore_init_module_imports = true
        linter.logger_objects = []
        linter.namespace_packages = []
        linter.src = [
        	"[TMP]/",
        	"[TMP]/src",
        ]
        linter.tab_size = 4
        linter.line_length = 88
        linter.task_tags = [
        	TODO,
        	FIXME,
        	XXX,
        ]
        linter.typing_modules = []

        # Linter Plugins
        linter.flake8_annotations.mypy_init_return = false
        linter.flake8_annotations.suppress_dummy_args = false
        linter.flake8_annotations.suppress_none_returning = false
        linter.flake8_annotations.allow_star_arg_any = false
        linter.flake8_annotations.ignore_fully_untyped = false
        linter.flake8_bandit.hardcoded_tmp_directory = [
        	/tmp,
        	/var/tmp,
        	/dev/shm,
        ]
        linter.flake8_bandit.check_typed_exception = false
        linter.flake8_bandit.extend_markup_names = []
        linter.flake8_bandit.allowed_markup_calls = []
        linter.flake8_bugbear.extend_immutable_calls = []
        linter.flake8_builtins.allowed_modules = []
        linter.flake8_builtins.ignorelist = []
        linter.flake8_builtins.strict_checking = false
        linter.flake8_comprehensions.allow_dict_calls_with_keyword_arguments = false
        linter.flake8_copyright.notice_rgx = (?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*
        linter.flake8_copyright.author = none
        linter.flake8_copyright.min_file_size = 0
        linter.flake8_errmsg.max_string_length = 0
        linter.flake8_gettext.functions_names = [
        	_,
        	gettext,
        	ngettext,
        ]
        linter.flake8_implicit_str_concat.allow_multiline = true
        linter.flake8_import_conventions.aliases = {
        	altair = alt,
        	holoviews = hv,
        	matplotlib = mpl,
        	matplotlib.pyplot = plt,
        	networkx = nx,
        	numpy = np,
        	numpy.typing = npt,
        	pandas = pd,
        	panel = pn,
        	plotly.express = px,
        	polars = pl,
        	pyarrow = pa,
        	seaborn = sns,
        	tensorflow = tf,
        	tkinter = tk,
        	xml.etree.ElementTree = ET,
        }
        linter.flake8_import_conventions.banned_aliases = {}
        linter.flake8_import_conventions.banned_from = []
        linter.flake8_pytest_style.fixture_parentheses = false
        linter.flake8_pytest_style.parametrize_names_type = tuple
        linter.flake8_pytest_style.parametrize_values_type = list
        linter.flake8_pytest_style.parametrize_values_row_type = tuple
        linter.flake8_pytest_style.raises_require_match_for = [
        	BaseException,
        	Exception,
        	ValueError,
        	OSError,
        	IOError,
        	EnvironmentError,
        	socket.error,
        ]
        linter.flake8_pytest_style.raises_extend_require_match_for = []
        linter.flake8_pytest_style.mark_parentheses = false
        linter.flake8_quotes.inline_quotes = double
        linter.flake8_quotes.multiline_quotes = double
        linter.flake8_quotes.docstring_quotes = double
        linter.flake8_quotes.avoid_escape = true
        linter.flake8_self.ignore_names = [
        	_make,
        	_asdict,
        	_replace,
        	_fields,
        	_field_defaults,
        	_name_,
        	_value_,
        ]
        linter.flake8_tidy_imports.ban_relative_imports = "parents"
        linter.flake8_tidy_imports.banned_api = {}
        linter.flake8_tidy_imports.banned_module_level_imports = []
        linter.flake8_type_checking.strict = false
        linter.flake8_type_checking.exempt_modules = [
        	typing,
        	typing_extensions,
        ]
        linter.flake8_type_checking.runtime_required_base_classes = []
        linter.flake8_type_checking.runtime_required_decorators = []
        linter.flake8_type_checking.quote_annotations = false
        linter.flake8_unused_arguments.ignore_variadic_names = false
        linter.isort.required_imports = []
        linter.isort.combine_as_imports = false
        linter.isort.force_single_line = false
        linter.isort.force_sort_within_sections = false
        linter.isort.detect_same_package = true
        linter.isort.case_sensitive = false
        linter.isort.force_wrap_aliases = false
        linter.isort.force_to_top = []
        linter.isort.known_modules = {}
        linter.isort.order_by_type = true
        linter.isort.relative_imports_order = furthest_to_closest
        linter.isort.single_line_exclusions = []
        linter.isort.split_on_trailing_comma = true
        linter.isort.classes = []
        linter.isort.constants = []
        linter.isort.variables = []
        linter.isort.no_lines_before = []
        linter.isort.lines_after_imports = -1
        linter.isort.lines_between_types = 0
        linter.isort.forced_separate = []
        linter.isort.section_order = [
        	known { type = future },
        	known { type = standard_library },
        	known { type = third_party },
        	known { type = first_party },
        	known { type = local_folder },
        ]
        linter.isort.default_section = known { type = third_party }
        linter.isort.no_sections = false
        linter.isort.from_first = false
        linter.isort.length_sort = false
        linter.isort.length_sort_straight = false
        linter.mccabe.max_complexity = 10
        linter.pep8_naming.ignore_names = [
        	setUp,
        	tearDown,
        	setUpClass,
        	tearDownClass,
        	setUpModule,
        	tearDownModule,
        	asyncSetUp,
        	asyncTearDown,
        	setUpTestData,
        	failureException,
        	longMessage,
        	maxDiff,
        ]
        linter.pep8_naming.classmethod_decorators = []
        linter.pep8_naming.staticmethod_decorators = []
        linter.pycodestyle.max_line_length = 88
        linter.pycodestyle.max_doc_length = none
        linter.pycodestyle.ignore_overlong_task_comments = false
        linter.pyflakes.extend_generics = []
        linter.pyflakes.allowed_unused_imports = []
        linter.pylint.allow_magic_value_types = [
        	str,
        	bytes,
        ]
        linter.pylint.allow_dunder_method_names = []
        linter.pylint.max_args = 5
        linter.pylint.max_positional_args = 5
        linter.pylint.max_returns = 6
        linter.pylint.max_bool_expr = 5
        linter.pylint.max_branches = 12
        linter.pylint.max_statements = 50
        linter.pylint.max_public_methods = 20
        linter.pylint.max_locals = 15
        linter.pyupgrade.keep_runtime_typing = false
        linter.ruff.parenthesize_tuple_in_subscript = false

        # Formatter Settings
        formatter.exclude = []
        formatter.unresolved_target_version = 3.10
        formatter.per_file_target_version = {}
        formatter.preview = disabled
        formatter.line_width = 88
        formatter.line_ending = auto
        formatter.indent_style = space
        formatter.indent_width = 4
        formatter.quote_style = double
        formatter.magic_trailing_comma = respect
        formatter.docstring_code_format = disabled
        formatter.docstring_code_line_width = dynamic

        # Analyze Settings
        analyze.exclude = []
        analyze.preview = disabled
        analyze.target_version = 3.10
        analyze.detect_string_imports = false
        analyze.extension = ExtensionMapping({})
        analyze.include_dependencies = {}

        ----- stderr -----
        "###);
    });

    Ok(())
}

#[test]
fn checks_notebooks_in_stable() -> anyhow::Result<()> {
    let tempdir = TempDir::new()?;
    std::fs::write(
        tempdir.path().join("main.ipynb"),
        r#"
{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "ad6f36d9-4b7d-4562-8d00-f15a0f1fbb6d",
   "metadata": {},
   "outputs": [],
   "source": [
    "import random"
   ]
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3 (ipykernel)",
   "language": "python",
   "name": "python3"
  },
  "language_info": {
   "codemirror_mode": {
    "name": "ipython",
    "version": 3
   },
   "file_extension": ".py",
   "mimetype": "text/x-python",
   "name": "python",
   "nbconvert_exporter": "python",
   "pygments_lexer": "ipython3",
   "version": "3.12.0"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--select")
        .arg("F401")
        .current_dir(&tempdir)
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.ipynb:cell 1:1:8: F401 [*] `random` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}

/// Verify that implicit namespace packages are detected even when they are nested.
///
/// See: <https://github.com/astral-sh/ruff/issues/13519>
#[test]
fn nested_implicit_namespace_package() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("foo").child("__init__.py").touch()?;
    root.child("foo")
        .child("bar")
        .child("baz")
        .child("__init__.py")
        .touch()?;
    root.child("foo")
        .child("bar")
        .child("baz")
        .child("bop.py")
        .touch()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--select")
        .arg("INP")
        .current_dir(&tempdir)
        , @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    insta::with_settings!({filters => vec![(r"\\", "/")]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--select")
            .arg("INP")
            .arg("--preview")
            .current_dir(&tempdir)
            , @r"
        success: false
        exit_code: 1
        ----- stdout -----
        foo/bar/baz/__init__.py:1:1: INP001 File `foo/bar/baz/__init__.py` declares a package, but is nested under an implicit namespace package. Add an `__init__.py` to `foo/bar`.
        Found 1 error.

        ----- stderr -----
        ");
    });

    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_alias_name() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.flake8-import-conventions.aliases]
"module.name" = "invalid.alias"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
    , @r#"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to load configuration `[TMP]/ruff.toml`
          Cause: Failed to parse [TMP]/ruff.toml
          Cause: TOML parse error at line 3, column 17
          |
        3 | "module.name" = "invalid.alias"
          |                 ^^^^^^^^^^^^^^^
        invalid value: string "invalid.alias", expected a Python identifier
        "#);});
    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_extend_alias_name() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.flake8-import-conventions.extend-aliases]
"module.name" = "__debug__"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
    , @r#"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to load configuration `[TMP]/ruff.toml`
          Cause: Failed to parse [TMP]/ruff.toml
          Cause: TOML parse error at line 3, column 17
          |
        3 | "module.name" = "__debug__"
          |                 ^^^^^^^^^^^
        invalid value: string "__debug__", expected an assignable Python identifier
        "#);});
    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_module_name() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint.flake8-import-conventions.aliases]
"module..invalid" = "alias"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
    , @r#"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to load configuration `[TMP]/ruff.toml`
          Cause: Failed to parse [TMP]/ruff.toml
          Cause: TOML parse error at line 3, column 1
          |
        3 | "module..invalid" = "alias"
          | ^^^^^^^^^^^^^^^^^
        invalid value: string "module..invalid", expected a sequence of Python identifiers delimited by periods
        "#);});
    Ok(())
}

#[test]
fn flake8_import_convention_unused_aliased_import() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(r#"lint.isort.required-imports = ["import pandas"]"#)
        .args(["--select", "I002,ICN001,F401"])
        .args(["--stdin-filename", "test.py"])
        .arg("--unsafe-fixes")
        .arg("--fix")
        .arg("-")
        .pass_stdin("1"));
}

#[test]
fn flake8_import_convention_unused_aliased_import_no_conflict() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .arg("--config")
        .arg(r#"lint.isort.required-imports = ["import pandas as pd"]"#)
        .args(["--select", "I002,ICN001,F401"])
        .args(["--stdin-filename", "test.py"])
        .arg("--unsafe-fixes")
        .arg("--fix")
        .arg("-")
        .pass_stdin("1"));
}

/// Test that private, old-style `TypeVar` generics
/// 1. Get replaced with PEP 695 type parameters (UP046, UP047)
/// 2. Get renamed to remove leading underscores (UP049)
/// 3. Emit a warning that the standalone type variable is now unused (PYI018)
/// 4. Remove the now-unused `Generic` import
#[test]
fn pep695_generic_rename() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--select", "F401,PYI018,UP046,UP047,UP049"])
        .args(["--stdin-filename", "test.py"])
        .arg("--unsafe-fixes")
        .arg("--fix")
        .arg("--preview")
        .arg("--target-version=py312")
        .arg("-")
        .pass_stdin(
            r#"
from typing import Generic, TypeVar
_T = TypeVar("_T")

class OldStyle(Generic[_T]):
    var: _T

def func(t: _T) -> _T:
    x: _T
    return x
"#
        ),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----


    class OldStyle[T]:
        var: T

    def func[T](t: T) -> T:
        x: T
        return x

    ----- stderr -----
    Found 7 errors (7 fixed, 0 remaining).
    "
    );
}

/// Test that we do not rename two different type parameters to the same name
/// in one execution of Ruff (autofixing this to `class Foo[T, T]: ...` would
/// introduce invalid syntax)
#[test]
fn type_parameter_rename_isolation() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--select", "UP049"])
        .args(["--stdin-filename", "test.py"])
        .arg("--unsafe-fixes")
        .arg("--fix")
        .arg("--preview")
        .arg("--target-version=py312")
        .arg("-")
        .pass_stdin(
            r#"
class Foo[_T, __T]:
    pass
"#
        ),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----

    class Foo[T, __T]:
        pass

    ----- stderr -----
    test.py:2:14: UP049 Generic class uses private type parameters
    Found 2 errors (1 fixed, 1 remaining).
    "
    );
}

/// construct a directory tree with this structure:
/// .
/// ├── abc
/// │  └── __init__.py
/// ├── collections
/// │  ├── __init__.py
/// │  ├── abc
/// │  │  └── __init__.py
/// │  └── foobar
/// │      └── __init__.py
/// ├── foobar
/// │  ├── __init__.py
/// │  ├── abc
/// │  │  └── __init__.py
/// │  └── collections
/// │      ├── __init__.py
/// │      ├── abc
/// │      │  └── __init__.py
/// │      └── foobar
/// │          └── __init__.py
/// ├── ruff.toml
/// └── urlparse
///     └── __init__.py
fn create_a005_module_structure(tempdir: &TempDir) -> Result<()> {
    fn create_module(path: &Path) -> Result<()> {
        fs::create_dir(path)?;
        fs::File::create(path.join("__init__.py"))?;
        Ok(())
    }

    let foobar = tempdir.path().join("foobar");
    create_module(&foobar)?;
    for base in [&tempdir.path().into(), &foobar] {
        for dir in ["abc", "collections"] {
            create_module(&base.join(dir))?;
        }
        create_module(&base.join("collections").join("abc"))?;
        create_module(&base.join("collections").join("foobar"))?;
    }
    create_module(&tempdir.path().join("urlparse"))?;
    // also create a ruff.toml to mark the project root
    fs::File::create(tempdir.path().join("ruff.toml"))?;

    Ok(())
}

/// Test A005 with `strict-checking = true`
#[test]
fn a005_module_shadowing_strict() -> Result<()> {
    let tempdir = TempDir::new()?;
    create_a005_module_structure(&tempdir)?;

    insta::with_settings!({
        filters => vec![(r"\\", "/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(r#"lint.flake8-builtins.strict-checking = true"#)
            .args(["--select", "A005"])
            .current_dir(tempdir.path()),
            @r"
        success: false
        exit_code: 1
        ----- stdout -----
        abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
        collections/abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        foobar/abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        foobar/collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
        foobar/collections/abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        Found 6 errors.

        ----- stderr -----
        ");
    });

    Ok(())
}

/// Test A005 with `strict-checking = false`
#[test]
fn a005_module_shadowing_non_strict() -> Result<()> {
    let tempdir = TempDir::new()?;
    create_a005_module_structure(&tempdir)?;

    insta::with_settings!({
        filters => vec![(r"\\", "/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(r#"lint.flake8-builtins.strict-checking = false"#)
            .args(["--select", "A005"])
            .current_dir(tempdir.path()),
            @r"
        success: false
        exit_code: 1
        ----- stdout -----
        abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
        Found 2 errors.

        ----- stderr -----
        ");

    });

    Ok(())
}

/// Test A005 with `strict-checking` unset
/// TODO(brent) This should currently match the strict version, but after the next minor
/// release it will match the non-strict version directly above
#[test]
fn a005_module_shadowing_strict_default() -> Result<()> {
    let tempdir = TempDir::new()?;
    create_a005_module_structure(&tempdir)?;

    insta::with_settings!({
        filters => vec![(r"\\", "/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .args(["--select", "A005"])
            .current_dir(tempdir.path()),
            @r"
        success: false
        exit_code: 1
        ----- stdout -----
        abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
        collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
        Found 2 errors.

        ----- stderr -----
        ");
    });
    Ok(())
}

/// Test that the linter respects per-file-target-version.
#[test]
fn per_file_target_version_linter() {
    // without per-file-target-version, there should be one UP046 error
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--target-version", "py312"])
        .args(["--select", "UP046"]) // only triggers on 3.12+
        .args(["--stdin-filename", "test.py"])
        .arg("--preview")
        .arg("-")
        .pass_stdin(r#"
from typing import Generic, TypeVar

T = TypeVar("T")

class A(Generic[T]):
    var: T
"#),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:6:9: UP046 Generic class `A` uses `Generic` subclass instead of type parameters
    Found 1 error.
    No fixes available (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    "
    );

    // with per-file-target-version, there should be no errors because the new generic syntax is
    // unavailable
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--target-version", "py312"])
        .args(["--config", r#"per-file-target-version = {"test.py" = "py311"}"#])
        .args(["--select", "UP046"]) // only triggers on 3.12+
        .args(["--stdin-filename", "test.py"])
        .arg("--preview")
        .arg("-")
        .pass_stdin(r#"
from typing import Generic, TypeVar

T = TypeVar("T")

class A(Generic[T]):
    var: T
"#),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );
}

#[test]
fn walrus_before_py38() {
    // ok
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "test.py"])
        .arg("--target-version=py38")
        .arg("-")
        .pass_stdin(r#"(x := 1)"#),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    // not ok on 3.7 with preview
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "test.py"])
        .arg("--target-version=py37")
        .arg("--preview")
        .arg("-")
        .pass_stdin(r#"(x := 1)"#),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:1:2: SyntaxError: Cannot use named assignment expression (`:=`) on Python 3.7 (syntax was added in Python 3.8)
    Found 1 error.

    ----- stderr -----
    "
    );
}

#[test]
fn match_before_py310() {
    // ok on 3.10
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "test.py"])
        .arg("--target-version=py310")
        .arg("-")
        .pass_stdin(
            r#"
match 2:
    case 1:
        print("it's one")
"#
        ),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    // ok on 3.9 without preview
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "test.py"])
        .arg("--target-version=py39")
        .arg("-")
        .pass_stdin(
            r#"
match 2:
    case 1:
        print("it's one")
"#
        ),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    // syntax error on 3.9 with preview
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "test.py"])
        .arg("--target-version=py39")
        .arg("--preview")
        .arg("-")
        .pass_stdin(
            r#"
match 2:
    case 1:
        print("it's one")
"#
        ),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:2:1: SyntaxError: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)
    Found 1 error.

    ----- stderr -----
    "
    );
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/16417>
#[test]
fn cache_syntax_errors() -> Result<()> {
    let tempdir = TempDir::new()?;
    fs::write(tempdir.path().join("main.py"), "match 2:\n    case 1: ...")?;

    let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
    // inline STDIN_BASE_OPTIONS to remove --no-cache
    cmd.args(["check", "--output-format", "concise"])
        .arg("--target-version=py39")
        .arg("--preview")
        .arg("--quiet") // suppress `debug build without --no-cache` warnings
        .current_dir(&tempdir);

    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:1: SyntaxError: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)

    ----- stderr -----
    "
    );

    // this should *not* be cached, like normal parse errors
    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:1: SyntaxError: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)

    ----- stderr -----
    "
    );

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/9381> with very helpful
/// reproduction repo here: <https://github.com/lucasfijen/example_ruff_glob_bug>
#[test]
fn cookiecutter_globbing() -> Result<()> {
    // This is a simplified directory structure from the repo linked above. The essence of the
    // problem is this `{{cookiecutter.repo_name}}` directory containing a config file with a glob.
    // The absolute path of the glob contains the glob metacharacters `{{` and `}}` even though the
    // user's glob does not.
    let tempdir = TempDir::new()?;
    let cookiecutter = tempdir.path().join("{{cookiecutter.repo_name}}");
    let cookiecutter_toml = cookiecutter.join("pyproject.toml");
    let tests = cookiecutter.join("tests");
    fs::create_dir_all(&tests)?;
    fs::write(
        &cookiecutter_toml,
        r#"tool.ruff.lint.per-file-ignores = { "tests/*" = ["F811"] }"#,
    )?;
    // F811 example from the docs to ensure the glob still works
    let maintest = tests.join("maintest.py");
    fs::write(maintest, "import foo\nimport bar\nimport foo")?;

    insta::with_settings!({filters => vec![(r"\\", "/")]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
                .args(STDIN_BASE_OPTIONS)
                .arg("--select=F811")
                .current_dir(tempdir.path()), @r"
			success: true
			exit_code: 0
			----- stdout -----
			All checks passed!

			----- stderr -----
			");
    });

    // after removing the config file with the ignore, F811 applies, so the glob worked above
    fs::remove_file(cookiecutter_toml)?;

    insta::with_settings!({filters => vec![(r"\\", "/")]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
                .args(STDIN_BASE_OPTIONS)
                .arg("--select=F811")
                .current_dir(tempdir.path()), @r"
			success: false
			exit_code: 1
			----- stdout -----
			{{cookiecutter.repo_name}}/tests/maintest.py:3:8: F811 [*] Redefinition of unused `foo` from line 1
			Found 1 error.
			[*] 1 fixable with the `--fix` option.

			----- stderr -----
		");
    });

    Ok(())
}

/// Like the test above but exercises the non-absolute path case in `PerFile::new`
#[test]
fn cookiecutter_globbing_no_project_root() -> Result<()> {
    let tempdir = TempDir::new()?;
    let tempdir = tempdir.path().join("{{cookiecutter.repo_name}}");
    fs::create_dir(&tempdir)?;

    insta::with_settings!({filters => vec![(r"\\", "/")]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .current_dir(&tempdir)
            .args(STDIN_BASE_OPTIONS)
            .args(["--extend-per-file-ignores", "generated.py:Q"]), @r"
		success: true
		exit_code: 0
		----- stdout -----
		All checks passed!

		----- stderr -----
		warning: No Python files found under the given path(s)
		");
    });

    Ok(())
}

/// Test that semantic syntax errors (1) are emitted, (2) are not cached, (3) don't affect the
/// reporting of normal diagnostics, and (4) are not suppressed by `select = []` (or otherwise
/// disabling all AST-based rules).
#[test]
fn semantic_syntax_errors() -> Result<()> {
    let tempdir = TempDir::new()?;
    let contents = "[(x := 1) for x in foo]";
    fs::write(tempdir.path().join("main.py"), contents)?;

    let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
    // inline STDIN_BASE_OPTIONS to remove --no-cache
    cmd.args(["check", "--output-format", "concise"])
        .arg("--preview")
        .arg("--quiet") // suppress `debug build without --no-cache` warnings
        .current_dir(&tempdir);

    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:3: SyntaxError: assignment expression cannot rebind comprehension variable
    main.py:1:20: F821 Undefined name `foo`

    ----- stderr -----
    "
    );

    // this should *not* be cached, like normal parse errors
    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:3: SyntaxError: assignment expression cannot rebind comprehension variable
    main.py:1:20: F821 Undefined name `foo`

    ----- stderr -----
    "
    );

    // ensure semantic errors are caught even without AST-based rules selected
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .args(["--config", "lint.select = []"])
            .arg("--preview")
            .arg("-")
            .pass_stdin(contents),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:3: SyntaxError: assignment expression cannot rebind comprehension variable
    Found 1 error.

    ----- stderr -----
    "
    );

    Ok(())
}
