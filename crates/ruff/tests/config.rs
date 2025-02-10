//! Tests for the `ruff config` subcommand.
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "ruff";

#[test]
fn lint_select() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).arg("config").arg("lint.select"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    A list of rule codes or prefixes to enable. Prefixes can specify exact
    rules (like `F841`), entire categories (like `F`), or anything in
    between.

    When breaking ties between enabled and disabled rules (via `select` and
    `ignore`, respectively), more specific prefixes override less
    specific prefixes. `ignore` takes precedence over `select` if the
    same prefix appears in both.

    Default value: ["E4", "E7", "E9", "F"]
    Type: list[RuleSelector]
    Example usage:
    ```toml
    # On top of the defaults (`E4`, E7`, `E9`, and `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
    select = ["E4", "E7", "E9", "F", "B", "Q"]
    ```

    ----- stderr -----
    "#
    );
}

#[test]
fn lint_select_json() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).arg("config").arg("lint.select").arg("--output-format").arg("json"), @r##"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "doc": "A list of rule codes or prefixes to enable. Prefixes can specify exact\nrules (like `F841`), entire categories (like `F`), or anything in\nbetween.\n\nWhen breaking ties between enabled and disabled rules (via `select` and\n`ignore`, respectively), more specific prefixes override less\nspecific prefixes. `ignore` takes precedence over `select` if the\nsame prefix appears in both.",
      "default": "[\"E4\", \"E7\", \"E9\", \"F\"]",
      "value_type": "list[RuleSelector]",
      "scope": null,
      "example": "# On top of the defaults (`E4`, E7`, `E9`, and `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).\nselect = [\"E4\", \"E7\", \"E9\", \"F\", \"B\", \"Q\"]",
      "deprecated": null
    }

    ----- stderr -----
    "##
    );
}
