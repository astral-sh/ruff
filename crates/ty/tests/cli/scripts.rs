use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn project_settings_and_overrides_do_not_apply() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            unresolved-reference = "ignore"

            [[tool.ty.overrides]]
            include = ["script.py"]

            [tool.ty.overrides.rules]
            unresolved-reference = "error"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # [tool.ty.rules]
            # unresolved-reference = "warn"
            # ///

            print(missing)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
     --> script.py:7:7
      |
    7 | print(missing)
      |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn metadata_without_tool_ty_uses_default_settings() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            all = "ignore"

            [tool.ty.analysis]
            respect-type-ignore-comments = false
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            value: int = "not an int"
            suppressed: int = "not an int"  # type: ignore
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-assignment]: Object of type `Literal["not an int"]` is not assignable to `int`
     --> script.py:6:14
      |
    6 | value: int = "not an int"
      |        ---   ^^^^^^^^^^^^ Incompatible value of type `Literal["not an int"]`
      |        |
      |        Declared type

    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn environment_options() -> anyhow::Result<()> {
    // TODO: This is not yet supported, but we should support this.
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.12"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # requires-python = ">=3.7"
            #
            # [tool.ty.environment]
            # python-version = "3.7"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.version_info[:2] == (3, 12))
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> script.py:12:13
       |
    12 | reveal_type(sys.version_info[:2] == (3, 12))
       |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `Literal[True]`

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn inline_overrides_are_ignored() -> anyhow::Result<()> {
    // TODO: Emit a diagnostic for options that are not allowed within scripts.
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unresolved-reference = "warn"
        #
        # [[tool.ty.overrides]]
        # include = ["script.py"]
        #
        # [tool.ty.overrides.rules]
        # unresolved-reference = "ignore"
        # ///

        print(missing)
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
      --> script.py:13:7
       |
    13 | print(missing)
       |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn inline_terminal_settings_do_not_apply() -> anyhow::Result<()> {
    // TODO: Either support (when calling `ty check <script>`), or raise a diagnostic that this option is not supported
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unresolved-reference = "warn"
        #
        # [tool.ty.terminal]
        # error-on-warning = false
        # ///

        print(missing)
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
      --> script.py:10:7
       |
    10 | print(missing)
       |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn inline_settings_override_user_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unresolved-reference = "error"
        #
        # [tool.ty.analysis]
        # respect-type-ignore-comments = false
        # ///

        print(missing)  # type: ignore
        "#,
    )?;
    case.write_file(
        case.user_config_directory().join("ty/ty.toml"),
        r#"
        [rules]
        unresolved-reference = "ignore"

        [analysis]
        respect-type-ignore-comments = true
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `missing` used when not defined
      --> script.py:10:7
       |
    10 | print(missing)  # type: ignore
       |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn user_configuration_applies() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # dependencies = []
        # ///

        print(missing)
        print(suppressed)  # type: ignore
        "#,
    )?;
    case.write_file(
        case.user_config_directory().join("ty/ty.toml"),
        r#"
        [rules]
        unresolved-reference = "warn"

        [analysis]
        respect-type-ignore-comments = false
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
     --> script.py:6:7
      |
    6 | print(missing)
      |       ^^^^^^^

    warning[unresolved-reference]: Name `suppressed` used when not defined
     --> script.py:7:7
      |
    7 | print(suppressed)  # type: ignore
      |       ^^^^^^^^^^

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn cli_arguments_override_script_options() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unresolved-reference = "ignore"
        #
        # [tool.ty.analysis]
        # respect-type-ignore-comments = false
        # ///

        print(missing)
        print(suppressed)  # type: ignore
        "#,
    )?;

    assert_cmd_snapshot!(
        case.command()
            .arg("--warn")
            .arg("unresolved-reference")
            .arg("--config")
            .arg("analysis.respect-type-ignore-comments=true"),
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
      --> script.py:10:7
       |
    10 | print(missing)
       |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn explicit_config_replaces_inline_metadata() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "explicit.toml",
            r#"
            [rules]
            unresolved-reference = "warn"

            [analysis]
            respect-type-ignore-comments = true
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # [tool.ty.rules]
            # unresolved-reference = "ignore"
            #
            # [tool.ty.analysis]
            # respect-type-ignore-comments = false
            # ///

            print(missing)
            print(suppressed)  # type: ignore
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(
        case.command().arg("--config-file").arg("explicit.toml"),
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
      --> script.py:10:7
       |
    10 | print(missing)
       |       ^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    "
    );

    Ok(())
}
