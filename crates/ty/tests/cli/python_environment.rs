use insta_cmd::assert_cmd_snapshot;
use ruff_python_ast::PythonVersion;

use crate::CliTest;

/// Specifying an option on the CLI should take precedence over the same setting in the
/// project's configuration. Here, this is tested for the Python version.
#[test]
fn config_override_python_version() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.11"
            "#,
        ),
        (
            "test.py",
            r#"
            import sys

            # Access `sys.last_exc` that was only added in Python 3.12
            print(sys.last_exc)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Type `<module 'sys'>` has no attribute `last_exc`
     --> test.py:5:7
      |
    4 | # Access `sys.last_exc` that was only added in Python 3.12
    5 | print(sys.last_exc)
      |       ^^^^^^^^^^^^
      |
    info: rule `unresolved-attribute` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    assert_cmd_snapshot!(case.command().arg("--python-version").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Same as above, but for the Python platform.
#[test]
fn config_override_python_platform() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-platform = "linux"
            "#,
        ),
        (
            "test.py",
            r#"
            import sys
            from typing_extensions import reveal_type

            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    3 | from typing_extensions import reveal_type
    4 |
    5 | reveal_type(sys.platform)
      |             ^^^^^^^^^^^^ `Literal["linux"]`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-platform").arg("all"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    3 | from typing_extensions import reveal_type
    4 |
    5 | reveal_type(sys.platform)
      |             ^^^^^^^^^^^^ `LiteralString`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn config_file_annotation_showing_where_python_version_set_typing_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.8"
            "#,
        ),
        (
            "test.py",
            r#"
            aiter
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:2:1
      |
    2 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types
     --> pyproject.toml:3:18
      |
    2 | [tool.ty.environment]
    3 | python-version = "3.8"
      |                  ^^^^^ Python 3.8 assumed due to this configuration setting
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version=3.9"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:2:1
      |
    2 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.9 was assumed when resolving types because it was specified on the command line
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// This tests that, even if no Python *version* has been specified on the CLI or in a config file,
/// ty is still able to infer the Python version from a `--python` argument on the CLI,
/// *even if* the `--python` argument points to a system installation.
///
/// We currently cannot infer the Python version from a system installation on Windows:
/// on Windows, we can only infer the Python version from a virtual environment.
/// This is because we use the layout of the Python installation to infer the Python version:
/// on Unix, the `site-packages` directory of an installation will be located at
/// `<sys.prefix>/lib/pythonX.Y/site-packages`. On Windows, however, the `site-packages`
/// directory will be located at `<sys.prefix>/Lib/site-packages`, which doesn't give us the
/// same information.
#[cfg(not(windows))]
#[test]
fn python_version_inferred_from_system_installation() -> anyhow::Result<()> {
    let cpython_case = CliTest::with_files([
        ("pythons/Python3.8/bin/python", ""),
        ("pythons/Python3.8/lib/python3.8/site-packages/foo.py", ""),
        ("test.py", "aiter"),
    ])?;

    assert_cmd_snapshot!(cpython_case.command().arg("--python").arg("pythons/Python3.8/bin/python"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:1:1
      |
    1 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types because of the layout of your Python installation
    info: The primary `site-packages` directory of your installation was found at `lib/python3.8/site-packages/`
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    let pypy_case = CliTest::with_files([
        ("pythons/pypy3.8/bin/python", ""),
        ("pythons/pypy3.8/lib/pypy3.8/site-packages/foo.py", ""),
        ("test.py", "aiter"),
    ])?;

    assert_cmd_snapshot!(pypy_case.command().arg("--python").arg("pythons/pypy3.8/bin/python"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:1:1
      |
    1 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types because of the layout of your Python installation
    info: The primary `site-packages` directory of your installation was found at `lib/pypy3.8/site-packages/`
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    let free_threaded_case = CliTest::with_files([
        ("pythons/Python3.13t/bin/python", ""),
        (
            "pythons/Python3.13t/lib/python3.13t/site-packages/foo.py",
            "",
        ),
        ("test.py", "import string.templatelib"),
    ])?;

    assert_cmd_snapshot!(free_threaded_case.command().arg("--python").arg("pythons/Python3.13t/bin/python"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `string.templatelib`
     --> test.py:1:8
      |
    1 | import string.templatelib
      |        ^^^^^^^^^^^^^^^^^^
      |
    info: The stdlib module `string.templatelib` is only available on Python 3.14+
    info: Python 3.13 was assumed when resolving modules because of the layout of your Python installation
    info: The primary `site-packages` directory of your installation was found at `lib/python3.13t/site-packages/`
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// On Unix systems, it's common for a Python installation at `.venv/bin/python` to only be a symlink
/// to a system Python installation. We must be careful not to resolve the symlink too soon!
/// If we do, we will incorrectly add the system installation's `site-packages` as a search path,
/// when we should be adding the virtual environment's `site-packages` directory as a search path instead.
#[cfg(unix)]
#[test]
fn python_argument_points_to_symlinked_executable() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "system-installation/lib/python3.13/site-packages/foo.py",
            "",
        ),
        ("system-installation/bin/python", ""),
        (
            "strange-venv-location/lib/python3.13/site-packages/bar.py",
            "",
        ),
        (
            "test.py",
            "\
import foo
import bar",
        ),
    ])?;

    case.write_symlink(
        "system-installation/bin/python",
        "strange-venv-location/bin/python",
    )?;

    assert_cmd_snapshot!(case.command().arg("--python").arg("strange-venv-location/bin/python"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `foo`
     --> test.py:1:8
      |
    1 | import foo
      |        ^^^
    2 | import bar
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn pyvenv_cfg_file_annotation_showing_where_python_version_set() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python = "venv"
            "#,
        ),
        (
            "venv/pyvenv.cfg",
            r#"
            version = 3.8
            home = foo/bar/bin
            "#,
        ),
        if cfg!(target_os = "windows") {
            ("foo/bar/bin/python.exe", "")
        } else {
            ("foo/bar/bin/python", "")
        },
        if cfg!(target_os = "windows") {
            ("venv/Lib/site-packages/foo.py", "")
        } else {
            ("venv/lib/python3.8/site-packages/foo.py", "")
        },
        ("test.py", "aiter"),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:1:1
      |
    1 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types because of your virtual environment
     --> venv/pyvenv.cfg:2:11
      |
    2 | version = 3.8
      |           ^^^ Python version inferred from virtual environment metadata file
    3 | home = foo/bar/bin
      |
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn pyvenv_cfg_file_annotation_no_trailing_newline() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python = "venv"
            "#,
        ),
        (
            "venv/pyvenv.cfg",
            r#"home = foo/bar/bin


            version = 3.8"#,
        ),
        if cfg!(target_os = "windows") {
            ("foo/bar/bin/python.exe", "")
        } else {
            ("foo/bar/bin/python", "")
        },
        if cfg!(target_os = "windows") {
            ("venv/Lib/site-packages/foo.py", "")
        } else {
            ("venv/lib/python3.8/site-packages/foo.py", "")
        },
        ("test.py", "aiter"),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:1:1
      |
    1 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types because of your virtual environment
     --> venv/pyvenv.cfg:4:23
      |
    4 |             version = 3.8
      |                       ^^^ Python version inferred from virtual environment metadata file
      |
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn config_file_annotation_showing_where_python_version_set_syntax_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [project]
            requires-python = ">=3.8"
            "#,
        ),
        (
            "test.py",
            r#"
            match object():
                case int():
                    pass
                case _:
                    pass
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> test.py:2:1
      |
    2 | match object():
      | ^^^^^ Cannot use `match` statement on Python 3.8 (syntax was added in Python 3.10)
    3 |     case int():
    4 |         pass
      |
    info: Python 3.8 was assumed when parsing syntax
     --> pyproject.toml:3:19
      |
    2 | [project]
    3 | requires-python = ">=3.8"
      |                   ^^^^^^^ Python 3.8 assumed due to this configuration setting
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version=3.9"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> test.py:2:1
      |
    2 | match object():
      | ^^^^^ Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)
    3 |     case int():
    4 |         pass
      |
    info: Python 3.9 was assumed when parsing syntax because it was specified on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn python_cli_argument_virtual_environment() -> anyhow::Result<()> {
    let path_to_executable = if cfg!(windows) {
        "my-venv/Scripts/python.exe"
    } else {
        "my-venv/bin/python"
    };

    let other_venv_path = "my-venv/foo/some_other_file.txt";

    let case = CliTest::with_files([
        ("test.py", ""),
        (
            if cfg!(windows) {
                "my-venv/Lib/site-packages/foo.py"
            } else {
                "my-venv/lib/python3.13/site-packages/foo.py"
            },
            "",
        ),
        (path_to_executable, ""),
        (other_venv_path, ""),
    ])?;

    // Passing a path to the installation works
    assert_cmd_snapshot!(case.command().arg("--python").arg("my-venv"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // And so does passing a path to the executable inside the installation
    assert_cmd_snapshot!(case.command().arg("--python").arg(path_to_executable), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // But random other paths inside the installation are rejected
    assert_cmd_snapshot!(case.command().arg("--python").arg(other_venv_path), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: Invalid search path settings
      Cause: Failed to discover the site-packages directory: Invalid `--python` argument `<temp_dir>/my-venv/foo/some_other_file.txt`: does not point to a Python executable or a directory on disk
    ");

    // And so are paths that do not exist on disk
    assert_cmd_snapshot!(case.command().arg("--python").arg("not-a-directory-or-executable"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: Invalid search path settings
      Cause: Failed to discover the site-packages directory: Invalid `--python` argument `<temp_dir>/not-a-directory-or-executable`: does not point to a Python executable or a directory on disk
    ");

    Ok(())
}

#[test]
fn python_cli_argument_system_installation() -> anyhow::Result<()> {
    let path_to_executable = if cfg!(windows) {
        "Python3.11/python.exe"
    } else {
        "Python3.11/bin/python"
    };

    let case = CliTest::with_files([
        ("test.py", ""),
        (
            if cfg!(windows) {
                "Python3.11/Lib/site-packages/foo.py"
            } else {
                "Python3.11/lib/python3.11/site-packages/foo.py"
            },
            "",
        ),
        (path_to_executable, ""),
    ])?;

    // Passing a path to the installation works
    assert_cmd_snapshot!(case.command().arg("--python").arg("Python3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // And so does passing a path to the executable inside the installation
    assert_cmd_snapshot!(case.command().arg("--python").arg(path_to_executable), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn config_file_broken_python_setting() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            description = "Some description"
            readme = "README.md"
            requires-python = ">=3.13"
            dependencies = []

            [tool.ty.environment]
            python = "not-a-directory-or-executable"
            "#,
        ),
        ("test.py", ""),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: Invalid search path settings
      Cause: Failed to discover the site-packages directory: Invalid `environment.python` setting

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
       |
     9 |
    10 | [tool.ty.environment]
    11 | python = "not-a-directory-or-executable"
       |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ does not point to a Python executable or a directory on disk
       |
    "#);

    Ok(())
}

#[test]
fn config_file_python_setting_directory_with_no_site_packages() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python = "directory-but-no-site-packages"
            "#,
        ),
        ("directory-but-no-site-packages/lib/foo.py", ""),
        ("test.py", ""),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: Invalid search path settings
      Cause: Failed to discover the site-packages directory: Invalid `environment.python` setting

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
      |
    1 |
    2 | [tool.ty.environment]
    3 | python = "directory-but-no-site-packages"
      |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Could not find a `site-packages` directory for this Python installation/executable
      |
    "#);

    Ok(())
}

// This error message is never emitted on Windows, because Windows installations have simpler layouts
#[cfg(not(windows))]
#[test]
fn unix_system_installation_with_no_lib_directory() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python = "directory-but-no-site-packages"
            "#,
        ),
        ("directory-but-no-site-packages/foo.py", ""),
        ("test.py", ""),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: Invalid search path settings
      Cause: Failed to discover the site-packages directory: Failed to iterate over the contents of the `lib` directory of the Python installation

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
      |
    1 |
    2 | [tool.ty.environment]
    3 | python = "directory-but-no-site-packages"
      |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      |
    "#);

    Ok(())
}

#[test]
fn defaults_to_a_new_python_version() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "ty.toml",
            &*format!(
                r#"
                [environment]
                python-version = "{}"
                python-platform = "linux"
                "#,
                PythonVersion::default()
            ),
        ),
        (
            "main.py",
            r#"
            import os

            os.grantpt(1) # only available on unix, Python 3.13 or newer
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Type `<module 'os'>` has no attribute `grantpt`
     --> main.py:4:1
      |
    2 | import os
    3 |
    4 | os.grantpt(1) # only available on unix, Python 3.13 or newer
      | ^^^^^^^^^^
      |
    info: rule `unresolved-attribute` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Use default (which should be latest supported)
    let case = CliTest::with_files([
        (
            "ty.toml",
            r#"
            [environment]
            python-platform = "linux"
            "#,
        ),
        (
            "main.py",
            r#"
            import os

            os.grantpt(1) # only available on unix, Python 3.13 or newer
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// The `site-packages` directory is used by ty for external import.
/// Ty does the following checks to discover the `site-packages` directory in the order:
/// 1) If `VIRTUAL_ENV` environment variable is set
/// 2) If `CONDA_PREFIX` environment variable is set
/// 3) If a `.venv` directory exists at the project root
///
/// This test is aiming at validating the logic around `CONDA_PREFIX`.
///
/// A conda-like environment file structure is used
/// We test by first not setting the `CONDA_PREFIX` and expect a fail.
/// Then we test by setting `CONDA_PREFIX` to `conda-env` and expect a pass.
///
/// ├── project
/// │   └── test.py
/// └── conda-env
///     └── lib
///         └── python3.13
///             └── site-packages
///                 └── package1
///                     └── __init__.py
///
/// test.py imports package1
/// And the command is run in the `child` directory.
#[test]
fn check_conda_prefix_var_to_resolve_path() -> anyhow::Result<()> {
    let conda_package1_path = if cfg!(windows) {
        "conda-env/Lib/site-packages/package1/__init__.py"
    } else {
        "conda-env/lib/python3.13/site-packages/package1/__init__.py"
    };

    let case = CliTest::with_files([
        (
            "project/test.py",
            r#"
            import package1
            "#,
        ),
        (
            conda_package1_path,
            r#"
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().current_dir(case.root().join("project")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:2:8
      |
    2 | import package1
      |        ^^^^^^^^
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // do command : CONDA_PREFIX=<temp_dir>/conda_env
    assert_cmd_snapshot!(case.command().current_dir(case.root().join("project")).env("CONDA_PREFIX", case.root().join("conda-env")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn src_root_deprecation_warning() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.src]
            root = "./src"
            "#,
        ),
        ("src/test.py", ""),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[deprecated-setting]: The `src.root` setting is deprecated. Use `environment.root` instead.
     --> pyproject.toml:3:8
      |
    2 | [tool.ty.src]
    3 | root = "./src"
      |        ^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    Ok(())
}

#[test]
fn src_root_deprecation_warning_with_environment_root() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.src]
            root = "./src"

            [tool.ty.environment]
            root = ["./app"]
            "#,
        ),
        ("app/test.py", ""),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[deprecated-setting]: The `src.root` setting is deprecated. Use `environment.root` instead.
     --> pyproject.toml:3:8
      |
    2 | [tool.ty.src]
    3 | root = "./src"
      |        ^^^^^^^
    4 |
    5 | [tool.ty.environment]
      |
    info: The `src.root` setting was ignored in favor of the `environment.root` setting

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    Ok(())
}

#[test]
fn environment_root_takes_precedence_over_src_root() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.src]
            root = "./src"

            [tool.ty.environment]
            root = ["./app"]
            "#,
        ),
        ("src/test.py", "import my_module"),
        (
            "app/my_module.py",
            "# This module exists in app/ but not src/",
        ),
    ])?;

    // The test should pass because environment.root points to ./app where my_module.py exists
    // If src.root took precedence, it would fail because my_module.py doesn't exist in ./src
    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[deprecated-setting]: The `src.root` setting is deprecated. Use `environment.root` instead.
     --> pyproject.toml:3:8
      |
    2 | [tool.ty.src]
    3 | root = "./src"
      |        ^^^^^^^
    4 |
    5 | [tool.ty.environment]
      |
    info: The `src.root` setting was ignored in favor of the `environment.root` setting

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    Ok(())
}
