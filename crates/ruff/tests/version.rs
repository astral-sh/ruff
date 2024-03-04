//! Tests for the --version command
use std::fs;
use std::process::Command;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";
const VERSION_FILTER: [(&str, &str); 1] = [(
    r"\d+\.\d+\.\d+(\+\d+)?( \(\w{9} \d\d\d\d-\d\d-\d\d\))?",
    "[VERSION]",
)];

#[test]
fn version_basics() {
    insta::with_settings!({filters => VERSION_FILTER.to_vec()}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME)).arg("version"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        ruff [VERSION]

        ----- stderr -----
        "###
        );
    });
}

/// `--config` is a global option,
/// so it's allowed to pass --config to subcommands such as `version`
/// -- the flag is simply ignored
#[test]
fn config_option_allowed_but_ignored() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_dot_toml = tempdir.path().join("ruff.toml");
    fs::File::create(&ruff_dot_toml)?;
    insta::with_settings!({filters => VERSION_FILTER.to_vec()}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME))
                .arg("version")
                .arg("--config")
                .arg(&ruff_dot_toml)
                .args(["--config", "lint.isort.extra-standard-library = ['foo', 'bar']"]), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        ruff [VERSION]

        ----- stderr -----
        "###
        );
    });
    Ok(())
}
#[test]
fn config_option_ignored_but_validated() {
    insta::with_settings!({filters => VERSION_FILTER.to_vec()}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME))
                .arg("version")
                .args(["--config", "foo = bar"]), @r###"
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
        "###
        );
    });
}

/// `--isolated` is also a global option,
#[test]
fn isolated_option_allowed() {
    insta::with_settings!({filters => VERSION_FILTER.to_vec()}, {
        assert_cmd_snapshot!(
            Command::new(get_cargo_bin(BIN_NAME)).arg("version").arg("--isolated"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        ruff [VERSION]

        ----- stderr -----
        "###
        );
    });
}
