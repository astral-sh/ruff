use insta_cmd::assert_cmd_snapshot;
use ruff_python_ast::PythonVersion;

use crate::{CliTest, site_packages_filter};

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

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Module `sys` has no member `last_exc`
     --> test.py:5:7
      |
    4 | # Access `sys.last_exc` that was only added in Python 3.12
    5 | print(sys.last_exc)
      |       ^^^^^^^^^^^^
      |
    info: The member may be available on other Python versions or platforms
    info: Python 3.11 was assumed when resolving the `last_exc` attribute
     --> pyproject.toml:3:18
      |
    2 | [tool.ty.environment]
    3 | python-version = "3.11"
      |                  ^^^^^^ Python version configuration
      |
    info: rule `unresolved-attribute` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    assert_cmd_snapshot!(case.command().arg("--python-platform").arg("all"), @r###"
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
    "###);

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
      |                  ^^^^^ Python version configuration
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version=3.9"), @r###"
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
    "###);

    Ok(())
}

/// If `.` and `./src` are both registered as first-party search paths,
/// the `./src` directory should take precedence for module resolution,
/// because it is relative to `.`.
#[test]
fn src_subdirectory_takes_precedence_over_repo_root() -> anyhow::Result<()> {
    let case = CliTest::with_files([(
        "src/package/__init__.py",
        "from . import nonexistent_submodule",
    )])?;

    // If `./src` didn't take priority over `.` here, we would report
    // "Module `src.package` has no member `nonexistent_submodule`"
    // instead of "Module `package` has no member `nonexistent_submodule`".
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package` has no member `nonexistent_submodule`
     --> src/package/__init__.py:1:15
      |
    1 | from . import nonexistent_submodule
      |               ^^^^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

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

    assert_cmd_snapshot!(cpython_case.command().arg("--python").arg("pythons/Python3.8/bin/python"), @r###"
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
    "###);

    let pypy_case = CliTest::with_files([
        ("pythons/pypy3.8/bin/python", ""),
        ("pythons/pypy3.8/lib/pypy3.8/site-packages/foo.py", ""),
        ("test.py", "aiter"),
    ])?;

    assert_cmd_snapshot!(pypy_case.command().arg("--python").arg("pythons/pypy3.8/bin/python"), @r###"
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
    "###);

    let free_threaded_case = CliTest::with_files([
        ("pythons/Python3.13t/bin/python", ""),
        (
            "pythons/Python3.13t/lib/python3.13t/site-packages/foo.py",
            "",
        ),
        ("test.py", "import string.templatelib"),
    ])?;

    assert_cmd_snapshot!(free_threaded_case.command().arg("--python").arg("pythons/Python3.13t/bin/python"), @r###"
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
    "###);

    Ok(())
}

/// This attempts to simulate the tangled web of symlinks that a homebrew install has
/// which can easily confuse us if we're ever told to use it.
///
/// The main thing this is regression-testing is a panic in one *extremely* specific case
/// that you have to try really hard to hit (but vscode, hilariously, did hit).
#[cfg(unix)]
#[test]
fn python_argument_trapped_in_a_symlink_factory() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        // This is the real python binary.
        (
            "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3.13",
            "",
        ),
        // There's a real site-packages here (although it's basically empty).
        (
            "opt/homebrew/Cellar/python@3.13/3.13.5/lib/python3.13/site-packages/foo.py",
            "",
        ),
        // There's also a real site-packages here (although it's basically empty).
        ("opt/homebrew/lib/python3.13/site-packages/bar.py", ""),
        // This has the real stdlib, but the site-packages in this dir is a symlink.
        (
            "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/abc.py",
            "",
        ),
        // It's important that this our faux-homebrew not be in the same dir as our working directory
        // to reproduce the crash, don't ask me why.
        (
            "project/test.py",
            "\
import foo
import bar
import colorama
",
        ),
    ])?;

    // many python symlinks pointing to a single real python (the longest path)
    case.write_symlink(
        "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3.13",
        "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3",
    )?;
    case.write_symlink(
        "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3",
        "opt/homebrew/Cellar/python@3.13/3.13.5/bin/python3",
    )?;
    case.write_symlink(
        "opt/homebrew/Cellar/python@3.13/3.13.5/bin/python3",
        "opt/homebrew/bin/python3",
    )?;
    // the "real" python's site-packages is a symlink to a different dir
    case.write_symlink(
        "opt/homebrew/Cellar/python@3.13/3.13.5/lib/python3.13/site-packages",
        "opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/site-packages",
    )?;

    // Try all 4 pythons with absolute paths to our fauxbrew install
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .arg("--python").arg(case.root().join("opt/homebrew/bin/python3")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `foo`
     --> test.py:1:8
      |
    1 | import foo
      |        ^^^
    2 | import bar
    3 | import colorama
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `colorama`
     --> test.py:3:8
      |
    1 | import foo
    2 | import bar
    3 | import colorama
      |        ^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .arg("--python").arg(case.root().join("opt/homebrew/Cellar/python@3.13/3.13.5/bin/python3")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> test.py:2:8
      |
    1 | import foo
    2 | import bar
      |        ^^^
    3 | import colorama
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `colorama`
     --> test.py:3:8
      |
    1 | import foo
    2 | import bar
    3 | import colorama
      |        ^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .arg("--python").arg(case.root().join("opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> test.py:2:8
      |
    1 | import foo
    2 | import bar
      |        ^^^
    3 | import colorama
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `colorama`
     --> test.py:3:8
      |
    1 | import foo
    2 | import bar
    3 | import colorama
      |        ^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .arg("--python").arg(case.root().join("opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/bin/python3.13")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> test.py:2:8
      |
    1 | import foo
    2 | import bar
      |        ^^^
    3 | import colorama
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `colorama`
     --> test.py:3:8
      |
    1 | import foo
    2 | import bar
    3 | import colorama
      |        ^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/opt/homebrew/Cellar/python@3.13/3.13.5/Frameworks/Python.framework/Versions/3.13/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
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

    assert_cmd_snapshot!(case.command().arg("--python").arg("strange-venv-location/bin/python"), @r###"
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
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/strange-venv-location/lib/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// On Unix systems, a virtual environment can come with multiple `site-packages` directories:
/// one at `<sys.prefix>/lib/pythonX.Y/site-packages` and one at
/// `<sys.prefix>/lib64/pythonX.Y/site-packages`. According to [the stdlib docs], the `lib64`
/// is not *meant* to have any Python files in it (only C extensions and similar). Empirically,
/// however, it sometimes does indeed have Python files in it: popular tools such as poetry
/// appear to sometimes install Python packages into the `lib64` site-packages directory even
/// though they probably shouldn't. We therefore check for both a `lib64` and a `lib` directory,
/// and add them both as search paths if they both exist.
///
/// See:
/// - <https://github.com/astral-sh/ty/issues/1043>
/// - <https://github.com/astral-sh/ty/issues/257>.
///
/// [the stdlib docs]: https://docs.python.org/3/library/sys.html#sys.platlibdir
#[cfg(unix)]
#[test]
fn lib64_site_packages_directory_on_unix() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (".venv/lib/python3.13/site-packages/foo.py", ""),
        (".venv/lib64/python3.13/site-packages/bar.py", ""),
        ("test.py", "import foo, bar, baz"),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--python").arg(".venv"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `baz`
     --> test.py:1:18
      |
    1 | import foo, bar, baz
      |                  ^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/.venv/lib/python3.13/site-packages (site-packages)
    info:   4. <temp_dir>/.venv/lib64/python3.13/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn many_search_paths() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("extra1/foo1.py", ""),
        ("extra2/foo2.py", ""),
        ("extra3/foo3.py", ""),
        ("extra4/foo4.py", ""),
        ("extra5/foo5.py", ""),
        ("extra6/foo6.py", ""),
        ("test.py", "import foo1, baz"),
    ])?;

    assert_cmd_snapshot!(
        case.command()
            .arg("--python-platform").arg("linux")
            .arg("--extra-search-path").arg("extra1")
            .arg("--extra-search-path").arg("extra2")
            .arg("--extra-search-path").arg("extra3")
            .arg("--extra-search-path").arg("extra4")
            .arg("--extra-search-path").arg("extra5")
            .arg("--extra-search-path").arg("extra6"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `baz`
     --> test.py:1:14
      |
    1 | import foo1, baz
      |              ^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/extra1 (extra search path specified on the CLI or in your config file)
    info:   2. <temp_dir>/extra2 (extra search path specified on the CLI or in your config file)
    info:   3. <temp_dir>/extra3 (extra search path specified on the CLI or in your config file)
    info:   4. <temp_dir>/extra4 (extra search path specified on the CLI or in your config file)
    info:   5. <temp_dir>/extra5 (extra search path specified on the CLI or in your config file)
    info:   ... and 3 more paths. Run with `-v` to see all paths.
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    ");

    // Shows all with `-v`
    assert_cmd_snapshot!(
        case.command()
            .arg("--python-platform").arg("linux")
            .arg("--extra-search-path").arg("extra1")
            .arg("--extra-search-path").arg("extra2")
            .arg("--extra-search-path").arg("extra3")
            .arg("--extra-search-path").arg("extra4")
            .arg("--extra-search-path").arg("extra5")
            .arg("--extra-search-path").arg("extra6")
            .arg("-v"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `baz`
     --> test.py:1:14
      |
    1 | import foo1, baz
      |              ^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/extra1 (extra search path specified on the CLI or in your config file)
    info:   2. <temp_dir>/extra2 (extra search path specified on the CLI or in your config file)
    info:   3. <temp_dir>/extra3 (extra search path specified on the CLI or in your config file)
    info:   4. <temp_dir>/extra4 (extra search path specified on the CLI or in your config file)
    info:   5. <temp_dir>/extra5 (extra search path specified on the CLI or in your config file)
    info:   6. <temp_dir>/extra6 (extra search path specified on the CLI or in your config file)
    info:   7. <temp_dir>/ (first-party code)
    info:   8. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    INFO Python version: Python 3.14, platform: linux
    INFO Indexed 7 file(s) in 0.000s
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
      |           ^^^ Virtual environment metadata
    3 | home = foo/bar/bin
      |
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
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
      |                       ^^^ Virtual environment metadata
      |
    info: No Python version was specified on the command line or in a configuration file
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
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
    error[invalid-syntax]: Cannot use `match` statement on Python 3.8 (syntax was added in Python 3.10)
     --> test.py:2:1
      |
    2 | match object():
      | ^^^^^
    3 |     case int():
    4 |         pass
      |
    info: Python 3.8 was assumed when parsing syntax
     --> pyproject.toml:3:19
      |
    2 | [project]
    3 | requires-python = ">=3.8"
      |                   ^^^^^^^ Python version configuration
      |

    Found 1 diagnostic

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version=3.9"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]: Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)
     --> test.py:2:1
      |
    2 | match object():
      | ^^^^^
    3 |     case int():
    4 |         pass
      |
    info: Python 3.9 was assumed when parsing syntax because it was specified on the command line

    Found 1 diagnostic

    ----- stderr -----
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
    assert_cmd_snapshot!(case.command().arg("--python").arg("my-venv"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    // And so does passing a path to the executable inside the installation
    assert_cmd_snapshot!(case.command().arg("--python").arg(path_to_executable), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    // But random other paths inside the installation are rejected
    assert_cmd_snapshot!(case.command().arg("--python").arg(other_venv_path), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Invalid `--python` argument `<temp_dir>/my-venv/foo/some_other_file.txt`: does not point to a Python executable or a directory on disk
    "###);

    // And so are paths that do not exist on disk
    assert_cmd_snapshot!(case.command().arg("--python").arg("not-a-directory-or-executable"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Invalid `--python` argument `<temp_dir>/not-a-directory-or-executable`: does not point to a Python executable or a directory on disk
      Cause: No such file or directory (os error 2)
    "###);

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
    assert_cmd_snapshot!(case.command().arg("--python").arg("Python3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    // And so does passing a path to the executable inside the installation
    assert_cmd_snapshot!(case.command().arg("--python").arg(path_to_executable), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Invalid `environment.python` setting

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
       |
     9 |
    10 | [tool.ty.environment]
    11 | python = "not-a-directory-or-executable"
       |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ does not point to a Python executable or a directory on disk
       |

      Cause: No such file or directory (os error 2)
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Failed to discover the site-packages directory
      Cause: Invalid `environment.python` setting

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
      |
    1 |
    2 | [tool.ty.environment]
    3 | python = "directory-but-no-site-packages"
      |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Could not find a `site-packages` directory for this Python installation/executable
      |
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Failed to discover the site-packages directory
      Cause: Failed to iterate over the contents of the `lib`/`lib64` directories of the Python installation

    --> Invalid setting in configuration file `<temp_dir>/pyproject.toml`
      |
    1 |
    2 | [tool.ty.environment]
    3 | python = "directory-but-no-site-packages"
      |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      |
    "###);

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

            from typing import LiteralString  # added in Python 3.11
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Module `os` has no member `grantpt`
     --> main.py:4:1
      |
    2 | import os
    3 |
    4 | os.grantpt(1) # only available on unix, Python 3.13 or newer
      | ^^^^^^^^^^
    5 |
    6 | from typing import LiteralString  # added in Python 3.11
      |
    info: The member may be available on other Python versions or platforms
    info: Python 3.10 was assumed when resolving the `grantpt` attribute
     --> ty.toml:3:18
      |
    2 | [environment]
    3 | python-version = "3.10"
      |                  ^^^^^^ Python version configuration
    4 | python-platform = "linux"
      |
    info: rule `unresolved-attribute` is enabled by default

    error[unresolved-import]: Module `typing` has no member `LiteralString`
     --> main.py:6:20
      |
    4 | os.grantpt(1) # only available on unix, Python 3.13 or newer
    5 |
    6 | from typing import LiteralString  # added in Python 3.11
      |                    ^^^^^^^^^^^^^
      |
    info: The member may be available on other Python versions or platforms
    info: Python 3.10 was assumed when resolving imports
     --> ty.toml:3:18
      |
    2 | [environment]
    3 | python-version = "3.10"
      |                  ^^^^^^ Python version configuration
    4 | python-platform = "linux"
      |
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    "#);

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

            from typing import LiteralString  # added in Python 3.11
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

/// The `site-packages` directory is used by ty for external import.
/// Ty does the following checks to discover the `site-packages` directory in the order:
/// 1) If `VIRTUAL_ENV` environment variable is set
/// 2) If `CONDA_PREFIX` environment variable is set (and .filename == `CONDA_DEFAULT_ENV`)
/// 3) If a `.venv` directory exists at the project root
/// 4) If `CONDA_PREFIX` environment variable is set (and .filename != `CONDA_DEFAULT_ENV`)
///    or if `_CONDA_ROOT` is set (and `_CONDA_ROOT` == `CONDA_PREFIX`)
///
/// This test (and the next one) is aiming at validating the logic around these cases.
///
/// To do this we create a program that has these 4 imports:
///
/// ```python
/// from package1 import ActiveVenv
/// from package1 import ChildConda
/// from package1 import WorkingVenv
/// from package1 import BaseConda
/// ```
///
/// We then create 4 different copies of package1. Each copy defines all of these
/// classes... except the one that describes it. Therefore we know we got e.g.
/// the working venv if we get a diagnostic like this:
///
/// ```text
/// Unresolved import
/// 4 | from package1 import WorkingVenv
///   |                      ^^^^^^^^^^^
/// ```
///
/// This test uses a directory structure as follows:
///
/// ├── project
/// │   ├── test.py
/// │   └── .venv
/// │       ├── pyvenv.cfg
/// │       └── lib
/// │           └── python3.13
/// │               └── site-packages
/// │                   └── package1
/// │                       └── __init__.py
/// ├── myvenv
/// │   ├── pyvenv.cfg
/// │   └── lib
/// │       └── python3.13
/// │           └── site-packages
/// │               └── package1
/// │                   └── __init__.py
/// └── conda
///     ├── lib
///     │   └── python3.13
///     │       └── site-packages
///     │           └── package1
///     │               └── __init__.py
///     └── envs
///         └── conda-env
///             └── lib
///                 └── python3.13
///                     └── site-packages
///                         └── package1
///                             └── __init__.py
///
/// test.py imports package1
/// And the command is run in the `child` directory.
#[test]
fn check_venv_resolution_with_working_venv() -> anyhow::Result<()> {
    let child_conda_package1_path = if cfg!(windows) {
        "conda/envs/conda-env/Lib/site-packages/package1/__init__.py"
    } else {
        "conda/envs/conda-env/lib/python3.13/site-packages/package1/__init__.py"
    };

    let base_conda_package1_path = if cfg!(windows) {
        "conda/Lib/site-packages/package1/__init__.py"
    } else {
        "conda//lib/python3.13/site-packages/package1/__init__.py"
    };

    let working_venv_package1_path = if cfg!(windows) {
        "project/.venv/Lib/site-packages/package1/__init__.py"
    } else {
        "project/.venv/lib/python3.13/site-packages/package1/__init__.py"
    };

    let active_venv_package1_path = if cfg!(windows) {
        "myvenv/Lib/site-packages/package1/__init__.py"
    } else {
        "myvenv/lib/python3.13/site-packages/package1/__init__.py"
    };

    let case = CliTest::with_files([
        (
            "project/test.py",
            r#"
            from package1 import ActiveVenv
            from package1 import ChildConda
            from package1 import WorkingVenv
            from package1 import BaseConda
            "#,
        ),
        (
            "project/.venv/pyvenv.cfg",
            r#"
home = ./

            "#,
        ),
        (
            "myvenv/pyvenv.cfg",
            r#"
home = ./

            "#,
        ),
        (
            active_venv_package1_path,
            r#"
            class ChildConda: ...
            class WorkingVenv: ...
            class BaseConda: ...
            "#,
        ),
        (
            child_conda_package1_path,
            r#"
            class ActiveVenv: ...
            class WorkingVenv: ...
            class BaseConda: ...
            "#,
        ),
        (
            working_venv_package1_path,
            r#"
            class ActiveVenv: ...
            class ChildConda: ...
            class BaseConda: ...
            "#,
        ),
        (
            base_conda_package1_path,
            r#"
            class ActiveVenv: ...
            class ChildConda: ...
            class WorkingVenv: ...
            "#,
        ),
    ])?;

    // Run with nothing set, should find the working venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `WorkingVenv`
     --> test.py:4:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |                      ^^^^^^^^^^^
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // Run with VIRTUAL_ENV set, should find the active venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("VIRTUAL_ENV", case.root().join("myvenv")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ActiveVenv`
     --> test.py:2:22
      |
    2 | from package1 import ActiveVenv
      |                      ^^^^^^^^^^
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX set, should find the child conda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda/envs/conda-env")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ChildConda`
     --> test.py:3:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |                      ^^^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV set (unequal), should find working venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("CONDA_DEFAULT_ENV", "base"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `WorkingVenv`
     --> test.py:4:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |                      ^^^^^^^^^^^
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV (unequal) and VIRTUAL_ENV set,
    // should find child active venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("CONDA_DEFAULT_ENV", "base")
        .env("VIRTUAL_ENV", case.root().join("myvenv")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ActiveVenv`
     --> test.py:2:22
      |
    2 | from package1 import ActiveVenv
      |                      ^^^^^^^^^^
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV (equal!) set, should find ChildConda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda/envs/conda-env"))
        .env("CONDA_DEFAULT_ENV", "conda-env"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ChildConda`
     --> test.py:3:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |                      ^^^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with _CONDA_ROOT and CONDA_PREFIX (unequal!) set, should find ChildConda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda/envs/conda-env"))
        .env("_CONDA_ROOT", "conda"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ChildConda`
     --> test.py:3:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |                      ^^^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with _CONDA_ROOT and CONDA_PREFIX (equal!) set, should find BaseConda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("_CONDA_ROOT", "conda"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `BaseConda`
     --> test.py:5:22
      |
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |                      ^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// The exact same test as above, but without a working venv
///
/// In this case the Base Conda should be a possible outcome.
#[test]
fn check_venv_resolution_without_working_venv() -> anyhow::Result<()> {
    let child_conda_package1_path = if cfg!(windows) {
        "conda/envs/conda-env/Lib/site-packages/package1/__init__.py"
    } else {
        "conda/envs/conda-env/lib/python3.13/site-packages/package1/__init__.py"
    };

    let base_conda_package1_path = if cfg!(windows) {
        "conda/Lib/site-packages/package1/__init__.py"
    } else {
        "conda/lib/python3.13/site-packages/package1/__init__.py"
    };

    let active_venv_package1_path = if cfg!(windows) {
        "myvenv/Lib/site-packages/package1/__init__.py"
    } else {
        "myvenv/lib/python3.13/site-packages/package1/__init__.py"
    };

    let case = CliTest::with_files([
        (
            "project/test.py",
            r#"
            from package1 import ActiveVenv
            from package1 import ChildConda
            from package1 import WorkingVenv
            from package1 import BaseConda
            "#,
        ),
        (
            "myvenv/pyvenv.cfg",
            r#"
home = ./

            "#,
        ),
        (
            active_venv_package1_path,
            r#"
            class ChildConda: ...
            class WorkingVenv: ...
            class BaseConda: ...
            "#,
        ),
        (
            child_conda_package1_path,
            r#"
            class ActiveVenv: ...
            class WorkingVenv: ...
            class BaseConda: ...
            "#,
        ),
        (
            base_conda_package1_path,
            r#"
            class ActiveVenv: ...
            class ChildConda: ...
            class WorkingVenv: ...
            "#,
        ),
    ])?;

    // Run with nothing set, should fail to find anything
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:2:6
      |
    2 | from package1 import ActiveVenv
      |      ^^^^^^^^
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:3:6
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |      ^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:4:6
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |      ^^^^^^^^
    5 | from package1 import BaseConda
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:5:6
      |
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |      ^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/project (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 4 diagnostics

    ----- stderr -----
    "###);

    // Run with VIRTUAL_ENV set, should find the active venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("VIRTUAL_ENV", case.root().join("myvenv")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ActiveVenv`
     --> test.py:2:22
      |
    2 | from package1 import ActiveVenv
      |                      ^^^^^^^^^^
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX set, should find the child conda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda/envs/conda-env")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ChildConda`
     --> test.py:3:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |                      ^^^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV set (unequal), should find base conda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("CONDA_DEFAULT_ENV", "base"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `BaseConda`
     --> test.py:5:22
      |
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |                      ^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV (unequal) and VIRTUAL_ENV set,
    // should find child active venv
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("CONDA_DEFAULT_ENV", "base")
        .env("VIRTUAL_ENV", case.root().join("myvenv")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ActiveVenv`
     --> test.py:2:22
      |
    2 | from package1 import ActiveVenv
      |                      ^^^^^^^^^^
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with CONDA_PREFIX and CONDA_DEFAULT_ENV (unequal!) set, should find base conda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("CONDA_DEFAULT_ENV", "base"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `BaseConda`
     --> test.py:5:22
      |
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |                      ^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with _CONDA_ROOT and CONDA_PREFIX (unequal!) set, should find ChildConda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda/envs/conda-env"))
        .env("_CONDA_ROOT", "conda"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `ChildConda`
     --> test.py:3:22
      |
    2 | from package1 import ActiveVenv
    3 | from package1 import ChildConda
      |                      ^^^^^^^^^^
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    // run with _CONDA_ROOT and CONDA_PREFIX (equal!) set, should find BaseConda
    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("CONDA_PREFIX", case.root().join("conda"))
        .env("_CONDA_ROOT", "conda"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `package1` has no member `BaseConda`
     --> test.py:5:22
      |
    3 | from package1 import ChildConda
    4 | from package1 import WorkingVenv
    5 | from package1 import BaseConda
      |                      ^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// ty should include site packages from its own environment when no other environment is found.
#[test]
fn ty_environment_is_only_environment() -> anyhow::Result<()> {
    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let ty_package_path = format!("{ty_venv_site_packages}/ty_package/__init__.py");

    let case = CliTest::with_files([
        (ty_package_path.as_str(), "class TyEnvClass: ..."),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            from ty_package import TyEnvClass
            ",
        ),
    ])?;

    let case = case.with_ty_at(ty_executable_path)?;
    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

/// ty should include site packages from both its own environment and a local `.venv`. The packages
/// from ty's environment should take precedence.
#[test]
fn ty_environment_and_discovered_venv() -> anyhow::Result<()> {
    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let local_venv_site_packages = if cfg!(windows) {
        ".venv/Lib/site-packages"
    } else {
        ".venv/lib/python3.13/site-packages"
    };

    let ty_unique_package = format!("{ty_venv_site_packages}/ty_package/__init__.py");
    let local_unique_package = format!("{local_venv_site_packages}/local_package/__init__.py");
    let ty_conflicting_package = format!("{ty_venv_site_packages}/shared_package/__init__.py");
    let local_conflicting_package =
        format!("{local_venv_site_packages}/shared_package/__init__.py");

    let case = CliTest::with_files([
        (ty_unique_package.as_str(), "class TyEnvClass: ..."),
        (local_unique_package.as_str(), "class LocalClass: ..."),
        (ty_conflicting_package.as_str(), "class FromTyEnv: ..."),
        (
            local_conflicting_package.as_str(),
            "class FromLocalVenv: ...",
        ),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            ".venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            # Should resolve from ty's environment
            from ty_package import TyEnvClass
            # Should resolve from local .venv
            from local_package import LocalClass
            # Should resolve from ty's environment (takes precedence)
            from shared_package import FromTyEnv
            # Should NOT resolve (shadowed by ty's environment version)
            from shared_package import FromLocalVenv
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `shared_package` has no member `FromLocalVenv`
     --> test.py:9:28
      |
    7 | from shared_package import FromTyEnv
    8 | # Should NOT resolve (shadowed by ty's environment version)
    9 | from shared_package import FromLocalVenv
      |                            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// When `VIRTUAL_ENV` is set, ty should *not* discover its own environment's site-packages.
#[test]
fn ty_environment_and_active_environment() -> anyhow::Result<()> {
    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let active_venv_site_packages = if cfg!(windows) {
        "active-venv/Lib/site-packages"
    } else {
        "active-venv/lib/python3.13/site-packages"
    };

    let ty_package_path = format!("{ty_venv_site_packages}/ty_package/__init__.py");
    let active_package_path = format!("{active_venv_site_packages}/active_package/__init__.py");

    let case = CliTest::with_files([
        (ty_package_path.as_str(), "class TyEnvClass: ..."),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (active_package_path.as_str(), "class ActiveClass: ..."),
        (
            "active-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            from ty_package import TyEnvClass
            from active_package import ActiveClass
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?
    .with_filter(&site_packages_filter("3.13"), "<site-packages>");

    assert_cmd_snapshot!(
        case.command()
            .env("VIRTUAL_ENV", case.root().join("active-venv")),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `ty_package`
     --> test.py:2:6
      |
    2 | from ty_package import TyEnvClass
      |      ^^^^^^^^^^
    3 | from active_package import ActiveClass
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/active-venv/<site-packages> (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "
    );

    Ok(())
}

/// When ty is installed in a system environment rather than a virtual environment, it should
/// not include the environment's site-packages in its search path.
#[test]
fn ty_environment_is_system_not_virtual() -> anyhow::Result<()> {
    let ty_system_site_packages = if cfg!(windows) {
        "system-python/Lib/site-packages"
    } else {
        "system-python/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "system-python/Scripts/ty.exe"
    } else {
        "system-python/bin/ty"
    };

    let ty_package_path = format!("{ty_system_site_packages}/system_package/__init__.py");

    let case = CliTest::with_files([
        // Package in system Python installation (should NOT be discovered)
        (ty_package_path.as_str(), "class SystemClass: ..."),
        // Note: NO pyvenv.cfg - this is a system installation, not a venv
        (
            "test.py",
            r"
            from system_package import SystemClass
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `system_package`
     --> test.py:2:6
      |
    2 | from system_package import SystemClass
      |      ^^^^^^^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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
    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    Ok(())
}

#[test]
fn default_root_src_layout() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("bar.py", "bar = 20"),
        (
            "src/main.py",
            r#"
            from foo import foo
            from bar import bar

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn default_root_project_name_folder() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [project]
            name = "psycopg"
            "#,
        ),
        ("psycopg/psycopg/foo.py", "foo = 10"),
        ("bar.py", "bar = 20"),
        (
            "psycopg/psycopg/main.py",
            r#"
            from psycopg.foo import foo
            from bar import bar

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn default_root_flat_layout() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("app/foo.py", "foo = 10"),
        ("bar.py", "bar = 20"),
        (
            "app/main.py",
            r#"
            from app.foo import foo
            from bar import bar

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn default_root_tests_folder() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("tests/bar.py", "baz = 20"),
        (
            "tests/test_bar.py",
            r#"
            from foo import foo
            from bar import baz

            print(f"{foo} {baz}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

/// If `tests/__init__.py` is present, it is considered a package and `tests` is not added to `sys.path`.
#[test]
fn default_root_tests_package() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("tests/__init__.py", ""),
        ("tests/bar.py", "bar = 20"),
        (
            "tests/test_bar.py",
            r#"
            from foo import foo
            from bar import bar  # expected unresolved import

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> tests/test_bar.py:3:6
      |
    2 | from foo import foo
    3 | from bar import bar  # expected unresolved import
      |      ^^^
    4 |
    5 | print(f"{foo} {bar}")
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn default_root_python_folder() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("python/bar.py", "bar = 20"),
        (
            "python/test_bar.py",
            r#"
            from foo import foo
            from bar import bar

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

/// If `python/__init__.py` is present, it is considered a package and `python` is not added to search paths.
#[test]
fn default_root_python_package() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("python/__init__.py", ""),
        ("python/bar.py", "bar = 20"),
        (
            "python/test_bar.py",
            r#"
            from foo import foo
            from bar import bar  # expected unresolved import

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> python/test_bar.py:3:6
      |
    2 | from foo import foo
    3 | from bar import bar  # expected unresolved import
      |      ^^^
    4 |
    5 | print(f"{foo} {bar}")
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// Similarly, if `python/__init__.pyi` is present, it is considered a package and `python` is not added to search paths.
#[test]
fn default_root_python_package_pyi() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("src/foo.py", "foo = 10"),
        ("python/__init__.pyi", ""),
        ("python/bar.py", "bar = 20"),
        (
            "python/test_bar.py",
            r#"
            from foo import foo
            from bar import bar  # expected unresolved import

            print(f"{foo} {bar}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `bar`
     --> python/test_bar.py:3:6
      |
    2 | from foo import foo
    3 | from bar import bar  # expected unresolved import
      |      ^^^
    4 |
    5 | print(f"{foo} {bar}")
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn pythonpath_is_respected() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("baz-dir/baz.py", "it = 42"),
        (
            "src/foo.py",
            r#"
            import baz
            print(f"{baz.it}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `baz`
     --> src/foo.py:2:8
      |
    2 | import baz
      |        ^^^
    3 | print(f"{baz.it}")
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    assert_cmd_snapshot!(case.command()
        .env("PYTHONPATH", case.root().join("baz-dir")),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn pythonpath_multiple_dirs_is_respected() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("baz-dir/baz.py", "it = 42"),
        ("foo-dir/foo.py", "it = 42"),
        (
            "src/main.py",
            r#"
            import baz
            import foo

            print(f"{baz.it}")
            print(f"{foo.it}")
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `baz`
     --> src/main.py:2:8
      |
    2 | import baz
      |        ^^^
    3 | import foo
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `foo`
     --> src/main.py:3:8
      |
    2 | import baz
    3 | import foo
      |        ^^^
    4 |
    5 | print(f"{baz.it}")
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/src (first-party code)
    info:   2. <temp_dir>/ (first-party code)
    info:   3. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    "###);

    let pythonpath =
        std::env::join_paths([case.root().join("baz-dir"), case.root().join("foo-dir")])?;
    assert_cmd_snapshot!(case.command()
        .env("PYTHONPATH", pythonpath),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    "###);

    Ok(())
}

/// Test behavior when `VIRTUAL_ENV` is set but points to a non-existent path.
#[test]
fn missing_virtual_env() -> anyhow::Result<()> {
    let working_venv_package1_path = if cfg!(windows) {
        "project/.venv/Lib/site-packages/package1/__init__.py"
    } else {
        "project/.venv/lib/python3.13/site-packages/package1/__init__.py"
    };

    let case = CliTest::with_files([
        (
            "project/test.py",
            r#"
            from package1 import WorkingVenv
            "#,
        ),
        (
            "project/.venv/pyvenv.cfg",
            r#"
home = ./

            "#,
        ),
        (
            working_venv_package1_path,
            r#"
            class WorkingVenv: ...
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command()
        .current_dir(case.root().join("project"))
        .env("VIRTUAL_ENV", case.root().join("nonexistent-venv")), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Failed to discover local Python environment
      Cause: Invalid `VIRTUAL_ENV` environment variable `<temp_dir>/nonexistent-venv`: does not point to a directory on disk
      Cause: No such file or directory (os error 2)
    ");

    Ok(())
}
