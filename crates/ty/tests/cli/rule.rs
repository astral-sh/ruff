use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;

fn ty_cmd() -> Command {
    let mut cmd = Command::new(get_cargo_bin("ty"));
    cmd.env_clear();
    cmd
}

#[test]
fn rule_default_output() {
    assert_cmd_snapshot!(ty_cmd().args(["explain", "rule", "invalid-return-statement"]), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    # invalid-return-statement

    Default level: error | Stable (since 0.0.97)

    ## What it does

    Checks whether the value in a `return` statement is assignable to the function's annotated return
    type. In a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the
    returned value must be assignable to `ReturnType`.

    A bare `return` is equivalent to `return None` and is checked by this rule. If execution falls off
    the end of a function, Python also returns `None`. ty checks that case with
    `missing-return-statement` for nonempty bodies and `empty-body` for empty bodies.

    ## Why is this bad?

    Returning an object of a type incompatible with the annotated return type is unsound, and will lead
    to ty inferring incorrect types elsewhere.

    ## Examples

    ```python
    def func() -> int:
        return "a"  # error: [invalid-return-statement]
    ```

    ## Compatibility

    ty 0.0.97 renamed `invalid-return-type` to `invalid-return-statement`. The old name is an alias for
    this rule. Missing returns and incompatible generator annotations now have their own rules,
    `missing-return-statement` and `invalid-generator-return-type`.

    ----- stderr -----
    "#);
}

#[test]
fn rule_json_output() {
    assert_cmd_snapshot!(ty_cmd().args(["explain", "rule", "invalid-return-statement", "--output-format", "json"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "name": "invalid-return-statement",
      "summary": "detects return statements whose value is incompatible with the expected return type",
      "documentation": "## What it does\n\nChecks whether the value in a `return` statement is assignable to the function's annotated return\ntype. In a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the\nreturned value must be assignable to `ReturnType`.\n\nA bare `return` is equivalent to `return None` and is checked by this rule. If execution falls off\nthe end of a function, Python also returns `None`. ty checks that case with\n`missing-return-statement` for nonempty bodies and `empty-body` for empty bodies.\n\n## Why is this bad?\n\nReturning an object of a type incompatible with the annotated return type is unsound, and will lead\nto ty inferring incorrect types elsewhere.\n\n## Examples\n\n```python\ndef func() -> int:\n    return \"a\"  # error: [invalid-return-statement]\n```\n\n## Compatibility\n\nty 0.0.97 renamed `invalid-return-type` to `invalid-return-statement`. The old name is an alias for\nthis rule. Missing returns and incompatible generator annotations now have their own rules,\n`missing-return-statement` and `invalid-generator-return-type`.",
      "default_level": "error",
      "status": {
        "type": "stable",
        "since": "0.0.97"
      }
    }
    ----- stderr -----
    "###);
}

#[test]
fn preview_rule_status() -> anyhow::Result<()> {
    let output = ty_cmd()
        .args(["explain", "rule", "missing-direct-dependency"])
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Default level: ignore | Preview (since 0.0.76)"),
        "{stdout}"
    );

    let output = ty_cmd()
        .args([
            "explain",
            "rule",
            "missing-direct-dependency",
            "--output-format",
            "json",
        ])
        .output()?;
    assert!(output.status.success());
    let rule: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        rule["status"],
        serde_json::json!({"type": "preview", "since": "0.0.76"})
    );

    Ok(())
}

#[test]
fn rule_unknown() {
    assert_cmd_snapshot!(ty_cmd().args(["explain", "rule", "does-not-exist"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Unknown rule `does-not-exist`
    ");
}

/// Without a selector, we default to explaining all rules, so just assert that the command succeeds
/// rather than snapshotting the documentation for every rule.
#[test]
fn rule_no_selector() -> anyhow::Result<()> {
    assert!(ty_cmd().args(["explain", "rule"]).status()?.success());

    Ok(())
}
