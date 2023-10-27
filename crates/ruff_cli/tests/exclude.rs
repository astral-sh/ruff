#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

use anyhow::Result;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

fn ruff_cli() -> Command {
    Command::new(get_cargo_bin("ruff"))
}

fn ruff_check(current_dir: &Path) -> Command {
    let mut command = ruff_cli();

    command
        .current_dir(current_dir)
        .args(["check", ".", "--no-cache"]);

    command
}

/// Tests excluding files and directories by their basename.
///
/// ```toml
/// exclude = ["logs"]
/// ```
///
/// Excludes files and directories named `logs`.
#[test]
fn exclude_basename() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
select = ["F401"]
exclude = ["logs.py"]
"#,
    )?;

    fs::write(tempdir.path().join("main.py"), "import im_included")?;

    let logs_dir = tempdir.path().join("logs.py");
    fs::create_dir(&logs_dir)?;

    // Excluded because the file is inside of the `logs.py` directory
    fs::write(logs_dir.join("excluded.py"), "import im_excluded")?;

    let src = tempdir.path().join("src");
    let sub_logs_dir = src.join("nested").join("logs.py").join("output");
    fs::create_dir_all(&sub_logs_dir)?;

    // "excluded because the file name is logs.py"
    fs::write(src.join("logs.py"), "import im_excluded")?;

    // Excluded because the file is in a nested `logs.py` directory
    fs::write(sub_logs_dir.join("excluded2.py"), "import im_excluded")?;

    assert_cmd_snapshot!(
        ruff_check(tempdir.path())
        .args(["--config"])
        .arg(ruff_toml.file_name().unwrap()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:8: F401 [*] `im_included` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    Ok(())
}

/// Tests excluding directories by their name
///
/// ```toml
/// exclude = ["logs/"]
/// ```
///
/// Excludes the directory `logs`.
#[test]
fn exclude_dirname() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
select = ["F401"]
exclude = ["logs.py/"]
"#,
    )?;

    fs::write(tempdir.path().join("main.py"), "import im_included")?;

    let logs_dir = tempdir.path().join("logs.py");
    fs::create_dir(&logs_dir)?;

    let src = tempdir.path().join("src");
    let sub_logs_dir = src.join("nested").join("logs.py").join("output");
    fs::create_dir_all(&sub_logs_dir)?;

    // Included, because the pattern only excludes directories named `logs.py`
    fs::write(src.join("logs.py"), "import im_included")?;

    // This file is included. Ruff does not support the gitignore syntax where `logs.py/` matches any
    // directory named `logs.py`
    fs::write(sub_logs_dir.join("included2.py"), "import im_included")?;

    assert_cmd_snapshot!(
        ruff_check(tempdir.path())
        .args(["--config"])
        .arg(ruff_toml.file_name().unwrap()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:8: F401 [*] `im_included` imported but unused
    src/logs.py:1:8: F401 [*] `im_included` imported but unused
    src/nested/logs.py/output/included2.py:1:8: F401 [*] `im_included` imported but unused
    Found 3 errors.
    [*] 3 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    Ok(())
}

/// Tests that a directory pattern doesn't match a file with the same name.
///
/// ```toml
/// exclude = ["logs/"]
/// ```
///
/// Excludes the directory `logs` but not a file named `logs`.
#[test]
fn exclude_dirname_doesnt_match_file() -> Result<()> {
    let tempdir = TempDir::new()?;
    let ruff_toml = tempdir.path().join("ruff.toml");
    fs::write(
        &ruff_toml,
        r#"
select = ["F401"]
exclude = ["logs.py/"]
"#,
    )?;

    fs::write(tempdir.path().join("main.py"), "import im_included")?;

    // Included, because the pattern only excludes directories named `logs.py`
    fs::write(tempdir.path().join("logs.py"), "import im_included")?;

    assert_cmd_snapshot!(
        ruff_check(tempdir.path())
        .args(["--config"])
        .arg(ruff_toml.file_name().unwrap()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    main.py:1:8: F401 [*] `im_included` imported but unused
    Found 1 error.
    [*] 1 fixable with the `--fix` option.

    ----- stderr -----
    "###);
    Ok(())
}
