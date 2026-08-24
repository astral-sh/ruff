use insta_cmd::assert_cmd_snapshot;
use ty_static::EnvVars;

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

#[test]
fn unavailable_uv_reports_metadata_error() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        "#!/usr/bin/env python3\n\n# /// script\n# dependencies = []\n# ///\nprint(missing)\n",
    )?
    .with_filter(
        "program not found",
        "No such file or directory (os error 2)",
    );

    assert_cmd_snapshot!(
        case.command()
            .arg("script.py")
            .env(EnvVars::TY_UV, "1")
            .env(EnvVars::UV, "missing-uv-executable"),
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[uv-metadata]: Failed to invoke `uv workspace metadata`: No such file or directory (os error 2)
    --> script.py:3:1

    Found 1 diagnostic

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn ordinary_files_do_not_initialize_scripts() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "script.py",
            "# /// script\n# dependencies = []\n# ///\nvalue = 1\n",
        ),
        ("ordinary.py", "value = 1\n"),
    ])?;
    assert_cmd_snapshot!(
        case.command()
            .arg("ordinary.py")
            .env(EnvVars::TY_UV, "1")
            .env(EnvVars::UV, "missing-uv-executable"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn disabled_integration_does_not_initialize_scripts() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "script.py",
        "# /// script\n# dependencies = []\n# ///\nvalue = 1\n",
    )?;

    assert_cmd_snapshot!(
        case.command()
            .arg("script.py")
            .env(EnvVars::UV, "missing-uv-executable"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn excluded_scripts_do_not_initialize_their_environments() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("ty.toml", "[src]\nexclude-scripts = true\n"),
        (
            "script.py",
            "# /// script\n# dependencies = []\n# ///\nvalue = 1\n",
        ),
        ("ordinary.py", "value = 1\n"),
    ])?;
    assert_cmd_snapshot!(
        case.command()
            .arg(".")
            .args(["--config-file", "ty.toml"])
            .env(EnvVars::TY_UV, "1")
            .env(EnvVars::UV, "missing-uv-executable"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "
    );

    Ok(())
}

#[cfg(feature = "test-uv")]
mod uv_metadata {
    use std::{fs, process::Command};

    use insta_cmd::assert_cmd_snapshot;
    use ty_static::EnvVars;

    use crate::CliTest;

    fn command_with_script_uv(case: &CliTest) -> Command {
        let mut command = case.command_inheriting_environment();
        command
            .env(EnvVars::TY_UV, "1")
            .env(EnvVars::UV, "uv")
            .env("UV_CACHE_DIR", case.root().join("cache"));
        command
    }

    fn assert_uv_supports_script_metadata() -> anyhow::Result<()> {
        let output = Command::new("uv")
            .args(["workspace", "metadata", "--help"])
            .output()?;

        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("--script"),
            "installed uv does not support script metadata"
        );

        Ok(())
    }

    #[test]
    fn uses_uv_script_environment_and_python_version() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_file(
            "script.py",
            r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["attrs==25.4.0"]
        # [tool.ty.environment]
        # python-version = "3.10"
        # ///

        import sys
        from attrs import define
        from typing import reveal_type

        @define
        class User:
            value: int

        reveal_type(User(1).value)
        reveal_type(sys.version_info[:2])
        "#,
        )?
        .with_filter(r"Literal\[(?:1[2-9]|[2-9][0-9])\]", "Literal[<uv-minor>]");

        assert_cmd_snapshot!(command_with_script_uv(&case).arg("script.py"), @"
        success: true
        exit_code: 0
        ----- stdout -----
        info[revealed-type]: Revealed type
          --> script.py:17:13
           |
        17 | reveal_type(User(1).value)
           |             ^^^^^^^^^^^^^ `int`

        info[revealed-type]: Revealed type
          --> script.py:18:13
           |
        18 | reveal_type(sys.version_info[:2])
           |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[<uv-minor>]]`

        Found 2 diagnostics

        ----- stderr -----
        ");

        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .arg("script.py")
                .args(["--python-version", "3.11"]),
            @"
        success: true
        exit_code: 0
        ----- stdout -----
        info[revealed-type]: Revealed type
          --> script.py:17:13
           |
        17 | reveal_type(User(1).value)
           |             ^^^^^^^^^^^^^ `int`

        info[revealed-type]: Revealed type
          --> script.py:18:13
           |
        18 | reveal_type(sys.version_info[:2])
           |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[11]]`

        Found 2 diagnostics

        ----- stderr -----
        "
        );

        Ok(())
    }

    #[test]
    fn imported_script_environment() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_files([
            ("a.py", "from b import foo\nprint(foo)\n"),
            (
                "b.py",
                r#"
                # /// script
                # requires-python = "==3.12.*"
                # dependencies = []
                # [tool.ty.environment]
                # python-version = "3.11"
                # ///
                import sys
                from typing import Literal, assert_type

                foo = 1
                assert_type(sys.version_info[:2], tuple[Literal[3], Literal[12]])
                "#,
            ),
        ])?;

        // FIXME: Checking a.py can create b.py's environment before synchronization, causing
        // b.py to use its Python 3.11 fallback instead of the Python 3.12 environment from uv.
        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .args(["a.py", "b.py"])
                .env(EnvVars::TY_UV, "scripts")
                .env(EnvVars::TY_MAX_PARALLELISM, "1"),
            @"
        success: false
        exit_code: 1
        ----- stdout -----
        error[type-assertion-failure]: Argument does not have asserted type `tuple[Literal[3], Literal[12]]`
          --> b.py:12:1
           |
        12 | assert_type(sys.version_info[:2], tuple[Literal[3], Literal[12]])
           | ^^^^^^^^^^^^--------------------^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
           |             |
           |             Inferred type is `tuple[Literal[3], Literal[11]]`
        info: `tuple[Literal[3], Literal[12]]` and `tuple[Literal[3], Literal[11]]` are not equivalent types

        Found 1 diagnostic

        ----- stderr -----
        "
        );

        Ok(())
    }

    #[test]
    fn synchronizes_imported_script_with_one_worker() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_files([
            ("a.py", "from b import foo\nprint(foo)\n"),
            (
                "b.py",
                r#"
                # /// script
                # requires-python = "==3.12.*"
                # dependencies = []
                # ///
                foo = 1
                "#,
            ),
        ])?;

        // Starting with the script must not run its importer on the same Rayon worker while
        // initialization is waiting for uv. The importer would wait for that initialization.
        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .args(["b.py", "a.py"])
                .env(EnvVars::TY_UV, "scripts")
                .env(EnvVars::TY_MAX_PARALLELISM, "1"),
            @"
        success: true
        exit_code: 0
        ----- stdout -----
        All checks passed!

        ----- stderr -----
        "
        );

        Ok(())
    }

    #[test]
    fn synchronizes_multiple_scripts_with_one_worker() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let script =
            "# /// script\n# dependencies = ['attrs==25.4.0']\n# ///\nfrom attrs import define\n";
        let case = CliTest::with_files([("first.py", script), ("second.py", script)])?;

        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .args(["first.py", "second.py"])
                .env(EnvVars::TY_MAX_PARALLELISM, "1"),
            @"
        success: true
        exit_code: 0
        ----- stdout -----
        All checks passed!

        ----- stderr -----
        "
        );

        Ok(())
    }

    #[test]
    fn cli_python_selects_script_interpreter_without_replacing_its_environment()
    -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_file(
            "scripts/script.py",
            r#"
            # /// script
            # requires-python = ">=3.11"
            # dependencies = ["attrs==25.4.0"]
            # ///

            import sys
            from attrs import define
            from typing import reveal_type

            @define
            class User:
                value: int

            reveal_type(User(1).value)
            reveal_type(sys.version_info[:2])
            "#,
        )?;

        // The CLI environment selects uv's interpreter, but the script's dependencies must still
        // come from the separate environment that uv creates for the script.
        let environment = case.root().join(".venv");
        let output = Command::new("uv")
            .args(["venv", "--no-project", "--python", "3.12"])
            .arg(&environment)
            .env("UV_CACHE_DIR", case.root().join("cache"))
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "failed to create project environment: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .arg("scripts/script.py")
                .args(["--python", ".venv"]),
            @"
        success: true
        exit_code: 0
        ----- stdout -----
        info[revealed-type]: Revealed type
          --> scripts/script.py:15:13
           |
        15 | reveal_type(User(1).value)
           |             ^^^^^^^^^^^^^ `int`

        info[revealed-type]: Revealed type
          --> scripts/script.py:16:13
           |
        16 | reveal_type(sys.version_info[:2])
           |             ^^^^^^^^^^^^^^^^^^^^ `tuple[Literal[3], Literal[12]]`

        Found 2 diagnostics

        ----- stderr -----
        "
        );

        Ok(())
    }

    #[test]
    fn fixes_script_using_uv_environment() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_file(
            "script.py",
            r#"
            # /// script
            # requires-python = ">=3.12"
            # dependencies = ["attrs==25.4.0"]
            # ///

            from attrs import define

            @define
            class User:
                value: int

            User(1)  # ty: ignore[unresolved-reference]
            "#,
        )?;

        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .arg("script.py")
                .arg("--fix")
                .args(["--warn", "unused-ignore-comment"]),
            @"
        success: true
        exit_code: 0
        ----- stdout -----
        Found 1 diagnostic (1 fixed, 0 remaining).

        ----- stderr -----
        "
        );

        let updated = fs::read_to_string(case.root().join("script.py"))?;
        assert!(!updated.contains("ty: ignore"));

        Ok(())
    }

    #[test]
    fn failed_uv_script_synchronization_reports_an_error() -> anyhow::Result<()> {
        assert_uv_supports_script_metadata()?;

        let case = CliTest::with_file(
            "script.py",
            "# /// script\n# requires-python = '>=3.8'\n# dependencies = ['missing-script-dependency==99.0.0']\n# ///\nprint(missing)\n",
        )?
        .with_filter(
            r"(?s)`uv workspace metadata` failed with status.*?missing-script-dependency==99\.0\.0.*?\n(Found 1 diagnostic)",
            "`uv workspace metadata` failed: missing-script-dependency==99.0.0 could not be resolved\n$1",
        );
        assert_cmd_snapshot!(
            command_with_script_uv(&case)
                .arg("script.py")
                .env("UV_OFFLINE", "1"),
            @"
        success: false
        exit_code: 1
        ----- stdout -----
        error[uv-metadata]: `uv workspace metadata` failed: missing-script-dependency==99.0.0 could not be resolved
        Found 1 diagnostic

        ----- stderr -----
        "
        );

        Ok(())
    }
}
