use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::Path;
use std::process::Command;

const BIN_NAME: &str = "ruff";

#[test]
fn display_default_settings() {
    // Navigate from the crate directory to the workspace root.
    let base_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let base_path = base_path.to_string_lossy();

    // Escape the backslashes for the regex.
    let base_path = regex::escape(&base_path);

    #[cfg(not(target_os = "windows"))]
    let test_filters = &[(base_path.as_ref(), "[BASEPATH]")];

    #[cfg(target_os = "windows")]
    let test_filters = &[
        (base_path.as_ref(), "[BASEPATH]"),
        (r#"\\+(\w\w|\s|\.|")"#, "/$1"),
    ];

    insta::with_settings!({ filters => test_filters.to_vec() }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(["check", "--show-settings", "unformatted.py"]).current_dir(Path::new("./resources/test/fixtures")));
    });
}
