#![cfg(not(target_family = "wasm"))]

use std::path::Path;
use std::process::Command;
use std::str;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
const BIN_NAME: &str = "ruff";

#[cfg(not(target_os = "windows"))]
const TEST_FILTERS: &[(&str, &str)] = &[(".*/resources/test/fixtures/", "[BASEPATH]/")];
#[cfg(target_os = "windows")]
const TEST_FILTERS: &[(&str, &str)] = &[
    (r".*\\resources\\test\\fixtures\\", "[BASEPATH]\\"),
    (r"\\", "/"),
];

#[test]
fn check_project_include_defaults() {
    // Defaults to checking the current working directory
    //
    // The test directory includes:
    //  - A pyproject.toml which specifies an include
    //  - A nested pyproject.toml which has a Ruff section
    //
    // The nested project should all be checked instead of respecting the parent includes

    insta::with_settings!({
        filters => TEST_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        [BASEPATH]/include-test/a.py
        [BASEPATH]/include-test/nested-project/e.py
        [BASEPATH]/include-test/nested-project/pyproject.toml
        [BASEPATH]/include-test/subdirectory/c.py

        ----- stderr -----
        warning: The top-level linter settings are deprecated in favour of their counterparts in the `lint` section. Please update the following options in `nested-project/pyproject.toml`:
          - 'select' -> 'lint.select'
        ");
    });
}

#[test]
fn check_project_respects_direct_paths() {
    // Given a direct path not included in the project `includes`, it should be checked

    insta::with_settings!({
        filters => TEST_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files", "b.py"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        [BASEPATH]/include-test/b.py

        ----- stderr -----
        ");
    });
}

#[test]
fn check_project_respects_subdirectory_includes() {
    // Given a direct path to a subdirectory, the include should be respected

    insta::with_settings!({
        filters => TEST_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files", "subdirectory"]).current_dir(Path::new("./resources/test/fixtures/include-test")), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        [BASEPATH]/include-test/subdirectory/c.py

        ----- stderr -----
        ");
    });
}

#[test]
fn check_project_from_project_subdirectory_respects_includes() {
    // Run from a project subdirectory, the include specified in the parent directory should be respected

    insta::with_settings!({
        filters => TEST_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .args(["check", "--show-files"]).current_dir(Path::new("./resources/test/fixtures/include-test/subdirectory")), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        [BASEPATH]/include-test/subdirectory/c.py

        ----- stderr -----
        ");
    });
}
