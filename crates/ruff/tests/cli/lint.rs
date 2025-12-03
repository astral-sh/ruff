//! Tests the interaction of the `lint` configuration section

use std::fs;
use std::process::Command;
use std::str;

use anyhow::Result;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use crate::CliTest;

const BIN_NAME: &str = "ruff";
const STDIN_BASE_OPTIONS: &[&str] = &["check", "--no-cache", "--output-format", "concise"];

impl CliTest {
    fn check_command(&self) -> Command {
        let mut command = self.command();
        command.args(STDIN_BASE_OPTIONS);
        command
    }
}

#[test]
fn top_level_options() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]

[flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(test.check_command()
            .arg("--config")
            .arg("ruff.toml")
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
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
      - 'flake8-quotes' -> 'lint.flake8-quotes'
    ");

    Ok(())
}

#[test]
fn lint_options() -> Result<()> {
    let case = CliTest::with_file(
        "ruff.toml",
        r#"
[lint]
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(
        case.check_command()
            .arg("--config")
            .arg("ruff.toml")
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

    Ok(())
}

/// Tests that configurations from the top-level and `lint` section are merged together.
#[test]
fn mixed_levels() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(test.check_command()
        .arg("--config")
        .arg("ruff.toml")
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
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");

    Ok(())
}

/// Tests that options in the `lint` section have higher precedence than top-level options (because they are more specific).
#[test]
fn precedence() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "ruff.toml",
        r#"
[lint]
extend-select = ["B", "Q"]

[flake8-quotes]
inline-quotes = "double"

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(test.check_command()
        .arg("--config")
        .arg("ruff.toml")
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
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'flake8-quotes' -> 'lint.flake8-quotes'
    ");

    Ok(())
}

#[test]
fn exclude() -> Result<()> {
    let case = CliTest::new()?;

    case.write_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]
extend-exclude = ["out"]

[lint]
exclude = ["test.py", "generated.py"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    case.write_file(
        "main.py",
        r#"from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#,
    )?;

    // Excluded file but passed to the CLI directly, should be linted
    case.write_file(
        "test.py",
        r#"def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    case.write_file(
        "generated.py",
        r#"NUMBERS = [
     0,  1,  2,  3,  4,  5,  6,  7,  8,  9,
    10, 11, 12, 13, 14, 15, 16, 17, 18, 19
]
OTHER = "OTHER"
"#,
    )?;

    case.write_file("out/a.py", r#"a = "a""#)?;

    assert_cmd_snapshot!(
        case.check_command()
            .args(["--config", "ruff.toml"])
            // Explicitly pass test.py, should be linted regardless of it being excluded by lint.exclude
            .arg("test.py")
            // Lint all other files in the directory, should respect the `exclude` and `lint.exclude` options
            .arg("."), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:3:16: Q000 [*] Double quotes found but single quotes preferred
    main.py:4:12: Q000 [*] Double quotes found but single quotes preferred
    test.py:2:15: Q000 [*] Double quotes found but single quotes preferred
    Found 3 errors.
    [*] 3 fixable with the `--fix` option.

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
    ");

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/20035>
#[test]
fn deduplicate_directory_and_explicit_file() -> Result<()> {
    let case = CliTest::new()?;

    case.write_file(
        "ruff.toml",
        r#"
[lint]
exclude = ["main.py"]
"#,
    )?;

    case.write_file("main.py", "import os\n")?;

    assert_cmd_snapshot!(
        case.check_command()
            .args(["--config", "ruff.toml"])
            .arg(".")
            // Explicitly pass main.py, should be linted regardless of it being excluded by lint.exclude
            .arg("main.py"),
        @r"
        success: false
        exit_code: 1
        ----- stdout -----
        main.py:1:8: F401 [*] `os` imported but unused
        Found 1 error.
        [*] 1 fixable with the `--fix` option.

        ----- stderr -----
        "
    );

    Ok(())
}

#[test]
fn exclude_stdin() -> Result<()> {
    let case = CliTest::with_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]

[lint]
exclude = ["generated.py"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(
        case.check_command()
            .args(["--config", "ruff.toml"])
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

    Ok(())
}

#[test]
fn line_too_long_width_override() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "ruff.toml",
        r#"
line-length = 80
select = ["E501"]

[pycodestyle]
max-line-length = 100
"#,
    )?;

    assert_cmd_snapshot!(test.check_command()
        .arg("--config")
        .arg("ruff.toml")
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
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'select' -> 'lint.select'
      - 'pycodestyle' -> 'lint.pycodestyle'
    ");

    Ok(())
}

#[test]
fn per_file_ignores_stdin() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
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

    Ok(())
}

#[test]
fn extend_per_file_ignores_stdin() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
extend-select = ["B", "Q"]

[lint.flake8-quotes]
inline-quotes = "single"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
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

    Ok(())
}

/// Regression test for [#8858](https://github.com/astral-sh/ruff/issues/8858)
#[test]
fn parent_configuration_override() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["ALL"]
"#,
    )?;

    fixture.write_file(
        "subdirectory/ruff.toml",
        r#"
[lint]
ignore = ["D203", "D212"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .current_dir(fixture.root().join("subdirectory"))
        , @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: No Python files found under the given path(s)
    ");

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
        .args(["--config", "foo = bar", "."]), @r"
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
      |       ^^^
    string values must be quoted, expected literal string

    For more information, try '--help'.
    ");
}

#[test]
fn too_many_config_files() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file("ruff.toml", "")?;
    fixture.write_file("ruff2.toml", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("--config")
        .arg("ruff2.toml")
        .arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: You cannot specify more than one configuration file on the command line.

      tip: remove either `--config=ruff.toml` or `--config=ruff2.toml`.
           For more information, try `--help`.
    ");
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
extend = "ruff2.toml"
"#,
    )?;

    fixture.write_file(
        "ruff2.toml",
        r#"
extend = "ruff3.toml"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command(), @r"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to load extended configuration `[TMP]/ruff3.toml` (`[TMP]/ruff.toml` extends `[TMP]/ruff2.toml` extends `[TMP]/ruff3.toml`)
          Cause: Failed to read [TMP]/ruff3.toml
          Cause: No such file or directory (os error 2)
        ");

    Ok(())
}

#[test]
fn circular_extend() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
extend = "ruff2.toml"
"#,
    )?;
    fixture.write_file(
        "ruff2.toml",
        r#"
extend = "ruff3.toml"
"#,
    )?;
    fixture.write_file(
        "ruff3.toml",
        r#"
extend = "ruff.toml"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command(),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Circular configuration detected: `[TMP]/ruff.toml` extends `[TMP]/ruff2.toml` extends `[TMP]/ruff3.toml` extends `[TMP]/ruff.toml`
    ");

    Ok(())
}

#[test]
fn parse_error_extends() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
extend = "ruff2.toml"
"#,
    )?;
    fixture.write_file(
        "ruff2.toml",
        r#"
[lint]
select = [E501]
"#,
    )?;

    assert_cmd_snapshot!(
        fixture.check_command(),
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
      |           ^^^^
    string values must be quoted, expected literal string
    ");

    Ok(())
}

#[test]
fn config_file_and_isolated() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file("ruff.toml", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("--isolated")
        .arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: The argument `--config=ruff.toml` cannot be used with `--isolated`

      tip: You cannot specify a configuration file and also specify `--isolated`,
           as `--isolated` causes ruff to ignore all configuration files.
           For more information, try `--help`.
    ");
    Ok(())
}

#[test]
fn config_override_via_cli() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
line-length = 100

[lint]
select = ["I"]

[lint.isort]
combine-as-imports = true
        "#,
    )?;
    let test_code = r#"
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
    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .args(["--config", "line-length=90"])
        .args(["--config", "lint.extend-select=['E501', 'F841']"])
        .args(["--config", "lint.isort.combine-as-imports = false"])
        .arg("-")
        .pass_stdin(test_code), @r"
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
    unexpected key or value, expected newline, `#`

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
    unexpected key or value, expected newline, `#`

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
    - `lint.typing-extensions`
    - `lint.future-annotations`

    For more information, try '--help'.
    ");
}

#[test]
fn config_doubly_overridden_via_cli() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
line-length = 100

[lint]
select=["E501"]
"#,
    )?;
    let test_code = "x = 'longer_than_90_charactersssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss'";
    assert_cmd_snapshot!(fixture
        .check_command()
        // The --line-length flag takes priority over both the config file
        // and the `--config="line-length=110"` flag,
        // despite them both being specified after this flag on the command line:
        .args(["--line-length", "90"])
        .arg("--config")
        .arg("ruff.toml")
        .args(["--config", "line-length=110"])
        .arg("-")
        .pass_stdin(test_code), @r"
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
    let fixture = CliTest::with_file("ruff.toml", "lint.select = ['N801']")?;
    let test_code = "class violates_n801: pass";
    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .args(["--config", "lint.per-file-ignores = {'generated.py' = ['N801']}"])
        .args(["--stdin-filename", "generated.py"])
        .arg("-")
        .pass_stdin(test_code), @r"
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
include = ["*.ipy"]
"#,
    )?;

    fixture.write_file(
        "main.ipy",
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

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
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

    Ok(())
}

#[test]
fn warn_invalid_noqa_with_no_diagnostics() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
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
            )
    );
}

#[test]
fn file_noqa_external() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint]
external = ["AAA"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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

    Ok(())
}

#[test]
fn required_version_exact_mismatch() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
required-version = "0.1.0"
"#,
    )?;

    insta::with_settings!({
        filters => vec![(version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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

    let fixture = CliTest::with_file(
        "ruff.toml",
        &format!(
            r#"
required-version = "{version}"
"#
        ),
    )?;

    insta::with_settings!({
        filters => vec![(version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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

    let fixture = CliTest::with_file(
        "ruff.toml",
        &format!(
            r#"
required-version = ">{version}"
"#
        ),
    )?;

    insta::with_settings!({
        filters => vec![(version, "[VERSION]")]
    }, {
    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
required-version = ">=0.1.0"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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

    Ok(())
}

/// Expand environment variables in `--config` paths provided via the CLI.
#[test]
fn config_expand() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint]
select = ["F"]
ignore = ["F841"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("${NAME}.toml")
        .env("NAME", "ruff")
        .arg("-")
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint.per-file-ignores]
"!selected.py" = ["RUF"]
"#,
    )?;
    fixture.write_file("selected.py", "")?;
    fixture.write_file("ignored.py", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("--select")
        .arg("RUF901")
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint.per-file-ignores]
"!src/**.py" = ["RUF"]
"#,
    )?;
    fixture.write_file("src/selected.py", "")?;
    fixture.write_file("ignored.py", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("--select")
        .arg("RUF901")
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----
    src/selected.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}

/// patterns are additive, can't use negative patterns to "un-ignore"
#[test]
fn negated_per_file_ignores_overlap() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint.per-file-ignores]
"*.py" = ["RUF"]
"!foo.py" = ["RUF"]
"#,
    )?;
    fixture.write_file("foo.py", "")?;
    fixture.write_file("bar.py", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("--select")
        .arg("RUF901")
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
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint]
select = ["F"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
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

    Ok(())
}

#[test]
fn add_noqa() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["RUF015"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
        .arg("noqa.py")
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

    let test_code =
        fs::read_to_string(fixture.root().join("noqa.py")).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def first_square():
        return [x * x for x in range(20)][0]  # noqa: RUF015
    ");

    Ok(())
}

#[test]
fn add_noqa_multiple_codes() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["ANN001", "ANN201", "ARG001", "D103"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
        r#"
def unused(x):
    pass
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
        .arg("noqa.py")
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

    let test_code =
        fs::read_to_string(fixture.root().join("noqa.py")).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def unused(x):  # noqa: ANN001, ANN201, D103
        pass
    ");

    Ok(())
}

#[test]
fn add_noqa_multiline_diagnostic() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["I"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
        r#"
import z
import c
import a
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
        .arg("noqa.py")
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

    let test_code =
        fs::read_to_string(fixture.root().join("noqa.py")).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    import z  # noqa: I001
    import c
    import a
    ");

    Ok(())
}

#[test]
fn add_noqa_existing_noqa() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["ANN001", "ANN201", "ARG001", "D103"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
        r#"
def unused(x):  # noqa: ANN001, ARG001, D103
    pass
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
        .arg("noqa.py")
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

    let test_code =
        fs::read_to_string(fixture.root().join("noqa.py")).expect("should read test file");

    insta::assert_snapshot!(test_code, @r"
    def unused(x):  # noqa: ANN001, ANN201, ARG001, D103
        pass
    ");

    Ok(())
}

#[test]
fn add_noqa_multiline_comment() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["UP031"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
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

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--config", "ruff.toml"])
        .arg("noqa.py")
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

    let test_code =
        fs::read_to_string(fixture.root().join("noqa.py")).expect("should read test file");

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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
exclude = ["excluded.py"]
select = ["RUF015"]
"#,
    )?;

    fixture.write_file(
        "noqa.py",
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    fixture.write_file(
        "excluded.py",
        r#"
def first_square():
    return [x * x for x in range(20)][0]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--add-noqa"]), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 1 noqa directive.
    ");

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/2253>
#[test]
fn add_noqa_parent() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "noqa.py",
        r#"
from foo import (  # noqa: F401
		bar
)
		"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--add-noqa")
        .arg("--select=F401")
        .arg("noqa.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn add_noqa_with_reason() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "test.py",
        r#"import os

def foo():
    x = 1
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--add-noqa=TODO: fix")
        .arg("--select=F401,F841")
        .arg("test.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added 2 noqa directives.
    ");

    let content = fs::read_to_string(fixture.root().join("test.py"))?;
    insta::assert_snapshot!(content, @r"
import os  # noqa: F401 TODO: fix

def foo():
    x = 1  # noqa: F841 TODO: fix
");

    Ok(())
}

#[test]
fn add_noqa_with_newline_in_reason() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file("test.py", "import os\n")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--add-noqa=line1\nline2")
        .arg("--select=F401")
        .arg("test.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: --add-noqa <reason> cannot contain newline characters
    "###);

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python() -> Result<()> {
    let fixture = CliTest::with_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("pyproject.toml")
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

    let fixture2 = CliTest::with_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.8"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    assert_cmd_snapshot!(fixture2
        .check_command()
        .arg("--config")
        .arg("pyproject.toml")
        .args(["--stdin-filename", "test.py"])
        .arg("-")
        .pass_stdin(r#"from typing import List; foo: List[int]"#), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_patch() -> Result<()> {
    let fixture = CliTest::with_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11.4"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("pyproject.toml")
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

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_equals() -> Result<()> {
    let fixture = CliTest::with_file(
        "pyproject.toml",
        r#"[project]
requires-python = "== 3.11"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("pyproject.toml")
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

    Ok(())
}

/// Infer `3.11` from `requires-python` in `pyproject.toml`.
#[test]
fn requires_python_equals_patch() -> Result<()> {
    let fixture = CliTest::with_file(
        "pyproject.toml",
        r#"[project]
requires-python = "== 3.11.4"

[tool.ruff.lint]
select = ["UP006"]
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("pyproject.toml")
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

    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .args(["--select", "UP007"])
            .arg("test.py")
            .arg("-")
    );

    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool_preview_enabled() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--preview")
            .arg("--show-settings")
            .args(["--select", "UP007"])
            .arg("test.py")
            .arg("-")
    );

    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool_target_version_override() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .args(["--select", "UP007"])
            .args(["--target-version", "py310"])
            .arg("test.py")
            .arg("-")
    );

    Ok(())
}

/// ```
/// tmp
/// ├── pyproject.toml #<--- no `[tool.ruff]`
/// └── test.py
/// ```
#[test]
fn requires_python_no_tool_with_check() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"from typing import Union;foo: Union[int, str] = 1"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .args(["--select","UP007"])
        .arg(".")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:1:31: UP007 [*] Use `X | Y` for type annotations
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"[lint]
select = ["UP007"]
"#,
    )?;

    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("test.py")
            .arg("--show-settings"),
    );

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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"[lint]
select = ["UP007"]
"#,
    )?;

    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("test.py")
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:2:31: UP007 [*] Use `X | Y` for type annotations
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "foo/pyproject.toml",
        r#"[project]
"#,
    )?;

    fixture.write_file(
        "foo/test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .args(["--select", "UP007"])
            .arg("foo/test.py"),
    );

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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "foo/pyproject.toml",
        r#"
[tool.ruff]
target-version = "py310"
"#,
    )?;

    fixture.write_file(
        "foo/test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .args(["--select", "UP007"])
            .arg("foo/test.py"),
    );
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
[lint]
select = ["UP007"]
"#,
    )?;

    fixture.write_file(
        "foo/pyproject.toml",
        r#"[project]
requires-python = ">= 3.11"
"#,
    )?;

    fixture.write_file(
        "foo/test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .arg("foo/test.py"),
    );

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .arg("test.py")
            .current_dir(fixture.root().join("foo")),
    );
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
    let fixture = CliTest::new()?;
    fixture.write_file(
        "ruff.toml",
        r#"
extend = "./shared/base_config.toml"
[lint]
select = ["UP007"]
"#,
    )?;

    fixture.write_file(
        "pyproject.toml",
        r#"[project]
requires-python = ">= 3.10"
"#,
    )?;

    fixture.write_file(
        "shared/base_config.toml",
        r#"
target-version = "py311"
"#,
    )?;

    fixture.write_file(
        "test.py",
        r#"
from typing import Union;foo: Union[int, str] = 1
"#,
    )?;

    assert_cmd_snapshot!(
        fixture
            .check_command()
            .arg("--show-settings")
            .arg("test.py"),
    );
    Ok(())
}

#[test]
fn checks_notebooks_in_stable() -> anyhow::Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file(
        "main.ipynb",
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

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--select")
        .arg("F401")
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
    let fixture = CliTest::new()?;

    fixture.write_file("foo/__init__.py", "")?;
    fixture.write_file("foo/bar/baz/__init__.py", "")?;
    fixture.write_file("foo/bar/baz/bop.py", "")?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--select")
        .arg("INP")
        , @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--select")
        .arg("INP")
        .arg("--preview")
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----
    foo/bar/baz/__init__.py:1:1: INP001 File `foo/bar/baz/__init__.py` declares a package, but is nested under an implicit namespace package. Add an `__init__.py` to `foo/bar`.
    Found 1 error.

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_alias_name() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint.flake8-import-conventions.aliases]
"module.name" = "invalid.alias"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("-")
        .pass_stdin("")
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
    "#);
    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_extend_alias_name() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint.flake8-import-conventions.extend-aliases]
"module.name" = "__debug__"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("-")
        .pass_stdin("")
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
    "#);
    Ok(())
}

#[test]
fn flake8_import_convention_invalid_aliases_config_module_name() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint.flake8-import-conventions.aliases]
"module..invalid" = "alias"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("-")
        .pass_stdin("")
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
    "#);
    Ok(())
}

#[test]
fn flake8_import_convention_nfkc_normalization() -> Result<()> {
    let fixture = CliTest::with_file(
        "ruff.toml",
        r#"
[lint.flake8-import-conventions.aliases]
"test.module" = "_﹏𝘥𝘦𝘣𝘶𝘨﹏﹏"
"#,
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--config")
        .arg("ruff.toml")
        .arg("-")
        .pass_stdin("")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: Invalid alias for module 'test.module': alias normalizes to '__debug__', which is not allowed.
    "###);
    Ok(())
}

#[test]
fn flake8_import_convention_unused_aliased_import() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(r#"lint.isort.required-imports = ["import pandas"]"#)
            .args(["--select", "I002,ICN001,F401"])
            .args(["--stdin-filename", "test.py"])
            .arg("--unsafe-fixes")
            .arg("--fix")
            .arg("-")
            .pass_stdin("1")
    );
}

#[test]
fn flake8_import_convention_unused_aliased_import_no_conflict() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .arg("--config")
            .arg(r#"lint.isort.required-imports = ["import pandas as pd"]"#)
            .args(["--select", "I002,ICN001,F401"])
            .args(["--stdin-filename", "test.py"])
            .arg("--unsafe-fixes")
            .arg("--fix")
            .arg("-")
            .pass_stdin("1")
    );
}

// https://github.com/astral-sh/ruff/issues/19842
#[test]
fn pyupgrade_up026_respects_isort_required_import_fix() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .arg("--isolated")
            .arg("check")
            .arg("-")
            .args(["--select", "I002,UP026"])
            .arg("--config")
            .arg(r#"lint.isort.required-imports=["import mock"]"#)
            .arg("--fix")
            .arg("--no-cache")
            .pass_stdin("1\n"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    import mock
    1

    ----- stderr -----
    Found 1 error (1 fixed, 0 remaining).
    "
    );
}

// https://github.com/astral-sh/ruff/issues/19842
#[test]
fn pyupgrade_up026_respects_isort_required_import_from_fix() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .arg("--isolated")
            .arg("check")
            .arg("-")
            .args(["--select", "I002,UP026"])
            .arg("--config")
            .arg(r#"lint.isort.required-imports = ["from mock import mock"]"#)
            .arg("--fix")
            .arg("--no-cache")
            .pass_stdin("from mock import mock\n"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    from mock import mock

    ----- stderr -----
    All checks passed!
    "
    );
}

// See: https://github.com/astral-sh/ruff/issues/16177
#[test]
fn flake8_pyi_redundant_none_literal() {
    let snippet = r#"
from typing import Literal

# For each of these expressions, Ruff provides a fix for one of the `Literal[None]` elements
# but not both, as if both were autofixed it would result in `None | None`,
# which leads to a `TypeError` at runtime.
a: Literal[None,] | Literal[None,]
b: Literal[None] | Literal[None]
c: Literal[None] | Literal[None,]
d: Literal[None,] | Literal[None]
"#;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--select", "PYI061"])
        .args(["--stdin-filename", "test.py"])
        .arg("--preview")
        .arg("--diff")
        .arg("-")
        .pass_stdin(snippet), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    --- test.py
    +++ test.py
    @@ -4,7 +4,7 @@
     # For each of these expressions, Ruff provides a fix for one of the `Literal[None]` elements
     # but not both, as if both were autofixed it would result in `None | None`,
     # which leads to a `TypeError` at runtime.
    -a: Literal[None,] | Literal[None,]
    -b: Literal[None] | Literal[None]
    -c: Literal[None] | Literal[None,]
    -d: Literal[None,] | Literal[None]
    +a: None | Literal[None,]
    +b: None | Literal[None]
    +c: None | Literal[None,]
    +d: None | Literal[None]


    ----- stderr -----
    Would fix 4 errors.
    ");
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
fn create_a005_module_structure(fixture: &CliTest) -> Result<()> {
    // Create module structure
    fixture.write_file("abc/__init__.py", "")?;
    fixture.write_file("collections/__init__.py", "")?;
    fixture.write_file("collections/abc/__init__.py", "")?;
    fixture.write_file("collections/foobar/__init__.py", "")?;
    fixture.write_file("foobar/__init__.py", "")?;
    fixture.write_file("foobar/abc/__init__.py", "")?;
    fixture.write_file("foobar/collections/__init__.py", "")?;
    fixture.write_file("foobar/collections/abc/__init__.py", "")?;
    fixture.write_file("urlparse/__init__.py", "")?;
    // also create a ruff.toml to mark the project root
    fixture.write_file("ruff.toml", "")?;

    Ok(())
}

/// Test A005 with `strict-checking = true`
#[test]
fn a005_module_shadowing_strict() -> Result<()> {
    let fixture = CliTest::new()?;
    create_a005_module_structure(&fixture)?;

    assert_cmd_snapshot!(fixture.check_command()
        .arg("--config")
        .arg(r#"lint.flake8-builtins.strict-checking = true"#)
        .args(["--select", "A005"]),
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

    Ok(())
}

/// Test A005 with `strict-checking = false`
#[test]
fn a005_module_shadowing_non_strict() -> Result<()> {
    let fixture = CliTest::new()?;
    create_a005_module_structure(&fixture)?;

    assert_cmd_snapshot!(fixture.check_command()
        .arg("--config")
        .arg(r#"lint.flake8-builtins.strict-checking = false"#)
        .args(["--select", "A005"]),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
    collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
    Found 2 errors.

    ----- stderr -----
    ");

    Ok(())
}

/// Test A005 with `strict-checking` unset
///
/// This should match the non-strict version directly above
/// Test A005 with `strict-checking` default (should be `false`)
#[test]
fn a005_module_shadowing_strict_default() -> Result<()> {
    let fixture = CliTest::new()?;
    create_a005_module_structure(&fixture)?;

    assert_cmd_snapshot!(fixture.check_command()
        .args(["--select", "A005"]),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    abc/__init__.py:1:1: A005 Module `abc` shadows a Python standard-library module
    collections/__init__.py:1:1: A005 Module `collections` shadows a Python standard-library module
    Found 2 errors.

    ----- stderr -----
    ");

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
    test.py:1:2: invalid-syntax: Cannot use named assignment expression (`:=`) on Python 3.7 (syntax was added in Python 3.8)
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
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:2:1: invalid-syntax: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)
    Found 1 error.

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
    test.py:2:1: invalid-syntax: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)
    Found 1 error.

    ----- stderr -----
    "
    );
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/16417>
#[test]
fn cache_syntax_errors() -> Result<()> {
    let fixture = CliTest::with_file("main.py", "match 2:\n    case 1: ...")?;

    let mut cmd = fixture.command();
    // inline STDIN_BASE_OPTIONS to remove --no-cache
    cmd.args(["check", "--output-format", "concise"])
        .arg("--target-version=py39")
        .arg("--preview")
        .arg("--quiet"); // suppress `debug build without --no-cache` warnings

    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:1: invalid-syntax: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)

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
    main.py:1:1: invalid-syntax: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)

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
    let fixture = CliTest::new()?;

    fixture.write_file(
        "{{cookiecutter.repo_name}}/pyproject.toml",
        r#"tool.ruff.lint.per-file-ignores = { "tests/*" = ["F811"] }"#,
    )?;

    // F811 example from the docs to ensure the glob still works
    fixture.write_file(
        "{{cookiecutter.repo_name}}/tests/maintest.py",
        "import foo\nimport bar\nimport foo",
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--select=F811"), @r"
		success: true
		exit_code: 0
		----- stdout -----
		All checks passed!

		----- stderr -----
		");

    // after removing the config file with the ignore, F811 applies, so the glob worked above
    fs::remove_file(
        fixture
            .root()
            .join("{{cookiecutter.repo_name}}/pyproject.toml"),
    )?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .arg("--select=F811"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    {{cookiecutter.repo_name}}/tests/maintest.py:3:8: F811 [*] Redefinition of unused `foo` from line 1: `foo` redefined here
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ");

    Ok(())
}

/// Like the test above but exercises the non-absolute path case in `PerFile::new`
#[test]
fn cookiecutter_globbing_no_project_root() -> Result<()> {
    let fixture = CliTest::new()?;

    // Create the nested directory structure
    fs::create_dir(fixture.root().join("{{cookiecutter.repo_name}}"))?;

    assert_cmd_snapshot!(fixture
        .check_command()
        .current_dir(fixture.root().join("{{cookiecutter.repo_name}}"))
        .args(["--extend-per-file-ignores", "generated.py:Q"]), @r"
	success: true
	exit_code: 0
	----- stdout -----
	All checks passed!

	----- stderr -----
	warning: No Python files found under the given path(s)
	");

    Ok(())
}

/// Test that semantic syntax errors (1) are emitted, (2) are not cached, (3) don't affect the
/// reporting of normal diagnostics, and (4) are not suppressed by `select = []` (or otherwise
/// disabling all AST-based rules).
#[test]
fn semantic_syntax_errors() -> Result<()> {
    let fixture = CliTest::with_file("main.py", "[(x := 1) for x in foo]")?;
    let contents = "[(x := 1) for x in foo]";

    let mut cmd = fixture.command();
    // inline STDIN_BASE_OPTIONS to remove --no-cache
    cmd.args(["check", "--output-format", "concise"])
        .arg("--preview")
        .arg("--quiet"); // suppress `debug build without --no-cache` warnings

    assert_cmd_snapshot!(
        cmd,
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:3: invalid-syntax: assignment expression cannot rebind comprehension variable
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
    main.py:1:3: invalid-syntax: assignment expression cannot rebind comprehension variable
    main.py:1:20: F821 Undefined name `foo`

    ----- stderr -----
    "
    );

    // ensure semantic errors are caught even without AST-based rules selected
    assert_cmd_snapshot!(
        fixture.check_command()
            .args(["--config", "lint.select = []"])
            .arg("--preview")
            .arg("-")
            .pass_stdin(contents),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:3: invalid-syntax: assignment expression cannot rebind comprehension variable
    Found 1 error.

    ----- stderr -----
    "
    );

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/ruff/issues/17821>.
///
/// `lint.typing-extensions = false` with Python 3.9 should disable the PYI019 lint because it would
/// try to import `Self` from `typing_extensions`
#[test]
fn combine_typing_extensions_config() {
    let contents = "
from typing import TypeVar
T = TypeVar('T')
class Foo:
    def f(self: T) -> T: ...
";
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .args(["--config", "lint.typing-extensions = false"])
            .arg("--select=PYI019")
            .arg("--target-version=py39")
            .arg("-")
            .pass_stdin(contents),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );
}

#[test_case::test_case("concise")]
#[test_case::test_case("full")]
#[test_case::test_case("json")]
#[test_case::test_case("json-lines")]
#[test_case::test_case("junit")]
#[test_case::test_case("grouped")]
#[test_case::test_case("github")]
#[test_case::test_case("gitlab")]
#[test_case::test_case("pylint")]
#[test_case::test_case("rdjson")]
#[test_case::test_case("azure")]
#[test_case::test_case("sarif")]
fn output_format(output_format: &str) -> Result<()> {
    const CONTENT: &str = "\
import os  # F401
x = y      # F821
match 42:  # invalid-syntax
    case _: ...
";

    let fixture = CliTest::with_settings(|_project_dir, mut settings| {
        // JSON double escapes backslashes
        settings.add_filter(r#""[^"]+\\?/?input.py"#, r#""[TMP]/input.py"#);

        settings
    })?;

    fixture.write_file("input.py", CONTENT)?;

    let snapshot = format!("output_format_{output_format}");

    assert_cmd_snapshot!(
        snapshot,
        fixture.command().args([
            "check",
            "--no-cache",
            "--output-format",
            output_format,
            "--select",
            "F401,F821",
            "--target-version",
            "py39",
            "input.py",
        ])
    );

    Ok(())
}

#[test_case::test_case("concise"; "concise_show_fixes")]
#[test_case::test_case("full"; "full_show_fixes")]
#[test_case::test_case("grouped"; "grouped_show_fixes")]
fn output_format_show_fixes(output_format: &str) -> Result<()> {
    let fixture = CliTest::with_file("input.py", "import os  # F401")?;
    let snapshot = format!("output_format_show_fixes_{output_format}");

    assert_cmd_snapshot!(
        snapshot,
        fixture.command().args([
            "check",
            "--no-cache",
            "--output-format",
            output_format,
            "--select",
            "F401",
            "--fix",
            "--show-fixes",
            "input.py",
        ])
    );

    Ok(())
}

#[test]
fn up045_nested_optional_flatten_all() {
    let contents = "\
from typing import Optional
nested_optional: Optional[Optional[Optional[str]]] = None
";

    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(STDIN_BASE_OPTIONS)
            .args(["--select", "UP045", "--diff", "--target-version", "py312"])
            .arg("-")
            .pass_stdin(contents),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    @@ -1,2 +1,2 @@
     from typing import Optional
    -nested_optional: Optional[Optional[Optional[str]]] = None
    +nested_optional: str | None = None


    ----- stderr -----
    Would fix 1 error.
    ",
    );
}

#[test]
fn show_fixes_in_full_output_with_preview_enabled() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME))
            .args(["check", "--no-cache", "--output-format", "full"])
            .args(["--select", "F401"])
            .arg("--preview")
            .arg("-")
            .pass_stdin("import math"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    F401 [*] `math` imported but unused
     --> -:1:8
      |
    1 | import math
      |        ^^^^
      |
    help: Remove unused import: `math`
      - import math

    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    ",
    );
}

#[test]
fn rule_panic_mixed_results_concise() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file("normal.py", "import os")?;
    fixture.write_file("panic.py", "print('hello, world!')")?;

    assert_cmd_snapshot!(
        fixture.check_command()
            .args(["--select", "RUF9", "--preview"])
            .args(["normal.py", "panic.py"]),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----
    normal.py:1:1: RUF900 Hey this is a stable test rule.
    normal.py:1:1: RUF901 [*] Hey this is a stable test rule with a safe fix.
    normal.py:1:1: RUF902 Hey this is a stable test rule with an unsafe fix.
    normal.py:1:1: RUF903 Hey this is a stable test rule with a display only fix.
    normal.py:1:1: RUF911 Hey this is a preview test rule.
    normal.py:1:1: RUF950 Hey this is a test rule that was redirected from another.
    panic.py: panic: Panicked at <location> when checking `[TMP]/panic.py`: `This is a fake panic for testing.`
    Found 7 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    error: Panic during linting indicates a bug in Ruff. If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BLinter%20panic%5D

    ...with the relevant file contents, the `pyproject.toml` settings, and the stack trace above, we'd be very appreciative!
    ");

    Ok(())
}

#[test]
fn rule_panic_mixed_results_full() -> Result<()> {
    let fixture = CliTest::new()?;
    fixture.write_file("normal.py", "import os")?;
    fixture.write_file("panic.py", "print('hello, world!')")?;

    assert_cmd_snapshot!(
        fixture.command()
            .args(["check", "--select", "RUF9", "--preview", "--output-format=full", "--no-cache"])
            .args(["normal.py", "panic.py"]),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----
    RUF900 Hey this is a stable test rule.
    --> normal.py:1:1

    RUF901 [*] Hey this is a stable test rule with a safe fix.
    --> normal.py:1:1
    1 + # fix from stable-test-rule-safe-fix
    2 | import os

    RUF902 Hey this is a stable test rule with an unsafe fix.
    --> normal.py:1:1

    RUF903 Hey this is a stable test rule with a display only fix.
    --> normal.py:1:1

    RUF911 Hey this is a preview test rule.
    --> normal.py:1:1

    RUF950 Hey this is a test rule that was redirected from another.
    --> normal.py:1:1

    panic: Panicked at <location> when checking `[TMP]/panic.py`: `This is a fake panic for testing.`
    --> panic.py:1:1
    info: This indicates a bug in Ruff.
    info: If you could open an issue at https://github.com/astral-sh/ruff/issues/new?title=%5Bpanic%5D, we'd be very appreciative!
    info: run with `RUST_BACKTRACE=1` environment variable to show the full backtrace information

    Found 7 errors.
    [*] 1 fixable with the `--fix` option (1 hidden fix can be enabled with the `--unsafe-fixes` option).

    ----- stderr -----
    error: Panic during linting indicates a bug in Ruff. If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BLinter%20panic%5D

    ...with the relevant file contents, the `pyproject.toml` settings, and the stack trace above, we'd be very appreciative!
    ");

    Ok(())
}

/// Test that the same rule fires across all supported extensions, but not on unsupported files
#[test]
fn supported_file_extensions() -> Result<()> {
    let fixture = CliTest::new()?;

    // Create files of various types
    // text file
    fixture.write_file("src/thing.txt", "hello world\n")?;
    // regular python
    fixture.write_file("src/thing.py", "import os\nprint('hello world')\n")?;
    // python typestub
    fixture.write_file("src/thing.pyi", "import os\nclass foo:\n  ...\n")?;
    // windows gui
    fixture.write_file("src/thing.pyw", "import os\nprint('hello world')\n")?;
    // cython
    fixture.write_file(
        "src/thing.pyx",
        "import os\ncdef int add(int a, int b):\n  return a + b\n",
    )?;
    // notebook
    fixture.write_file(
        "src/thing.ipynb",
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

    assert_cmd_snapshot!(
        fixture.check_command()
            .args(["--select", "F401"])
            .arg("src"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    src/thing.ipynb:cell 1:1:8: F401 [*] `os` imported but unused
    src/thing.py:1:8: F401 [*] `os` imported but unused
    src/thing.pyi:1:8: F401 [*] `os` imported but unused
    Found 3 errors.
    [*] 3 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}

/// Test that the same rule fires across all supported extensions, but not on unsupported files
#[test]
fn supported_file_extensions_preview_enabled() -> Result<()> {
    let fixture = CliTest::new()?;

    // Create files of various types
    // text file
    fixture.write_file("src/thing.txt", "hello world\n")?;
    // regular python
    fixture.write_file("src/thing.py", "import os\nprint('hello world')\n")?;
    // python typestub
    fixture.write_file("src/thing.pyi", "import os\nclass foo:\n  ...\n")?;
    // windows gui
    fixture.write_file("src/thing.pyw", "import os\nprint('hello world')\n")?;
    // cython
    fixture.write_file(
        "src/thing.pyx",
        "import os\ncdef int add(int a, int b):\n  return a + b\n",
    )?;
    // notebook
    fixture.write_file(
        "src/thing.ipynb",
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

    assert_cmd_snapshot!(
        fixture.check_command()
            .args(["--select", "F401", "--preview"])
            .arg("src"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    src/thing.ipynb:cell 1:1:8: F401 [*] `os` imported but unused
    src/thing.py:1:8: F401 [*] `os` imported but unused
    src/thing.pyi:1:8: F401 [*] `os` imported but unused
    src/thing.pyw:1:8: F401 [*] `os` imported but unused
    Found 4 errors.
    [*] 4 fixable with the `--fix` option.

    ----- stderr -----
    ");
    Ok(())
}
