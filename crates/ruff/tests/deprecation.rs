//! A test suite that ensures deprecated command line options have appropriate warnings / behaviors

use ruff_linter::settings::types::OutputFormat;
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "ruff";

const STDIN: &str = "l = 1";

fn ruff_check(output_format: OutputFormat) -> Command {
    let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
    let output_format = output_format.to_string();
    cmd.arg("check")
        .arg("--output-format")
        .arg(output_format)
        .arg("--no-cache");
    cmd.arg("-");

    cmd
}

#[test]
#[allow(deprecated)]
fn ensure_output_format_is_deprecated() {
    assert_cmd_snapshot!(ruff_check(OutputFormat::Text).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: `--output-format=text` is no longer supported. Use `--output-format=full` or `--output-format=concise` instead.
    "###);
}
