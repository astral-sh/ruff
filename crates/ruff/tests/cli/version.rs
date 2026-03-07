use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

const VERSION_FILTER: (&str, &str) = (
    r"\d+\.\d+\.\d+(\+\d+)?( \(\w{9} \d\d\d\d-\d\d-\d\d\))?",
    "[VERSION]",
);

fn version_test() -> anyhow::Result<VersionTest> {
    VersionTest::new()
}

#[test]
fn version_basics() -> anyhow::Result<()> {
    let test = version_test()?;
    assert_cmd_snapshot!(
        test.command().arg("version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff [VERSION]

    ----- stderr -----
    "
    );
    Ok(())
}

/// `--config` is a global option,
/// so it's allowed to pass --config to subcommands such as `version`
/// -- the flag is simply ignored
#[test]
fn config_option_allowed_but_ignored() -> anyhow::Result<()> {
    let test = VersionTest::with_file("ruff.toml", "")?;
    let ruff_dot_toml = test.root().join("ruff.toml");

    assert_cmd_snapshot!(
        test.command()
            .arg("version")
            .arg("--config")
            .arg(&ruff_dot_toml)
            .args(["--config", "lint.isort.extra-standard-library = ['foo', 'bar']"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff [VERSION]

    ----- stderr -----
    "
    );
    Ok(())
}

#[test]
fn config_option_ignored_but_validated() -> anyhow::Result<()> {
    let test = version_test()?;
    assert_cmd_snapshot!(
        test.command()
            .arg("version")
            .args(["--config", "foo = bar"]), @"
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
    "
    );
    Ok(())
}

/// `--isolated` is also a global option,
#[test]
fn isolated_option_allowed() -> anyhow::Result<()> {
    let test = version_test()?;
    assert_cmd_snapshot!(
        test.command().arg("version").arg("--isolated"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff [VERSION]

    ----- stderr -----
    "
    );
    Ok(())
}

struct VersionTest {
    cli_test: CliTest,
}

impl VersionTest {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            cli_test: CliTest::with_settings(|_, mut settings| {
                settings.add_filter(VERSION_FILTER.0, VERSION_FILTER.1);
                settings
            })?,
        })
    }

    fn with_file(path: impl AsRef<std::path::Path>, content: &str) -> anyhow::Result<Self> {
        let test = Self::new()?;
        test.write_file(path, content)?;
        Ok(test)
    }

    fn command(&self) -> std::process::Command {
        self.cli_test.command()
    }
}

impl std::ops::Deref for VersionTest {
    type Target = CliTest;

    fn deref(&self) -> &Self::Target {
        &self.cli_test
    }
}
