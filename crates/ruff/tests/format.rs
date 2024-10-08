#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use regex::escape;
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";

fn tempdir_filter(tempdir: &TempDir) -> String {
    format!(r"{}\\?/?", escape(tempdir.path().to_str().unwrap()))
}

#[test]
fn default_options() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print('Shouldn\'t change quotes')


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
        print("Shouldn't change quotes")


    if condition:
        print('Hy "Micha"')  # Should not change quotes

    ----- stderr -----
    "###);
}

#[test]
fn default_files() -> Result<()> {
    let tempdir = TempDir::new()?;
    fs::write(
        tempdir.path().join("foo.py"),
        r#"
foo =     "needs formatting"
"#,
    )?;
    fs::write(
        tempdir.path().join("bar.py"),
        r#"
bar =     "needs formatting"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--no-cache", "--check"]).current_dir(tempdir.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: bar.py
    Would reformat: foo.py
    2 files would be reformatted

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn format_warn_stdin_filename_with_files() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "foo.py"])
        .arg("foo.py")
        .pass_stdin("foo =     1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    foo = 1

    ----- stderr -----
    warning: Ignoring file foo.py in favor of standard input.
    "###);
}

#[test]
fn nonexistent_config_file() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--config", "foo.toml", "."]), @r###"
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
        .args(["format", "--config", "foo = bar", "."]), @r###"
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
        .arg("format")
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
fn config_file_and_isolated() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_dot_toml = tempdir.path().join("ruff.toml");
    fs::File::create(&ruff_dot_toml)?;
    insta::with_settings!({
        filters => vec![(tempdir_filter(&tempdir).as_str(), "[TMP]/")]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("format")
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
    fs::write(&ruff_toml, "line-length = 100")?;
    let fixture = r#"
def foo():
    print("looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong string")

    "#;
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("format")
        .arg("--config")
        .arg(&ruff_toml)
        // This overrides the long line length set in the config file
        .args(["--config", "line-length=80"])
        .arg("-")
        .pass_stdin(fixture), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo():
        print(
            "looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong string"
        )

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn config_doubly_overridden_via_cli() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(&ruff_toml, "line-length = 70")?;
    let fixture = r#"
def foo():
    print("looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong string")

    "#;
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("format")
        .arg("--config")
        .arg(&ruff_toml)
        // This overrides the long line length set in the config file...
        .args(["--config", "line-length=80"])
        // ...but this overrides them both:
        .args(["--line-length", "100"])
        .arg("-")
        .pass_stdin(fixture), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo():
        print("looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong string")

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn format_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
indent-width = 8
line-length = 84

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
    "###);
    Ok(())
}

#[test]
fn docstring_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r"
[format]
docstring-code-format = true
docstring-code-line-length = 20
",
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r"
def f(x):
    '''
    Something about `f`. And an example:

    .. code-block:: python

        foo, bar, quux = this_is_a_long_line(lion, hippo, lemur, bear)

    Another example:

    ```py
    foo, bar, quux = this_is_a_long_line(lion, hippo, lemur, bear)
    ```

    And another:

    >>> foo, bar, quux = this_is_a_long_line(lion, hippo, lemur, bear)
    '''
    pass
"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def f(x):
        """
        Something about `f`. And an example:

        .. code-block:: python

            foo, bar, quux = (
                this_is_a_long_line(
                    lion,
                    hippo,
                    lemur,
                    bear,
                )
            )

        Another example:

        ```py
        foo, bar, quux = (
            this_is_a_long_line(
                lion,
                hippo,
                lemur,
                bear,
            )
        )
        ```

        And another:

        >>> foo, bar, quux = (
        ...     this_is_a_long_line(
        ...         lion,
        ...         hippo,
        ...         lemur,
        ...         bear,
        ...     )
        ... )
        """
        pass

    ----- stderr -----
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
        .args(["format", "--no-cache", "--diff", "--isolated"])
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    2 files already formatted
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
        .args(["format", "--no-cache", "--check", "--config"])
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
    "###);
    Ok(())
}

#[test]
fn syntax_error() -> Result<()> {
    let tempdir = TempDir::new()?;

    fs::write(
        tempdir.path().join("main.py"),
        r"
from module import =
",
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--no-cache", "--isolated", "--check"])
        .arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse main.py:2:20: Expected an import name
    "###);

    Ok(())
}

#[test]
fn messages() -> Result<()> {
    let tempdir = TempDir::new()?;

    fs::write(
        tempdir.path().join("main.py"),
        r#"
from test import say_hy

if __name__ == "__main__":
    say_hy("dear Ruff contributor")
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--no-cache", "--isolated", "--check"])
        .arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    1 file would be reformatted

    ----- stderr -----
    "###);

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--no-cache", "--isolated"])
        .arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    "###);

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--no-cache", "--isolated"])
        .arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file left unchanged

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn force_exclude() -> Result<()> {
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
        .args(["format", "--no-cache", "--force-exclude", "--check", "--config"])
        .arg(ruff_toml.file_name().unwrap())
        // Explicitly pass test.py, should be respect the `format.exclude` when `--force-exclude` is present
        .arg(test_path.file_name().unwrap())
        // Format all other files in the directory, should respect the `exclude` and `format.exclude` options
        .arg("."), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    1 file would be reformatted

    ----- stderr -----
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
ignore = ["Q000", "Q001", "Q002", "Q003"]

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
    from test import say_hy

    if __name__ == "__main__":
        say_hy("dear Ruff contributor")

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
      - 'ignore' -> 'lint.ignore'
    "###);
    Ok(())
}

#[test]
fn force_exclude_stdin() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
extend-select = ["B", "Q"]
ignore = ["Q000", "Q001", "Q002", "Q003"]

[format]
exclude = ["generated.py"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .current_dir(tempdir.path())
        .args(["format", "--config", &ruff_toml.file_name().unwrap().to_string_lossy(), "--stdin-filename", "generated.py", "--force-exclude", "-"])
        .pass_stdin(r#"
from test import say_hy

if __name__ == '__main__':
    say_hy("dear Ruff contributor")
"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    from test import say_hy

    if __name__ == '__main__':
        say_hy("dear Ruff contributor")

    ----- stderr -----
    warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `ruff.toml`:
      - 'extend-select' -> 'lint.extend-select'
      - 'ignore' -> 'lint.ignore'
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

[lint]
extend-select = ["COM812"]

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
    warning: The following rule may cause conflicts when used with the formatter: `COM812`. To avoid unexpected behavior, we recommend disabling this rule, either by removing it from the `select` or `extend-select` configuration, or adding it to the `ignore` configuration.
    "###);
    Ok(())
}

#[test]
fn deprecated_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r"
tab-size = 2
",
    )?;

    insta::with_settings!({filters => vec![
        (&*regex::escape(ruff_toml.to_str().unwrap()), "[RUFF-TOML-PATH]"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(["format", "--config"])
            .arg(&ruff_toml)
            .arg("-")
            .pass_stdin(r"
if True:
    pass
    "), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: The `tab-size` option has been renamed to `indent-width` to emphasize that it configures the indentation used by the formatter as well as the tab width. Please update `[RUFF-TOML-PATH]` to use `indent-width = <value>` instead.
        "###);
    });
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
            .pass_stdin(r"
    import os
    "), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Failed to parse [RUFF-TOML-PATH]
          Cause: TOML parse error at line 2, column 10
          |
        2 | format = "json"
          |          ^^^^^^
        invalid type: string "json", expected struct FormatOptions

        "###);
    });
    Ok(())
}

#[test]
fn conflicting_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
indent-width = 2

[lint]
select = ["ALL"]
ignore = ["D203", "D212"]

[lint.isort]
lines-after-imports = 3
lines-between-types = 2
force-wrap-aliases = true
combine-as-imports = true
split-on-trailing-comma = true

[lint.flake8-quotes]
inline-quotes = "single"
docstring-quotes = "single"
multiline-quotes = "single"

[format]
skip-magic-trailing-comma = true
indent-style = "tab"
"#,
    )?;

    let test_path = tempdir.path().join("test.py");
    fs::write(
        &test_path,
        r#"
def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--no-cache", "--config"])
        .arg(&ruff_toml)
        .arg(test_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: The following rules may cause conflicts when used with the formatter: `COM812`, `ISC001`. To avoid unexpected behavior, we recommend disabling these rules, either by removing them from the `select` or `extend-select` configuration, or adding them to the `ignore` configuration.
    warning: The `format.indent-style="tab"` option is incompatible with `W191`, which lints against all uses of tabs. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `"space"`.
    warning: The `format.indent-style="tab"` option is incompatible with `D206`, with requires space-based indentation. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `"space"`.
    warning: The `flake8-quotes.inline-quotes="single"` option is incompatible with the formatter's `format.quote-style="double"`. We recommend disabling `Q000` and `Q003` when using the formatter, which enforces a consistent quote style. Alternatively, set both options to either `"single"` or `"double"`.
    warning: The `flake8-quotes.multiline-quotes="single"` option is incompatible with the formatter. We recommend disabling `Q001` when using the formatter, which enforces double quotes for multiline strings. Alternatively, set the `flake8-quotes.multiline-quotes` option to `"double"`.`
    warning: The `flake8-quotes.multiline-quotes="single"` option is incompatible with the formatter. We recommend disabling `Q002` when using the formatter, which enforces double quotes for docstrings. Alternatively, set the `flake8-quotes.docstring-quotes` option to `"double"`.`
    warning: The isort option `isort.lines-after-imports` with a value other than `-1`, `1` or `2` is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `2`, `1`, or `-1` (default).
    warning: The isort option `isort.lines-between-types` with a value greater than 1 is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `1` or `0` (default).
    warning: The isort option `isort.force-wrap-aliases` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.force-wrap-aliases=false` or `format.skip-magic-trailing-comma=false`.
    warning: The isort option `isort.split-on-trailing-comma` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.split-on-trailing-comma=false` or `format.skip-magic-trailing-comma=false`.
    "###);
    Ok(())
}

#[test]
fn conflicting_options_stdin() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
indent-width = 2

[lint]
select = ["ALL"]
ignore = ["D203", "D212"]

[lint.isort]
lines-after-imports = 3
lines-between-types = 2
force-wrap-aliases = true
combine-as-imports = true
split-on-trailing-comma = true

[lint.flake8-quotes]
inline-quotes = "single"
docstring-quotes = "single"
multiline-quotes = "single"

[format]
skip-magic-trailing-comma = true
indent-style = "tab"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--config"])
        .arg(&ruff_toml)
        .arg("-")
        .pass_stdin(r#"
def say_hy(name: str):
        print(f"Hy {name}")"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    def say_hy(name: str):
    	print(f"Hy {name}")

    ----- stderr -----
    warning: The following rules may cause conflicts when used with the formatter: `COM812`, `ISC001`. To avoid unexpected behavior, we recommend disabling these rules, either by removing them from the `select` or `extend-select` configuration, or adding them to the `ignore` configuration.
    warning: The `format.indent-style="tab"` option is incompatible with `W191`, which lints against all uses of tabs. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `"space"`.
    warning: The `format.indent-style="tab"` option is incompatible with `D206`, with requires space-based indentation. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `"space"`.
    warning: The `flake8-quotes.inline-quotes="single"` option is incompatible with the formatter's `format.quote-style="double"`. We recommend disabling `Q000` and `Q003` when using the formatter, which enforces a consistent quote style. Alternatively, set both options to either `"single"` or `"double"`.
    warning: The `flake8-quotes.multiline-quotes="single"` option is incompatible with the formatter. We recommend disabling `Q001` when using the formatter, which enforces double quotes for multiline strings. Alternatively, set the `flake8-quotes.multiline-quotes` option to `"double"`.`
    warning: The `flake8-quotes.multiline-quotes="single"` option is incompatible with the formatter. We recommend disabling `Q002` when using the formatter, which enforces double quotes for docstrings. Alternatively, set the `flake8-quotes.docstring-quotes` option to `"double"`.`
    warning: The isort option `isort.lines-after-imports` with a value other than `-1`, `1` or `2` is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `2`, `1`, or `-1` (default).
    warning: The isort option `isort.lines-between-types` with a value greater than 1 is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `1` or `0` (default).
    warning: The isort option `isort.force-wrap-aliases` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.force-wrap-aliases=false` or `format.skip-magic-trailing-comma=false`.
    warning: The isort option `isort.split-on-trailing-comma` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.split-on-trailing-comma=false` or `format.skip-magic-trailing-comma=false`.
    "###);
    Ok(())
}

#[test]
fn valid_linter_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["ALL"]
ignore = ["D203", "D212", "COM812", "ISC001"]

[lint.isort]
lines-after-imports = 2
lines-between-types = 1
force-wrap-aliases = true
combine-as-imports = true
split-on-trailing-comma = true

[lint.flake8-quotes]
inline-quotes = "single"
docstring-quotes = "double"
multiline-quotes = "double"

[format]
skip-magic-trailing-comma = false
quote-style = "single"
"#,
    )?;

    let test_path = tempdir.path().join("test.py");
    fs::write(
        &test_path,
        r#"
def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--no-cache", "--config"])
        .arg(&ruff_toml)
        .arg(test_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn valid_linter_options_preserve() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["Q"]

[lint.flake8-quotes]
inline-quotes = "single"
docstring-quotes = "single"
multiline-quotes = "single"

[format]
quote-style = "preserve"
"#,
    )?;

    let test_path = tempdir.path().join("test.py");
    fs::write(
        &test_path,
        r#"
def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--no-cache", "--config"])
        .arg(&ruff_toml)
        .arg(test_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn all_rules_default_options() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");

    fs::write(
        &ruff_toml,
        r#"
[lint]
select = ["ALL"]
"#,
    )?;

    let test_path = tempdir.path().join("test.py");
    fs::write(
        &test_path,
        r#"
def say_hy(name: str):
        print(f"Hy {name}")"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--no-cache", "--config"])
        .arg(&ruff_toml)
        .arg(test_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    warning: The following rules may cause conflicts when used with the formatter: `COM812`, `ISC001`. To avoid unexpected behavior, we recommend disabling these rules, either by removing them from the `select` or `extend-select` configuration, or adding them to the `ignore` configuration.
    "###);
    Ok(())
}

#[test]
fn test_diff() {
    let args = ["format", "--no-cache", "--isolated", "--diff"];
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
        --- resources/test/fixtures/unformatted.ipynb:cell 1
        +++ resources/test/fixtures/unformatted.ipynb:cell 1
        @@ -1,3 +1,4 @@
         import numpy
        -maths = (numpy.arange(100)**2).sum()
        -stats= numpy.asarray([1,2,3,4]).median()
        +
        +maths = (numpy.arange(100) ** 2).sum()
        +stats = numpy.asarray([1, 2, 3, 4]).median()
        --- resources/test/fixtures/unformatted.ipynb:cell 3
        +++ resources/test/fixtures/unformatted.ipynb:cell 3
        @@ -1,4 +1,6 @@
         # A cell with IPython escape command
         def some_function(foo, bar):
             pass
        +
        +
         %matplotlib inline
        --- resources/test/fixtures/unformatted.ipynb:cell 4
        +++ resources/test/fixtures/unformatted.ipynb:cell 4
        @@ -1,5 +1,10 @@
         foo = %pwd
        -def some_function(foo,bar,):
        +
        +
        +def some_function(
        +    foo,
        +    bar,
        +):
             # Another cell with IPython escape command
             foo = %pwd
             print(foo)

        --- resources/test/fixtures/unformatted.py
        +++ resources/test/fixtures/unformatted.py
        @@ -1,3 +1,3 @@
         x = 1
        -y=2
        +y = 2
         z = 3


        ----- stderr -----
        2 files would be reformatted, 1 file already formatted
        "###);
    });
}

#[test]
fn test_diff_no_change() {
    let args = ["format", "--no-cache", "--isolated", "--diff"];
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
    "###);
}

#[test]
fn test_notebook_trailing_semicolon() {
    let fixtures = Path::new("resources").join("test").join("fixtures");
    let unformatted = fs::read(fixtures.join("trailing_semicolon.ipynb")).unwrap();
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.ipynb"])
        .arg("-")
        .pass_stdin(unformatted), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
     "cells": [
      {
       "cell_type": "code",
       "execution_count": 1,
       "id": "4f8ce941-1492-4d4e-8ab5-70d733fe891a",
       "metadata": {},
       "outputs": [],
       "source": [
        "%config ZMQInteractiveShell.ast_node_interactivity=\"last_expr_or_assign\""
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 2,
       "id": "721ec705-0c65-4bfb-9809-7ed8bc534186",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "1"
          ]
         },
         "execution_count": 2,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "# Assignment statement without a semicolon\n",
        "x = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 3,
       "id": "de50e495-17e5-41cc-94bd-565757555d7e",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Assignment statement with a semicolon\n",
        "x = 1\n",
        "x = 1;"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 4,
       "id": "39e31201-23da-44eb-8684-41bba3663991",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "2"
          ]
         },
         "execution_count": 4,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "# Augmented assignment without a semicolon\n",
        "x += 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 5,
       "id": "6b73d3dd-c73a-4697-9e97-e109a6c1fbab",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Augmented assignment without a semicolon\n",
        "x += 1\n",
        "x += 1;  # comment\n",
        "# comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 6,
       "id": "2a3e5b86-aa5b-46ba-b9c6-0386d876f58c",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Multiple assignment without a semicolon\n",
        "x = y = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 7,
       "id": "07f89e51-9357-4cfb-8fc5-76fb75e35949",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Multiple assignment with a semicolon\n",
        "x = y = 1\n",
        "x = y = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 8,
       "id": "c22b539d-473e-48f8-a236-625e58c47a00",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Tuple unpacking without a semicolon\n",
        "x, y = 1, 2"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 9,
       "id": "12c87940-a0d5-403b-a81c-7507eb06dc7e",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Tuple unpacking with a semicolon (irrelevant)\n",
        "x, y = 1, 2\n",
        "x, y = 1, 2  # comment\n",
        "# comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 10,
       "id": "5a768c76-6bc4-470c-b37e-8cc14bc6caf4",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "1"
          ]
         },
         "execution_count": 10,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "# Annotated assignment statement without a semicolon\n",
        "x: int = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 11,
       "id": "21bfda82-1a9a-4ba1-9078-74ac480804b5",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Annotated assignment statement without a semicolon\n",
        "x: int = 1\n",
        "x: int = 1;  # comment\n",
        "# comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 12,
       "id": "09929999-ff29-4d10-ad2b-e665af15812d",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "1"
          ]
         },
         "execution_count": 12,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "# Assignment expression without a semicolon\n",
        "(x := 1)"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 13,
       "id": "32a83217-1bad-4f61-855e-ffcdb119c763",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Assignment expression with a semicolon\n",
        "(x := 1)\n",
        "(x := 1);  # comment\n",
        "# comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 14,
       "id": "61b81865-277e-4964-b03e-eb78f1f318eb",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "1"
          ]
         },
         "execution_count": 14,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "x = 1\n",
        "# Expression without a semicolon\n",
        "x"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 15,
       "id": "974c29be-67e1-4000-95fa-6ca118a63bad",
       "metadata": {},
       "outputs": [],
       "source": [
        "x = 1\n",
        "# Expression with a semicolon\n",
        "x;"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 16,
       "id": "cfeb1757-46d6-4f13-969f-a283b6d0304f",
       "metadata": {},
       "outputs": [],
       "source": [
        "class Point:\n",
        "    def __init__(self, x, y):\n",
        "        self.x = x\n",
        "        self.y = y\n",
        "\n",
        "\n",
        "p = Point(0, 0);"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 17,
       "id": "2ee7f1a5-ccfe-4004-bfa4-ef834a58da97",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Assignment statement where the left is an attribute access doesn't\n",
        "# print the value.\n",
        "p.x = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 18,
       "id": "3e49370a-048b-474d-aa0a-3d1d4a73ad37",
       "metadata": {},
       "outputs": [],
       "source": [
        "data = {}\n",
        "\n",
        "# Neither does the subscript node\n",
        "data[\"foo\"] = 1"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 19,
       "id": "d594bdd3-eaa9-41ef-8cda-cf01bc273b2d",
       "metadata": {},
       "outputs": [],
       "source": [
        "if x := 1:\n",
        "    # It should be the top level statement\n",
        "    x"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 20,
       "id": "e532f0cf-80c7-42b7-8226-6002fcf74fb6",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "1"
          ]
         },
         "execution_count": 20,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "# Parentheses with comments\n",
        "(\n",
        "    x := 1  # comment\n",
        ")  # comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 21,
       "id": "473c5d62-871b-46ed-8a34-27095243f462",
       "metadata": {},
       "outputs": [],
       "source": [
        "# Parentheses with comments\n",
        "(\n",
        "    x := 1  # comment\n",
        ");  # comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 22,
       "id": "8c3c2361-f49f-45fe-bbe3-7e27410a8a86",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "'Hello world!'"
          ]
         },
         "execution_count": 22,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "\"\"\"Hello world!\"\"\""
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 23,
       "id": "23dbe9b5-3f68-4890-ab2d-ab0dbfd0712a",
       "metadata": {},
       "outputs": [],
       "source": [
        "\"\"\"Hello world!\"\"\";  # comment\n",
        "# comment"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 24,
       "id": "3ce33108-d95d-4c70-83d1-0d4fd36a2951",
       "metadata": {},
       "outputs": [
        {
         "data": {
          "text/plain": [
           "'x = 1'"
          ]
         },
         "execution_count": 24,
         "metadata": {},
         "output_type": "execute_result"
        }
       ],
       "source": [
        "x = 1\n",
        "f\"x = {x}\""
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 25,
       "id": "654a4a67-de43-4684-824a-9451c67db48f",
       "metadata": {},
       "outputs": [],
       "source": [
        "x = 1\n",
        "f\"x = {x}\"\n",
        "f\"x = {x}\";  # comment\n",
        "# comment"
       ]
      }
     ],
     "metadata": {
      "kernelspec": {
       "display_name": "Python (ruff-playground)",
       "language": "python",
       "name": "ruff-playground"
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
       "version": "3.11.3"
      }
     },
     "nbformat": 4,
     "nbformat_minor": 5
    }

    ----- stderr -----
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
    "x=1"
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
        .current_dir(tempdir.path())
        .arg("format")
        .arg("--no-cache")
        .args(["--config", &ruff_toml.file_name().unwrap().to_string_lossy()])
        .args(["--extension", "ipy:ipynb"])
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn range_formatting() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=2:8-2:14"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't format this" )

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    def foo(
        arg1,
        arg2,
    ):
        print("Shouldn't format this" )


    ----- stderr -----
    "###);
}

#[test]
fn range_formatting_unicode() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=2:21-3"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1="👋🏽" ): print("Format this" )
"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    def foo(arg1="👋🏽" ):
        print("Format this")

    ----- stderr -----
    "###);
}

#[test]
fn range_formatting_multiple_files() -> std::io::Result<()> {
    let tempdir = TempDir::new()?;
    let file1 = tempdir.path().join("file1.py");

    fs::write(
        &file1,
        r#"
def file1(arg1, arg2,):
    print("Shouldn't format this" )

"#,
    )?;

    let file2 = tempdir.path().join("file2.py");

    fs::write(
        &file2,
        r#"
def file2(arg1, arg2,):
    print("Shouldn't format this" )

"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--range=1:8-1:15"])
        .arg(file1)
        .arg(file2),  @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: The `--range` option is only supported when formatting a single file but the specified paths resolve to 2 files.
    "###);

    Ok(())
}

#[test]
fn range_formatting_out_of_bounds() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=100:40-200:1"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't format this" )

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    def foo(arg1, arg2,):
        print("Shouldn't format this" )


    ----- stderr -----
    "###);
}

#[test]
fn range_start_larger_than_end() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=90-50"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't format this" )

"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '90-50' for '--range <RANGE>': the start position '90:1' is greater than the end position '50:1'.
        tip: Try switching start and end: '50:1-90:1'

    For more information, try '--help'.
    "###);
}

#[test]
fn range_line_numbers_only() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=2-3"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Shouldn't format this" )

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    def foo(
        arg1,
        arg2,
    ):
        print("Shouldn't format this" )


    ----- stderr -----
    "###);
}

#[test]
fn range_start_only() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=3"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Should format this" )

"#), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    def foo(arg1, arg2,):
        print("Should format this")

    ----- stderr -----
    "###);
}

#[test]
fn range_end_only() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=-3"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Should format this" )

"#), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    def foo(
        arg1,
        arg2,
    ):
        print("Should format this" )


    ----- stderr -----
    "#);
}

#[test]
fn range_missing_line() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=1-:20"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Should format this" )

"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1-:20' for '--range <RANGE>': the end line is not a valid number (cannot parse integer from empty string)
      tip: The format is 'line:column'.

    For more information, try '--help'.
    "###);
}

#[test]
fn zero_line_number() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=0:2"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Should format this" )

"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '0:2' for '--range <RANGE>': the start line is 0, but it should be 1 or greater.
      tip: The line numbers start at 1.
      tip: Try 1:2 instead.

    For more information, try '--help'.
    "###);
}

#[test]
fn column_and_line_zero() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--stdin-filename", "test.py", "--range=0:0"])
        .arg("-")
        .pass_stdin(r#"
def foo(arg1, arg2,):
    print("Should format this" )

"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '0:0' for '--range <RANGE>': the start line and column are both 0, but they should be 1 or greater.
      tip: The line and column numbers start at 1.
      tip: Try 1:1 instead.

    For more information, try '--help'.
    "###);
}

#[test]
fn range_formatting_notebook() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["format", "--isolated", "--no-cache", "--stdin-filename", "main.ipynb", "--range=1-2"])
        .arg("-")
        .pass_stdin(r#"
{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "id": "ad6f36d9-4b7d-4562-8d00-f15a0f1fbb6d",
   "metadata": {},
   "outputs": [],
   "source": [
    "x=1"
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
"#), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to format main.ipynb: Range formatting isn't supported for notebooks.
    "###);
}
