#![cfg(not(target_family = "wasm"))]

use std::path::Path;
use std::process::Command;
use std::str;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
const BIN_NAME: &str = "ruff";

#[test]
fn check_project_include_defaults() {
    // Defaults to checking the current working directory
    //
    // The test directory includes:
    //  - A pyproject.toml which specifies an include
    //  - A nested pyproject.toml which has a Ruff section
    //
    // The nested project should all be checked instead of respecting the parent includes

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/a.py
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/nested-project/e.py
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/nested-project/pyproject.toml
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/subdirectory/c.py

    ----- stderr -----
    "###);
}

#[test]
fn check_project_respects_direct_paths() {
    // Given a direct path not included in the project `includes`, it should be checked
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files", "b.py"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/b.py

    ----- stderr -----
    "###);
}

#[test]
fn check_project_respects_subdirectory_includes() {
    // Given a direct path to a subdirectory, the include should be respected
    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files", "subdirectory"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    /Users/mz/eng/src/astral-sh/ruff/crates/ruff_cli/resources/test/fixtures/include-test/subdirectory/c.py

    ----- stderr -----
    "###);
}
