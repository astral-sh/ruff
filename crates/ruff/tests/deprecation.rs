//! A test suite that ensures deprecated command line options have appropriate warnings / behaviors

use ruff_linter::settings::types::OutputFormat;
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "ruff";

const STDIN: &str = "l = 1";

fn ruff_check(output_format: Option<String>) -> Command {
    let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
    let output_format = output_format.unwrap_or(format!("{}", OutputFormat::default(false)));
    cmd.arg("check")
        .arg("--output-format")
        .arg(output_format)
        .arg("--no-cache");
    cmd.arg("-");

    cmd
}

#[test]
fn ensure_output_format_is_deprecated() {
    assert_cmd_snapshot!(ruff_check(Some("text".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: `--output-format=text` is deprecated. Use `--output-format=full` or `--output-format=concise` instead. `text` will be treated as `concise`.
    "###);
}
