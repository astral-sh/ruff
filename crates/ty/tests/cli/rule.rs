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
    type.

    A bare `return` statement is equivalent to `return None` and is checked by this rule. However, cases
    where a code path is missing a `return` statement entirely -- leading to execution "falling off" the
    end of a function and the function implicitly returning `None` in that code path -- are reported by
    either `missing-return-statement` or `empty-body`.

    In a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the value of a
    `return` statement must be assignable to `ReturnType`.

    ## Why is this bad?

    Returning an object of a type incompatible with the annotated return type is unsound, and will lead
    to ty inferring incorrect types elsewhere.

    ## Examples

    ```python
    from collections.abc import Generator, AsyncGenerator


    def func(coinflip: bool) -> int:
        if coinflip:
            return "a"  # error: [invalid-return-statement]
        else:
            return  # error: [invalid-return-statement]


    def gen_func() -> Generator[int, None, str]:
        yield 42
        return b"oops, a bytestring"  # error: [invalid-return-statement]
    ```

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
      "documentation": "## What it does\n\nChecks whether the value in a `return` statement is assignable to the function's annotated return\ntype.\n\nA bare `return` statement is equivalent to `return None` and is checked by this rule. However, cases\nwhere a code path is missing a `return` statement entirely -- leading to execution \"falling off\" the\nend of a function and the function implicitly returning `None` in that code path -- are reported by\neither `missing-return-statement` or `empty-body`.\n\nIn a generator function annotated with `Generator[YieldType, SendType, ReturnType]`, the value of a\n`return` statement must be assignable to `ReturnType`.\n\n## Why is this bad?\n\nReturning an object of a type incompatible with the annotated return type is unsound, and will lead\nto ty inferring incorrect types elsewhere.\n\n## Examples\n\n```python\nfrom collections.abc import Generator, AsyncGenerator\n\n\ndef func(coinflip: bool) -> int:\n    if coinflip:\n        return \"a\"  # error: [invalid-return-statement]\n    else:\n        return  # error: [invalid-return-statement]\n\n\ndef gen_func() -> Generator[int, None, str]:\n    yield 42\n    return b\"oops, a bytestring\"  # error: [invalid-return-statement]\n```",
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
