#![cfg(not(target_family = "wasm"))]

use std::env::set_current_dir;
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
const BIN_NAME: &str = "ruff";

#[test]
fn check_in_deleted_directory_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    set_current_dir(&temp_path).unwrap();
    drop(temp_dir);

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME)).arg("check"), @r###"
            success: false
            exit_code: 2
            ----- stdout -----

            ----- stderr -----
            ruff failed
              Cause: Working directory does not exist
            "###);
}
