use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn cli_config_args_toml_string_basic() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    // Long flag
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=true"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // Short flag
    assert_cmd_snapshot!(case.command().arg("-c").arg("terminal.error-on-warning=true"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn cli_config_args_overrides_ty_toml() -> anyhow::Result<()> {
    let case = CliTest::with_files(vec![
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = true
            "#,
        ),
        ("test.py", r"print(x)  # [unresolved-reference]"),
    ])?;

    // Exit code of 1 due to the setting in `ty.toml`
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // Exit code of 0 because the `ty.toml` setting is overwritten by `--config`
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=false"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn cli_config_args_later_overrides_earlier() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=true").arg("--config").arg("terminal.error-on-warning=false"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn cli_config_args_invalid_option() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(1)")?;
    assert_cmd_snapshot!(case.command().arg("--config").arg("bad-option=true"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 1, column 1
      |
    1 | bad-option=true
      | ^^^^^^^^^^
    unknown field `bad-option`, expected one of `environment`, `src`, `rules`, `terminal`, `analysis`, `overrides`


    Usage: ty <COMMAND>

    For more information, try '--help'.
    ");

    Ok(())
}

#[test]
fn config_file_override() -> anyhow::Result<()> {
    // Set `error-on-warning` to true in the configuration file
    // Explicitly set `--warn unresolved-reference` to ensure the rule warns instead of errors
    let case = CliTest::with_files(vec![
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty-override.toml",
            r#"
            [terminal]
            error-on-warning = true
            "#,
        ),
    ])?;

    // Ensure flag works via CLI arg
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config-file").arg("ty-override.toml"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // Ensure the flag works via an environment variable
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").env("TY_CONFIG_FILE", "ty-override.toml"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}
