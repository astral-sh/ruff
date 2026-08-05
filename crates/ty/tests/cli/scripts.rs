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
fn verbose_rule_diagnostics_identify_script_metadata() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unresolved-reference = "warn"
        # ///

        print(missing)
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--verbose"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `missing` used when not defined
     --> script.py:7:7
      |
    7 | print(missing)
      |       ^^^^^^^
    info: rule `unresolved-reference` was selected in script metadata

    Found 1 diagnostic

    ----- stderr -----
    INFO Indexed 1 file(s) in 0.000s
    ");

    Ok(())
}

#[test]
fn unknown_rule_diagnostics_point_to_script_metadata() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.rules]
        # unknown-script-rule = "warn"
        # ///
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unknown-rule]: Unknown rule `unknown-script-rule`
     --> script.py:4:3
      |
    4 | # unknown-script-rule = "warn"
      |   ^^^^^^^^^^^^^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn python_version_diagnostics_identify_script_metadata() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # requires-python = ">=3.12"
        # ///

        PythonFinalizationError
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `PythonFinalizationError` used when not defined
     --> script.py:6:1
      |
    6 | PythonFinalizationError
      | ^^^^^^^^^^^^^^^^^^^^^^^
    info: `PythonFinalizationError` was added as a builtin in Python 3.13
    info: Python 3.12 was assumed when resolving types because it was specified in script metadata
     --> script.py:3:21
      |
    3 | # requires-python = ">=3.12"
      |                     ^^^^^^^^ Python version configured here

    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    script.py:6:1: error[unresolved-reference] Name `PythonFinalizationError` used when not defined
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
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # requires-python = ">=3.13"
        #
        # [tool.ty.environment]
        # python-version = "3.11"
        # ///

        import sys
        from typing import reveal_type

        reveal_type(sys.version_info[:2])
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> script.py:12:13
       |
    12 | reveal_type(sys.version_info[:2])
       |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[11]]`

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

#[test]
fn explicit_config_replaces_the_script_environment() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "explicit.toml",
            r#"
            [environment]
            python-version = "3.12"
            python-platform = "linux"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # requires-python = ">=3.13"
            # [tool.ty.environment]
            # python-platform = "win32"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.version_info[:2])
            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--config-file").arg("explicit.toml"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> script.py:11:13
       |
    11 | reveal_type(sys.version_info[:2])
       |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[12]]`

    info[revealed-type]: Revealed type
      --> script.py:12:13
       |
    12 | reveal_type(sys.platform)
       |             ^^^^^^^^^^^^ `Literal["linux"]`

    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn cli_arguments_override_script_environment() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # requires-python = ">=3.13"
        # [tool.ty.environment]
        # python-platform = "win32"
        # ///

        import sys
        from typing import reveal_type

        reveal_type(sys.version_info[:2])
        reveal_type(sys.platform)
        "#,
    )?;

    assert_cmd_snapshot!(
        case.command()
            .arg("--python-version")
            .arg("3.12")
            .arg("--python-platform")
            .arg("linux"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> script.py:11:13
       |
    11 | reveal_type(sys.version_info[:2])
       |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[12]]`

    info[revealed-type]: Revealed type
      --> script.py:12:13
       |
    12 | reveal_type(sys.platform)
       |             ^^^^^^^^^^^^ `Literal["linux"]`

    Found 2 diagnostics

    ----- stderr -----
    "#
    );

    Ok(())
}

#[test]
fn script_version_and_platform_are_isolated_from_project_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.12"
            python-platform = "linux"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # requires-python = ">=3.13"
            # [tool.ty.environment]
            # python-version = "3.11"
            # python-platform = "win32"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.version_info[:2])
            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> script.py:12:13
       |
    12 | reveal_type(sys.version_info[:2])
       |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[11]]`

    info[revealed-type]: Revealed type
      --> script.py:13:13
       |
    13 | reveal_type(sys.platform)
       |             ^^^^^^^^^^^^ `Literal["win32"]`

    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn python_requirement_overrides_user_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # requires-python = ">=3.13"
        # ///

        import sys
        from typing import reveal_type

        reveal_type(sys.version_info[:2])
        "#,
    )?;
    case.write_file(
        case.user_config_directory().join("ty/ty.toml"),
        r#"
        [environment]
        python-version = "3.12"
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> script.py:9:13
      |
    9 | reveal_type(sys.version_info[:2])
      |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[13]]`

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn scripts_have_no_implicit_first_party_roots() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("shared.py", "value = 1\n"),
        ("src/layout_dependency.py", "value = 1\n"),
        ("scripts/local_dependency.py", "value = 1\n"),
        (
            "scripts/script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            from layout_dependency import value as layout_value
            from local_dependency import value as local_value
            from shared import value
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `layout_dependency`
     --> scripts/script.py:6:6
      |
    6 | from layout_dependency import value as layout_value
      |      ^^^^^^^^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    error[unresolved-import]: Cannot resolve imported module `local_dependency`
     --> scripts/script.py:7:6
      |
    7 | from local_dependency import value as local_value
      |      ^^^^^^^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    error[unresolved-import]: Cannot resolve imported module `shared`
     --> scripts/script.py:8:6
      |
    8 | from shared import value
      |      ^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 3 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn configured_source_roots_and_extra_paths_are_relative_to_the_script() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("scripts/source/first_party.py", "value = 1\n"),
        ("scripts/extra/dependency.py", "value = 1\n"),
        (
            "scripts/script.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # root = ["./source"]
            # extra-paths = ["./extra"]
            # ///

            from dependency import value as dependency
            from first_party import value as first_party
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn project_search_paths_do_not_apply_to_scripts() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            root = ["./project-source"]
            extra-paths = ["./project-extra"]
            "#,
        ),
        ("project-source/project_only.py", "value = 1\n"),
        ("project-extra/extra_only.py", "value = 1\n"),
        (
            "ordinary.py",
            "from extra_only import value as extra\nfrom project_only import value as project\n",
        ),
        (
            "scripts/script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            from extra_only import value as extra
            from project_only import value as project
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `extra_only`
     --> scripts/script.py:6:6
      |
    6 | from extra_only import value as extra
      |      ^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    error[unresolved-import]: Cannot resolve imported module `project_only`
     --> scripts/script.py:7:6
      |
    7 | from project_only import value as project
      |      ^^^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn shared_imports_use_each_scripts_platform() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "shared.py",
            r#"
            import sys

            if sys.platform == "win32":
                value = "windows"
            else:
                value = "other"
            "#,
        ),
        (
            "windows.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # extra-paths = ["."]
            # python-platform = "win32"
            # ///

            from shared import value
            from typing import reveal_type

            reveal_type(value)
            "#,
        ),
        (
            "linux.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # extra-paths = ["."]
            # python-platform = "linux"
            # ///

            from shared import value
            from typing import reveal_type

            reveal_type(value)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
      --> linux.py:11:13
       |
    11 | reveal_type(value)
       |             ^^^^^ `Literal["other"]`

    info[revealed-type]: Revealed type
      --> windows.py:11:13
       |
    11 | reveal_type(value)
       |             ^^^^^ `Literal["windows"]`

    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn inherited_file_settings_are_relative_to_the_script() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("user-extra/project_dependency.py", "value = 1\n"),
        ("scripts/user-extra/user_dependency.py", "value = 1\n"),
        ("cli-extra/cli_dependency.py", "value = 1\n"),
        (
            "scripts/script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            from user_dependency import value as user_value
            from cli_dependency import value as cli_value
            "#,
        ),
    ])?;
    case.write_file(
        case.user_config_directory().join("ty/ty.toml"),
        r#"
        [environment]
        extra-paths = ["./user-extra"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--extra-search-path").arg("./cli-extra"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn scripts_do_not_use_an_inactive_project_environment() -> anyhow::Result<()> {
    let dependency = if cfg!(windows) {
        ".venv/Lib/site-packages/project_dependency.py"
    } else {
        ".venv/lib/python3.13/site-packages/project_dependency.py"
    };

    let case = CliTest::with_files([
        (".venv/pyvenv.cfg", "home = ./\nversion = 3.13\n"),
        (dependency, "value = 1\n"),
        (
            "scripts/script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            from project_dependency import value
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `project_dependency`
     --> scripts/script.py:6:6
      |
    6 | from project_dependency import value
      |      ^^^^^^^^^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn scripts_use_an_activated_virtual_environment() -> anyhow::Result<()> {
    let dependency = if cfg!(windows) {
        ".venv/Lib/site-packages/project_dependency.py"
    } else {
        ".venv/lib/python3.13/site-packages/project_dependency.py"
    };

    let case = CliTest::with_files([
        (".venv/pyvenv.cfg", "home = ./\nversion = 3.13\n"),
        (dependency, "value = 1\n"),
        (
            "scripts/script.py",
            r#"
            # /// script
            # dependencies = []
            # ///

            from project_dependency import value
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().env("VIRTUAL_ENV", case.root().join(".venv")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn invalid_toml_reports_configuration_error() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # requires-python =
        # ///

        print(missing)
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-script-metadata]: string values must be quoted, expected literal string
     --> script.py:3:20
      |
    3 | # requires-python =
      |                    ^

    Found 1 diagnostic

    ----- stderr -----
    ");
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    script.py:3:20: error[invalid-script-metadata] string values must be quoted, expected literal string
    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn invalid_metadata_options_report_configuration_error() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        r#"
        # /// script
        # [tool.ty.environment]
        # python-version = true
        # ///

        print(missing)
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-script-metadata]: wanted string or table
     --> script.py:4:20
      |
    4 | # python-version = true
      |                    ^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn invalid_python_requirement_reports_configuration_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-platform = "linux"

            [tool.ty.rules]
            unresolved-reference = "error"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # requires-python = "<3.12"
            # [tool.ty.environment]
            # python-platform = "win32"
            # [tool.ty.rules]
            # unresolved-reference = "warn"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.platform)
            print(missing)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-script-metadata]: value `<3.12` does not contain a lower bound
     --> script.py:3:21
      |
    3 | # requires-python = "<3.12"
      |                     ^^^^^^^
    info: Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.13`) or specify a version in `environment.python-version`.

    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    script.py:3:21: error[invalid-script-metadata] value `<3.12` does not contain a lower bound
    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn invalid_script_settings_report_configuration_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-platform = "linux"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # [tool.ty.src]
            # include = ["src/**test/"]
            # [tool.ty.environment]
            # python-platform = "win32"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-glob]: Invalid pattern
     --> script.py:4:14
      |
    4 | # include = ["src/**test/"]
      |              ^^^^^^^^^^^^^ Too many stars at position 5

    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn invalid_script_environment_reports_configuration_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.13"
            python-platform = "linux"
            "#,
        ),
        (
            "script.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # python = "./missing-environment"
            # python-version = "3.12"
            # python-platform = "win32"
            # ///

            import sys
            from typing import reveal_type

            reveal_type(sys.version_info[:2])
            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-script-metadata]: Invalid `environment.python` setting in script metadata `<temp_dir>/missing-environment`: does not point to a Python executable or a directory on disk
     --> script.py:4:12
      |
    4 | # python = "./missing-environment"
      |            ^^^^^^^^^^^^^^^^^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn invalid_script_search_paths_do_not_blame_python_environment() -> anyhow::Result<()> {
    let dependency = if cfg!(windows) {
        "environment/Lib/site-packages/dependency.py"
    } else {
        "environment/lib/python3.13/site-packages/dependency.py"
    };

    let case = CliTest::with_files([
        ("environment/pyvenv.cfg", "home = ./\nversion = 3.13\n"),
        (dependency, "value = 1\n"),
        (
            "script.py",
            r#"
            # /// script
            # [tool.ty.environment]
            # python = "./environment"
            # typeshed = "./missing-typeshed"
            # ///

            print(missing)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-script-metadata]: Failed to read the custom typeshed versions file '<temp_dir>/missing-typeshed/stdlib/VERSIONS'
     --> script.py:5:14
      |
    5 | # typeshed = "./missing-typeshed"
      |              ^^^^^^^^^^^^^^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    script.py:5:14: error[invalid-script-metadata] Failed to read the custom typeshed versions file '<temp_dir>/missing-typeshed/stdlib/VERSIONS'
    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}
