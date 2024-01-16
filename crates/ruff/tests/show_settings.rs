use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::Path;
use std::process::Command;

const BIN_NAME: &str = "ruff";

#[cfg(not(target_os = "windows"))]
const TEST_FILTERS: &[(&str, &str)] = &[
    ("\"[^\\*\"]*/pyproject.toml", "\"[BASEPATH]/pyproject.toml"),
    ("\".*/crates", "\"[BASEPATH]/crates"),
    ("\".*/\\.ruff_cache", "\"[BASEPATH]/.ruff_cache"),
    ("\".*/ruff\"", "\"[BASEPATH]\""),
];
#[cfg(target_os = "windows")]
const TEST_FILTERS: &[(&str, &str)] = &[
    (r#""[^\*"]*\\pyproject.toml"#, "\"[BASEPATH]/pyproject.toml"),
    (r#"".*\\crates"#, "\"[BASEPATH]/crates"),
    (r#"".*\\\.ruff_cache"#, "\"[BASEPATH]/.ruff_cache"),
    (r#"".*\\ruff""#, "\"[BASEPATH]\""),
    (r#"\\+(\w\w|\s|")"#, "/$1"),
];

#[test]
fn display_default_settings() {
    insta::with_settings!({ filters => TEST_FILTERS.to_vec() }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(["check", "--show-settings", "unformatted.py"]).current_dir(Path::new("./resources/test/fixtures")));
    });
}
