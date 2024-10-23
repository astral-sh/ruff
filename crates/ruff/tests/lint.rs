//! Tests the interaction of the `lint` configuration section

#![cfg(not(target_family = "wasm"))]

use regex::escape;
use std::fs;
use std::process::Command;
use std::str;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";
const STDIN_BASE_OPTIONS: &[&str] = &["check", "--no-cache", "--output-format", "concise"];

fn tempdir_filter(tempdir: &TempDir) -> String {
    format!(r"{}\\?/?", escape(tempdir.path().to_str().unwrap()))
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
            .pass_stdin(r#"a = "abcba".strip("aba")"#), @r###"
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
        "###);
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
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:5: Q000 [*] Double quotes found but single quotes preferred
    -:1:5: B005 Using `.strip()` with multi-character strings is misleading
    -:1:19: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 2 fixable with the `--fix` option.

    ----- stderr -----
    "###);
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
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r###"
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
    "###);
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
        .pass_stdin(r#"a = "abcba".strip("aba")"#), @r###"
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
    "###);
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
        .arg("."), @r###"
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
    "###);
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
"#), @r###"
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
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:5:91: E501 Line too long (109 > 100)
    Found 1 error.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `[TMP]/ruff.toml`:
      - 'select' -> 'lint.select'
      - 'pycodestyle' -> 'lint.pycodestyle'
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    generated.py:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    generated.py:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    "###);
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
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: No Python files found under the given path(s)
    "###);
    });

    Ok(())
}

#[test]
fn nonexistent_config_file() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "foo.toml", "."]), @r###"
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
    "###);
}

#[test]
fn config_override_rejected_if_invalid_toml() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "foo = bar", "."]), @r###"
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
    "###);
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
        .arg("."), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: You cannot specify more than one configuration file on the command line.

          tip: remove either `--config=[TMP]/ruff.toml` or `--config=[TMP]/ruff2.toml`.
               For more information, try `--help`.

        "###);
    });
    Ok(())
}

#[test]
fn extend_passed_via_config_argument() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "extend = 'foo.toml'", "."]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'extend = 'foo.toml'' for '--config <CONFIG_OPTION>'

      tip: Cannot include `extend` in a --config flag value

    For more information, try '--help'.
    "###);
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
        .arg("."), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: The argument `--config=[TMP]/ruff.toml` cannot be used with `--isolated`

          tip: You cannot specify a configuration file and also specify `--isolated`,
               as `--isolated` causes ruff to ignore all configuration files.
               For more information, try `--help`.

        "###);
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
        .pass_stdin(fixture), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:1: I001 [*] Import block is un-sorted or un-formatted
    -:15:91: E501 Line too long (97 > 90)
    Found 2 errors.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn valid_toml_but_nonexistent_option_provided_via_config_argument() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args([".", "--config", "extend-select=['F481']"]),  // No such code as F481!
        @r###"
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
    "###);
}

#[test]
fn each_toml_option_requires_a_new_flag_1() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        // commas can't be used to delimit different config overrides;
        // you need a new --config flag for each override
        .args([".", "--config", "extend-select=['F841'], line-length=90"]),
        @r###"
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
    "###);
}

#[test]
fn each_toml_option_requires_a_new_flag_2() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        // spaces *also* can't be used to delimit different config overrides;
        // you need a new --config flag for each override
        .args([".", "--config", "extend-select=['F841'] line-length=90"]),
        @r###"
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
    "###);
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
        .pass_stdin(fixture), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:91: E501 Line too long (97 > 90)
    Found 1 error.

    ----- stderr -----
    "###);
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
        .pass_stdin(fixture), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn deprecated_config_option_overridden_via_cli() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--config", "select=['N801']", "-"])
        .pass_stdin("class lowercase: ..."),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:7: N801 Class name `lowercase` should use CapWords convention
    Found 1 error.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in your `--config` CLI arguments:
      - 'select' -> 'lint.select'
    "###);
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
        .arg("."), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    main.ipy:cell 1:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    });

    Ok(())
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:3:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    warning: Invalid rule code provided to `# ruff: noqa` at -:2: BBB102
    "###);
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
"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Required version `==0.1.0` does not match the running version `[VERSION]`
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
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
"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Required version `>[VERSION]` does not match the running version `[VERSION]`
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
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
"#), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:2:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);

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
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    selected.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
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

    insta::with_settings!({filters => vec![
        // Replace windows paths
        (r"\\", "/"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(&ruff_toml)
            .arg("--select")
            .arg("RUF901")
            .current_dir(&tempdir)
            , @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        src/selected.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
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
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);
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
"#), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        import os  # F401

        def function():
            print(os.name)

        ----- stderr -----
        Found 1 error (1 fixed, 0 remaining).
        "###);
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

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    "###);
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r###"
    def first_square():
        return [x * x for x in range(20)][0]  # noqa: RUF015
    "###);

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

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    "###);
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r###"

    def unused(x):  # noqa: ANN001, ANN201, D103
        pass
    "###);

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

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    "###);
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r###"

    import z  # noqa: I001
    import c
    import a
    "###);

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

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    "###);
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r###"

    def unused(x):  # noqa: ANN001, ANN201, ARG001, D103
        pass
    "###);

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

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    "###);
    });

    let test_code = std::fs::read_to_string(&test_path).expect("should read test file");

    insta::assert_snapshot!(test_code, @r###"
    print(
        """First line
        second line
        third line
          %s"""  # noqa: UP031
        % name
    )
    "###);

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
            .args(["--add-noqa"]), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Added 1 noqa directive.
        "###);
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
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
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
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        All checks passed!

        ----- stderr -----
        "###);
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
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
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
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "###);
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
            .pass_stdin(r#"from typing import List; foo: List[int]"#), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        test.py:1:31: UP006 [*] Use `list` instead of `List` for type annotation
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

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
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    main.ipynb:cell 1:1:8: F401 [*] `random` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    Ok(())
}
