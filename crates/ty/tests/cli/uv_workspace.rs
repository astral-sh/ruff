//! Integration tests for ty's side of `uv check`.
//!
//! Corresponding uv-side workspace tests live at
//! <https://github.com/astral-sh/uv/blob/main/crates/uv/tests/project/check.rs>.

#[cfg(feature = "test-uv")]
use std::{fmt::Write as _, fs::File, io::Write, path::Path, process::Command};

use insta_cmd::assert_cmd_snapshot;
use ty_static::EnvVars;
#[cfg(feature = "test-uv")]
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::CliTest;

fn workspace_case() -> anyhow::Result<CliTest> {
    CliTest::with_files([
        (
            "pyproject.toml",
            r#"
[tool.uv.workspace]
members = ["packages/*"]
"#,
        ),
        (
            "packages/member/pyproject.toml",
            r#"
[project]
name = "member"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "packages/member/member.py",
            "value: int = 'selected-member'",
        ),
        (
            "packages/sibling/pyproject.toml",
            r#"
[project]
name = "sibling"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "packages/sibling/sibling.py",
            "value: int = 'unselected-sibling'",
        ),
    ])
}

#[cfg(feature = "test-uv")]
fn uv_command(case: &CliTest) -> Command {
    let mut command = Command::new("uv");
    command
        .current_dir(case.root())
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never");
    command
}

#[cfg(feature = "test-uv")]
fn uv_sync_command(case: &CliTest, virtual_env: Option<&Path>) -> anyhow::Result<Command> {
    let mut sync = uv_command(case);
    sync.args(["workspace", "metadata", "--sync"]);
    if let Some(virtual_env) = virtual_env {
        sync.arg("--active").env("VIRTUAL_ENV", virtual_env);
    }
    anyhow::ensure!(
        sync.output()?.status.success(),
        "failed to prepare uv workspace"
    );

    let mut command = case.command();
    command
        .env("TY_UV", "1")
        .env("UV", "uv")
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never")
        .env("TY_OUTPUT_FORMAT", "concise")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    #[cfg(windows)]
    if let Some(path_ext) = std::env::var_os("PATHEXT") {
        command.env("PATHEXT", path_ext);
    }
    if let Some(virtual_env) = virtual_env {
        command.env("VIRTUAL_ENV", virtual_env);
    }

    Ok(command)
}

#[cfg(feature = "test-uv")]
fn write_dependency_wheel(
    case: &CliTest,
    distribution: &str,
    module: &str,
    dependencies: &[&str],
) -> anyhow::Result<()> {
    let wheel_directory = case.root().join("wheels");
    std::fs::create_dir_all(&wheel_directory)?;

    let prefix = format!("{}-0.1.0", distribution.replace('-', "_"));
    let mut wheel = ZipWriter::new(File::create(
        wheel_directory.join(format!("{prefix}-py3-none-any.whl")),
    )?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut metadata = format!("Metadata-Version: 2.1\nName: {distribution}\nVersion: 0.1.0\n");
    for dependency in dependencies {
        writeln!(metadata, "Requires-Dist: {dependency}")?;
    }
    let mut record = Vec::new();

    for (path, contents) in [
        (format!("{module}.py"), "value: int = 1\n"),
        (format!("{prefix}.dist-info/METADATA"), metadata.as_str()),
        (
            format!("{prefix}.dist-info/WHEEL"),
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ),
    ] {
        wheel.start_file(&path, options)?;
        wheel.write_all(contents.as_bytes())?;
        record.push(format!("{path},,"));
    }

    let record_path = format!("{prefix}.dist-info/RECORD");
    record.push(format!("{record_path},,"));
    wheel.start_file(record_path, options)?;
    writeln!(wheel, "{}", record.join("\n"))?;
    wheel.finish()?;

    Ok(())
}

#[cfg(feature = "test-uv")]
fn dependency_workspace_case() -> anyhow::Result<CliTest> {
    let case = workspace_case()?;
    case.write_files([
        (
            "pyproject.toml",
            r#"
                [tool.uv.workspace]
                members = ["packages/*"]

                [tool.uv]
                no-index = true
                find-links = ["wheels"]
            "#,
        ),
        (
            "packages/member/pyproject.toml",
            r#"
                [project]
                name = "member"
                version = "0.1.0"
                requires-python = ">=3.8"
                dependencies = ["direct-dependency"]
            "#,
        ),
        (
            "packages/member/member.py",
            r#"
                import direct_module
                from indirect_module import value
                import indirect_module
            "#,
        ),
        ("packages/sibling/sibling.py", "import direct_module\n"),
    ])?;
    write_dependency_wheel(&case, "indirect-dependency", "indirect_module", &[])?;
    write_dependency_wheel(
        &case,
        "direct-dependency",
        "direct_module",
        &["indirect-dependency"],
    )?;

    Ok(case)
}

/// Imports are checked against each member's direct dependencies, using uv's mapping from import
/// names to distributions. A dependency declared by one member does not apply to its siblings.
#[cfg(feature = "test-uv")]
#[test]
fn indirect_dependencies_use_uv_module_ownership() -> anyhow::Result<()> {
    let case = dependency_workspace_case()?;
    let mut command = uv_sync_command(&case, None)?;
    command.arg("packages");
    let lockfile = std::fs::read(case.root().join("uv.lock"))?;

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    command
        .args(["--error", "missing-direct-dependency"])
        .env("TY_OUTPUT_FORMAT", "full");
    assert_cmd_snapshot!(command, @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[missing-direct-dependency]: Import of `indirect_module` requires a direct dependency on `indirect-dependency`
     --> packages/member/member.py:3:6
      |
    3 | from indirect_module import value
      |      ^^^^^^^^^^^^^^^
    help: Declare `indirect-dependency` in `project.dependencies` or `project.optional-dependencies` in your `pyproject.toml`
    info: See https://docs.astral.sh/uv/concepts/projects/dependencies/

    error[missing-direct-dependency]: Import of `indirect_module` requires a direct dependency on `indirect-dependency`
     --> packages/member/member.py:4:8
      |
    4 | import indirect_module
      |        ^^^^^^^^^^^^^^^
    help: Declare `indirect-dependency` in `project.dependencies` or `project.optional-dependencies` in your `pyproject.toml`
    info: See https://docs.astral.sh/uv/concepts/projects/dependencies/

    error[missing-direct-dependency]: Import of `direct_module` requires a direct dependency on `direct-dependency`
     --> packages/sibling/sibling.py:1:8
      |
    1 | import direct_module
      |        ^^^^^^^^^^^^^
    help: Declare `direct-dependency` in `project.dependencies` or `project.optional-dependencies` in your `pyproject.toml`
    info: See https://docs.astral.sh/uv/concepts/projects/dependencies/

    Found 3 diagnostics

    ----- stderr -----
    ");

    assert_eq!(std::fs::read(case.root().join("uv.lock"))?, lockfile);

    Ok(())
}

/// An explicitly selected environment cannot use uv's module ownership, even when it is nested
/// inside uv's environment. Dependency checks are skipped, but ordinary type checking continues.
#[cfg(feature = "test-uv")]
#[test]
fn overridden_python_environment_disables_dependency_checks() -> anyhow::Result<()> {
    let case = dependency_workspace_case()?.with_filter(
        r"selected Python environment `<temp_dir>/(?:\.venv/)?other`",
        "selected Python environment `<environment>`",
    );
    case.write_file(
        "packages/member/member.py",
        r#"
            from indirect_module import value
            number: str = value
        "#,
    )?;

    assert_cmd_snapshot!(
        uv_sync_command(&case, None)?
            .args(["packages/member", "--error", "missing-direct-dependency"]),
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    packages/member/member.py:2:6: error[missing-direct-dependency] Import of `indirect_module` requires a direct dependency on `indirect-dependency`
    packages/member/member.py:3:15: error[invalid-assignment] Object of type `int` is not assignable to `str`
    Found 2 diagnostics

    ----- stderr -----
    "
    );

    for other_environment in ["other", ".venv/other"] {
        let output = uv_command(&case)
            .args(["venv", "--no-project", other_environment])
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "failed to create environment: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = uv_command(&case)
            .args([
                "pip",
                "install",
                "--python",
                other_environment,
                "--no-index",
                "--find-links",
                "wheels",
                "indirect-dependency",
            ])
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "failed to install dependency: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut command = uv_sync_command(&case, None)?;
        command.args([
            "packages/member",
            "--error",
            "missing-direct-dependency",
            "--python",
            other_environment,
        ]);
        insta::allow_duplicates! {
            assert_cmd_snapshot!(
                command,
                @"
            success: false
            exit_code: 1
            ----- stdout -----
            packages/member/member.py:3:15: error[invalid-assignment] Object of type `int` is not assignable to `str`
            pyproject.toml: warning[uv-metadata] Failed to load uv dependency metadata: selected Python environment `<environment>` (from `--python` argument) differs from uv's environment `<temp_dir>/.venv`
            Found 2 diagnostics

            ----- stderr -----
            "
            );
        }
    }

    Ok(())
}

/// The workspace root provides first-party imports without expanding analysis to unselected
/// sibling members.
#[cfg(feature = "test-uv")]
#[test]
fn uses_uv_workspace_root_without_checking_siblings() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = uv_sync_command(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert!(case.root().join(".venv").is_dir());

    Ok(())
}

/// An explicit file is treated as a script, so workspace discovery stays disabled even when
/// `TY_UV` is set.
#[cfg(feature = "test-uv")]
#[test]
fn explicit_file_path_disables_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-script'",
    )?;

    let mut command = uv_sync_command(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg("member.py");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:8: error[unresolved-import] Cannot resolve imported module `shared`
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-script"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

/// An explicitly selected member inherits ty rule configuration from the uv workspace root.
#[cfg(feature = "test-uv")]
#[test]
fn explicit_workspace_member_directory_uses_workspace_configuration() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*"]

[tool.ty.rules]
invalid-assignment = "ignore"
"#,
    )?;
    let mut command = uv_sync_command(&case, None)?;
    command.arg("packages/member");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// Workspace configuration still applies when the selected member lives outside the workspace
/// root's directory tree.
#[cfg(feature = "test-uv")]
#[test]
fn external_workspace_member_uses_workspace_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
[tool.uv.workspace]
members = ["../external-package"]

[tool.ty.rules]
invalid-assignment = "ignore"
"#,
        ),
        (
            "../external-package/pyproject.toml",
            r#"
[project]
name = "external-package"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "../external-package/member.py",
            "value: int = 'selected-external-member'",
        ),
    ])?;

    let mut command = uv_sync_command(&case, None)?;
    command
        .args(["--project", "../external-package", "../external-package"])
        .env("UV_PROJECT", case.root());

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// Excludes passed by `uv check` prevent an unselected nested member from being analyzed.
#[cfg(feature = "test-uv")]
#[test]
fn selected_workspace_member_excludes_nested_member() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*", "packages/member/nested"]
"#,
    )?;
    case.write_file(
        "packages/member/nested/pyproject.toml",
        r#"
[project]
name = "nested"
version = "0.1.0"
requires-python = ">=3.8"
"#,
    )?;
    case.write_file(
        "packages/member/nested/nested.py",
        "value: int = 'unselected-nested-member'",
    )?;

    let mut command = uv_sync_command(&case, None)?;
    command.args(["--exclude", "packages/member/nested", "packages/member"]);

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    packages/member/member.py:1:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

/// Metadata discovery preserves uv's active isolated environment instead of using an invalid
/// Python environment configured in the workspace.
#[cfg(feature = "test-uv")]
#[test]
fn forwards_active_environment_to_uv() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*"]

[tool.ty.environment]
python = "missing-configured-environment"
"#,
    )?;
    let environment = case.root().join("isolated");
    let mut command = uv_sync_command(&case, Some(&environment))?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env_remove("UV_PROJECT_ENVIRONMENT");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    assert!(environment.is_dir());
    assert!(!case.root().join(".venv").exists());

    Ok(())
}

/// Merely exposing the uv executable must not change ordinary ty project discovery without
/// `TY_UV`.
#[test]
fn uv_workspace_discovery_is_opt_in() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", "uv")
        .env("TY_OUTPUT_FORMAT", "concise")
        .env_remove("TY_UV");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:8: error[unresolved-import] Cannot resolve imported module `shared`
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

/// Script-only uv integration must not invoke uv to discover the enclosing workspace.
#[test]
fn scripts_only_mode_disables_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env(EnvVars::TY_UV, "scripts")
        .env(EnvVars::UV, "missing-uv-executable");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `shared`
     --> member.py:1:8
      |
    1 | import shared
      |        ^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/packages/member (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    error[invalid-assignment]: Object of type `Literal["selected-member"]` is not assignable to `int`
     --> member.py:2:14
      |
    2 | value: int = 'selected-member'
      |        ---   ^^^^^^^^^^^^^^^^^ Incompatible value of type `Literal["selected-member"]`
      |        |
      |        Declared type

    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

/// Failures to locate uv are visible by default instead of silently disabling integration.
#[test]
fn warns_when_uv_workspace_metadata_cannot_be_loaded() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "value: int = 1")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env("TY_UV", "1")
        .env_remove("UV")
        .env("PATH", "")
        .env("TY_OUTPUT_FORMAT", "concise");

    assert_cmd_snapshot!(command, @"
    success: false
    exit_code: 1
    ----- stdout -----
    pyproject.toml: warning[uv-metadata] Failed to invoke `uv workspace metadata`: failed to resolve uv executable: cannot find binary path
    Found 1 diagnostic

    ----- stderr -----
    ");

    command.env_remove("TY_OUTPUT_FORMAT");
    command.arg("--exit-zero-on-warning");
    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[uv-metadata]: Failed to invoke `uv workspace metadata`: failed to resolve uv executable: cannot find binary path
    --> pyproject.toml:1:1

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// Workspace discovery can find uv on `PATH` when the `UV` executable override is absent.
#[cfg(feature = "test-uv")]
#[test]
fn finds_uv_on_path_without_uv_environment_variable() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = uv_sync_command(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env_remove("UV");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

/// Version-sensitive diagnostics attribute their assumed Python version to workspace metadata,
/// not to a command-line override.
#[cfg(feature = "test-uv")]
#[test]
fn reports_uv_workspace_python_version_source() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "frozendict")?;

    for output_format in ["full", "concise"] {
        let mut command = uv_sync_command(&case, None)?;
        command
            .current_dir(case.root().join("packages/member"))
            .arg(".")
            .arg("--output-format")
            .arg(output_format);

        let output = command.output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(!output.status.success());
        assert!(!stdout.contains("specified on the command line"));
        if output_format == "full" {
            assert!(stdout.contains("provided by uv metadata"));
        }
    }

    Ok(())
}
