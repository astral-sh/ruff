#![cfg(not(target_family = "wasm"))]

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::path::Path;
use std::process::Command;
use std::str;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
#[cfg(unix)]
use clap::Parser;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
#[cfg(unix)]
use path_absolutize::path_dedot;
#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
use ruff_cli::args::Args;
#[cfg(unix)]
use ruff_cli::run;

const BIN_NAME: &str = "ruff";
const STDIN_BASE_OPTIONS: &[&str] = &["--isolated", "--no-cache", "-", "--format", "text"];

#[test]
fn stdin_success() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .pass_stdin(""), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[test]
fn stdin_error() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .pass_stdin("import os\n"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 potentially fixable with the --fix option.

    ----- stderr -----
    "#);
}

#[test]
fn stdin_filename() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "F401.py"])
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    F401.py:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 potentially fixable with the --fix option.

    ----- stderr -----
    "###);
}

#[test]
/// Raise `TCH` errors in `.py` files ...
fn stdin_source_type_py() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(["--stdin-filename", "TCH.py"])
        .pass_stdin("import os\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    TCH.py:1:8: F401 [*] `os` imported but unused
    Found 1 error.
    [*] 1 potentially fixable with the --fix option.

    ----- stderr -----
    "###);
}

/// ... but not in `.pyi` files.
#[test]
fn stdin_source_type_pyi() {
    let args = ["--stdin-filename", "TCH.pyi", "--select", "TCH"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[cfg(unix)]
#[test]
fn stdin_json() {
    let args = [
        "-",
        "--isolated",
        "--no-cache",
        "--format",
        "json",
        "--stdin-filename",
        "F401.py",
    ];

    let directory = path_dedot::CWD.to_str().unwrap();
    let binding = Path::new(directory).join("F401.py");
    let file_path = binding.display();

    insta::with_settings!({filters => vec![
        (file_path.to_string().as_str(), "/path/to/F401.py"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(args)
            .pass_stdin("import os\n"));
    });
}

#[test]
fn stdin_autofix() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\nimport sys\n\nprint(sys.version)\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    import sys

    print(sys.version)

    ----- stderr -----
    "###);
}

#[test]
fn stdin_autofix_when_not_fixable_should_still_print_contents() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import os\nimport sys\n\nif (1, 2):\n     print(sys.version)\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    import sys

    if (1, 2):
         print(sys.version)

    ----- stderr -----
    "###);
}

#[test]
fn stdin_autofix_when_no_issues_should_still_print_contents() {
    let args = ["--fix"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import sys\n\nprint(sys.version)\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    import sys

    print(sys.version)

    ----- stderr -----
    "###);
}

#[test]
fn show_source() {
    let args = ["--show-source"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("l = 1"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
      |
    1 | l = 1
      | ^ E741
      |

    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn explain_status_codes_f401() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME)).args(["--explain", "F401"]));
}
#[test]
fn explain_status_codes_ruf404() {
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME)).args(["--explain", "RUF404"]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'RUF404' for '[RULE]': unknown rule code

    For more information, try '--help'.
    "###);
}

#[test]
fn show_statistics() {
    let args = ["--select", "F401", "--statistics"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("import sys\nimport os\n\nprint(os.getuid())\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    1	F401	[*] `sys` imported but unused

    ----- stderr -----
    "###);
}

#[test]
fn nursery_prefix() {
    // `--select E` should detect E741, but not E225, which is in the nursery.
    let args = ["--select", "E"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn nursery_all() {
    // `--select ALL` should detect E741, but not E225, which is in the nursery.
    let args = ["--select", "ALL"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:1: D100 Missing docstring in public module
    Found 2 errors.

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    "###);
}

#[test]
fn nursery_direct() {
    // `--select E225` should detect E225.
    let args = ["--select", "E225"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:2: E225 Missing whitespace around operator
    Found 1 error.

    ----- stderr -----
    warning: Selection of nursery rule `E225` without the `--preview` flag is deprecated.
    "###);
}

#[test]
fn nursery_group_selector() {
    // Only nursery rules should be detected e.g. E225 and a warning should be displayed
    let args = ["--select", "NURSERY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 Missing whitespace around operator
    Found 2 errors.

    ----- stderr -----
    warning: The `NURSERY` selector has been deprecated. Use the `--preview` flag instead.
    "###);
}

#[test]
fn nursery_group_selector_preview_enabled() {
    // Only nursery rules should be detected e.g. E225 and a warning should be displayed
    let args = ["--select", "NURSERY", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 Missing whitespace around operator
    Found 2 errors.

    ----- stderr -----
    warning: The `NURSERY` selector has been deprecated.
    "###);
}

#[test]
fn preview_enabled_prefix() {
    // E741 and E225 (preview) should both be detected
    let args = ["--select", "E", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:2: E225 Missing whitespace around operator
    Found 2 errors.

    ----- stderr -----
    "###);
}

#[test]
fn preview_enabled_all() {
    let args = ["--select", "ALL", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `I`
    -:1:1: D100 Missing docstring in public module
    -:1:1: CPY001 Missing copyright notice at top of file
    -:1:2: E225 Missing whitespace around operator
    Found 4 errors.

    ----- stderr -----
    warning: `one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are incompatible. Ignoring `one-blank-line-before-class`.
    warning: `multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are incompatible. Ignoring `multi-line-summary-second-line`.
    "###);
}

#[test]
fn preview_enabled_direct() {
    // E225 should be detected without warning
    let args = ["--select", "E225", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:2: E225 Missing whitespace around operator
    Found 1 error.

    ----- stderr -----
    "###);
}

#[test]
fn preview_disabled_direct() {
    // FURB145 is preview not nursery so selecting should be empty
    let args = ["--select", "FURB145"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("a = l[:]\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Selection `FURB145` has no effect because the `--preview` flag was not included.
    "###);
}

#[test]
fn preview_disabled_prefix_empty() {
    // Warns that the selection is empty since all of the CPY rules are in preview
    let args = ["--select", "CPY"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Selection `CPY` has no effect because the `--preview` flag was not included.
    "###);
}

#[test]
fn preview_group_selector() {
    // `--select PREVIEW` should error (selector was removed)
    let args = ["--select", "PREVIEW", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'PREVIEW' for '--select <RULE_CODE>'

    For more information, try '--help'.
    "###);
}

#[test]
fn preview_enabled_group_ignore() {
    // `--select E --ignore PREVIEW` should detect E741 and E225, which is in preview but "E" is more specific.
    let args = ["--select", "E", "--ignore", "PREVIEW", "--preview"];
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(STDIN_BASE_OPTIONS)
        .args(args)
        .pass_stdin("I=42\n"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'PREVIEW' for '--ignore <RULE_CODE>'

    For more information, try '--help'.
    "###);
}

/// An unreadable pyproject.toml in non-isolated mode causes ruff to hard-error trying to build up
/// configuration globs
#[cfg(unix)]
#[test]
fn unreadable_pyproject_toml() -> Result<()> {
    let tempdir = TempDir::new()?;
    let pyproject_toml = tempdir.path().join("pyproject.toml");
    // Create an empty file with 000 permissions
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o000)
        .open(pyproject_toml)?;

    // Don't `--isolated` since the configuration discovery is where the error happens
    let args = Args::parse_from(["", "check", "--no-cache", tempdir.path().to_str().unwrap()]);
    let err = run(args).err().context("Unexpected success")?;
    assert_eq!(
        err.chain()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Permission denied (os error 13)".to_string()],
    );
    Ok(())
}

/// Check the output with an unreadable directory
#[cfg(unix)]
#[test]
fn unreadable_dir() -> Result<()> {
    // Create a directory with 000 (not iterable/readable) permissions
    let tempdir = TempDir::new()?;
    let unreadable_dir = tempdir.path().join("unreadable_dir");
    fs::create_dir(&unreadable_dir)?;
    fs::set_permissions(&unreadable_dir, Permissions::from_mode(0o000))?;

    // We (currently?) have to use a subcommand to check exit status (currently wrong) and logging
    // output
    // TODO(konstin): This should be a failure, but we currently can't track that
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["--no-cache", "--isolated"])
        .arg(&unreadable_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Encountered error: Permission denied (os error 13)
    "###);
    Ok(())
}

/// Check that reading arguments from an argfile works
#[cfg(unix)]
#[test]
fn check_input_from_argfile() -> Result<()> {
    let tempdir = TempDir::new()?;

    // Create python files
    let file_a_path = tempdir.path().join("a.py");
    let file_b_path = tempdir.path().join("b.py");
    fs::write(&file_a_path, b"import os")?;
    fs::write(&file_b_path, b"print('hello, world!')")?;

    // Create a the input file for argfile to expand
    let input_file_path = tempdir.path().join("file_paths.txt");
    fs::write(
        &input_file_path,
        format!("{}\n{}", file_a_path.display(), file_b_path.display()),
    )?;

    // Generate the args with the argfile notation
    let args = vec![
        "check".to_string(),
        "--no-cache".to_string(),
        format!("@{}", &input_file_path.display()),
    ];

    insta::with_settings!({filters => vec![
        (file_a_path.display().to_string().as_str(), "/path/to/a.py"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(args)
            .pass_stdin(""), @r###"
        success: false
        exit_code: 1
        ----- stdout -----
        /path/to/a.py:1:8: F401 [*] `os` imported but unused
        Found 1 error.
        [*] 1 potentially fixable with the --fix option.

        ----- stderr -----
        "###);
    });

    Ok(())
}
