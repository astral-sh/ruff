//! A test suite that ensures deprecated command line options have appropriate warnings / behaviors

use ruff_linter::settings::types::SerializationFormat;
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "ruff";

const STDIN: &str = "l = 1";

fn ruff_check(show_source: Option<bool>, output_format: Option<String>) -> Command {
    let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
    let output_format = output_format.unwrap_or(format!("{}", SerializationFormat::default(false)));
    cmd.arg("--output-format");
    cmd.arg(output_format);
    cmd.arg("--no-cache");
    match show_source {
        Some(true) => {
            cmd.arg("--show-source");
        }
        Some(false) => {
            cmd.arg("--no-show-source");
        }
        None => {}
    }
    cmd.arg("-");

    cmd
}

#[test]
fn ensure_show_source_is_deprecated() {
    assert_cmd_snapshot!(ruff_check(Some(true), None).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--show-source` argument is deprecated and has been ignored in favor of `--output-format=concise`.
    "###);
}

#[test]
fn ensure_no_show_source_is_deprecated() {
    assert_cmd_snapshot!(ruff_check(Some(false), None).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--no-show-source` argument is deprecated and has been ignored in favor of `--output-format=concise`.
    "###);
}

#[test]
fn ensure_output_format_is_deprecated() {
    assert_cmd_snapshot!(ruff_check(None, Some("text".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: `--output-format=text` is deprecated. Use `--output-format=full` or `--output-format=concise` instead. `text` will be treated as `concise`.
    "###);
}

#[test]
fn ensure_output_format_overrides_show_source() {
    assert_cmd_snapshot!(ruff_check(Some(true), Some("concise".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--show-source` argument is deprecated and has been ignored in favor of `--output-format=concise`.
    "###);
}

#[test]
fn ensure_full_output_format_overrides_no_show_source() {
    assert_cmd_snapshot!(ruff_check(Some(false), Some("full".into())).pass_stdin(STDIN), @r###"
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
    warning: The `--no-show-source` argument is deprecated and has been ignored in favor of `--output-format=full`.
    "###);
}

#[test]
fn ensure_output_format_uses_concise_over_no_show_source() {
    assert_cmd_snapshot!(ruff_check(Some(false), Some("concise".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--no-show-source` argument is deprecated and has been ignored in favor of `--output-format=concise`.
    "###);
}

#[test]
fn ensure_deprecated_output_format_overrides_show_source() {
    assert_cmd_snapshot!(ruff_check(Some(true), Some("text".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--show-source` argument is deprecated and has been ignored in favor of `--output-format=text`.
    warning: `--output-format=text` is deprecated. Use `--output-format=full` or `--output-format=concise` instead. `text` will be treated as `concise`.
    "###);
}

#[test]
fn ensure_deprecated_output_format_overrides_no_show_source() {
    assert_cmd_snapshot!(ruff_check(Some(false), Some("text".into())).pass_stdin(STDIN), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    -:1:1: E741 Ambiguous variable name: `l`
    Found 1 error.

    ----- stderr -----
    warning: The `--no-show-source` argument is deprecated and has been ignored in favor of `--output-format=text`.
    warning: `--output-format=text` is deprecated. Use `--output-format=full` or `--output-format=concise` instead. `text` will be treated as `concise`.
    "###);
}
