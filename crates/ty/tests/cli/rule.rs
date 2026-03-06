use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;

fn ty_cmd() -> Command {
    let mut cmd = Command::new(get_cargo_bin("ty"));
    cmd.env_clear();
    cmd
}

#[test]
fn rule_default_output() {
    assert_cmd_snapshot!(ty_cmd().args(["rule", "invalid-return-type"]), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    # invalid-return-type

    Default level: error | Stable (since 0.0.1-alpha.1)

    ## What it does
    Detects returned values that can't be assigned to the function's annotated return type.

    Note that the special case of a function with a non-`None` return type and an empty body
    is handled by the separate `empty-body` error code.

    ## Why is this bad?
    Returning an object of a type incompatible with the annotated return type
    is unsound, and will lead to ty inferring incorrect types elsewhere.

    ## Examples
    ```python
    def func() -> int:
        return "a"  # error: [invalid-return-type]
    ```

    ----- stderr -----
    "#);
}

#[test]
fn rule_json_output() {
    assert_cmd_snapshot!(ty_cmd().args(["rule", "invalid-return-type", "--output-format", "json"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "name": "invalid-return-type",
      "summary": "detects returned values that can't be assigned to the function's annotated return type",
      "documentation": "## What it does\nDetects returned values that can't be assigned to the function's annotated return type.\n\nNote that the special case of a function with a non-`None` return type and an empty body\nis handled by the separate `empty-body` error code.\n\n## Why is this bad?\nReturning an object of a type incompatible with the annotated return type\nis unsound, and will lead to ty inferring incorrect types elsewhere.\n\n## Examples\n```python\ndef func() -> int:\n    return \"a\"  # error: [invalid-return-type]\n```",
      "default_level": "error",
      "status": {
        "type": "stable",
        "since": "0.0.1-alpha.1"
      }
    }

    ----- stderr -----
    "###);
}

#[test]
fn rule_unknown() {
    assert_cmd_snapshot!(ty_cmd().args(["rule", "does-not-exist"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Unknown rule `does-not-exist`
    ");
}

#[test]
fn rule_no_selector() {
    insta::with_settings!({ filters => vec![(r"ty\.exe", "ty")] }, {
        assert_cmd_snapshot!(ty_cmd().args(["rule"]), @"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: the following required arguments were not provided:
          <RULE|--all>

        Usage: ty rule <RULE|--all>

        For more information, try '--help'.
        ");
    });
}
