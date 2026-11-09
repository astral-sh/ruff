use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;

fn ty_cmd() -> Command {
    let mut cmd = Command::new(get_cargo_bin("ty"));
    cmd.env_clear();
    cmd
}

#[test]
fn rule_default_output() {
    assert_cmd_snapshot!(ty_cmd().args(["explain", "rule", "invalid-explicit-return"]), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    # invalid-explicit-return

    Default level: error | Stable (since 0.0.101)

    ## What it does

    Checks for `return` statements where the value returned is inconsistent with the function's
    return-type annotation.

    ## Why is this bad?

    A caller relying on the annotated return type can receive a value of an unexpected type and fail at
    runtime.

    ## Details and Examples

    In a regular (non-generator) function, the value returned by a `return` statement must be assignable
    to the function's annotated return type:

    ```py
    def meaning_of_life() -> int:
        return 42  # OK


    def bad() -> int:
        return "not a number"  # error: [invalid-explicit-return]
    ```

    This principle applies the same way to async functions:

    ```py
    async def meaning_of_life_async() -> int:
        return 42  # OK


    async def bad_async() -> int:
        return "not a number"  # error: [invalid-explicit-return]
    ```

    Note that a bare `return` statement -- without an explicit value being returned -- always returns
    `None`, so this can also be flagged by the rule:

    ```py
    def fallible_meaning_of_life(should_answer: bool) -> int | None:
        if not should_answer:
            return  # OK: the return-type annotation allows returning `None`
        return 42


    def bad_fallible_function(coinflip: bool) -> int:
        if coinflip:
            # This statement returns `None`, which is not allowed by the return-type annotation:
            return  # error: [invalid-explicit-return]
        return 42
    ```

    For a synchronous generator function annotated with `Generator`, `GeneratorType` or similar,
    however, a `return` statement is only valid if it returns a value consistent with the third type
    parameter supplied to `Generator`/`GeneratorType`:

    ```python
    from collections.abc import Generator


    def gen_func(coinflip: bool) -> Generator[int, None, str]:
        yield 42

        if coinflip:
            # this statement returns `None`, which is not assignable to the third type
            # parameter (`str`) supplied to `Generator` in the return annotation
            return  # error: [invalid-explicit-return]

        yield 42

        # and this returns a `bytes` object, which is also not assignable to `str`
        return b"oops, a bytestring"  # error: [invalid-explicit-return]
    ```

    ## Rule history

    - v0.0.14: Split out `empty-body` (now `invalid-implicit-return-in-stub-function`) for missing
        returns in stub-like bodies.

    - v0.0.101: Split out `invalid-implicit-return` for missing returns in nonempty bodies.

    - v0.0.101: Split out `invalid-generator-return-type` for incompatible generator return annotations.

    - v0.0.101: Renamed from `invalid-return-type`.

        ty accepts `invalid-return-type` in configuration and ignore comments for explicit returns and
        warns about the rename. To configure or suppress fallthrough or incompatible generator
        annotations, use `invalid-implicit-return` or `invalid-generator-return-type`, respectively.

    ----- stderr -----
    "#);
}

#[test]
fn rule_json_output() {
    assert_cmd_snapshot!(ty_cmd().args(["explain", "rule", "invalid-explicit-return", "--output-format", "json"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "name": "invalid-explicit-return",
      "summary": "detects return statements whose value is incompatible with the expected return type",
      "documentation": "## What it does\n\nChecks for `return` statements where the value returned is inconsistent with the function's\nreturn-type annotation.\n\n## Why is this bad?\n\nA caller relying on the annotated return type can receive a value of an unexpected type and fail at\nruntime.\n\n## Details and Examples\n\nIn a regular (non-generator) function, the value returned by a `return` statement must be assignable\nto the function's annotated return type:\n\n```py\ndef meaning_of_life() -> int:\n    return 42  # OK\n\n\ndef bad() -> int:\n    return \"not a number\"  # error: [invalid-explicit-return]\n```\n\nThis principle applies the same way to async functions:\n\n```py\nasync def meaning_of_life_async() -> int:\n    return 42  # OK\n\n\nasync def bad_async() -> int:\n    return \"not a number\"  # error: [invalid-explicit-return]\n```\n\nNote that a bare `return` statement -- without an explicit value being returned -- always returns\n`None`, so this can also be flagged by the rule:\n\n```py\ndef fallible_meaning_of_life(should_answer: bool) -> int | None:\n    if not should_answer:\n        return  # OK: the return-type annotation allows returning `None`\n    return 42\n\n\ndef bad_fallible_function(coinflip: bool) -> int:\n    if coinflip:\n        # This statement returns `None`, which is not allowed by the return-type annotation:\n        return  # error: [invalid-explicit-return]\n    return 42\n```\n\nFor a synchronous generator function annotated with `Generator`, `GeneratorType` or similar,\nhowever, a `return` statement is only valid if it returns a value consistent with the third type\nparameter supplied to `Generator`/`GeneratorType`:\n\n```python\nfrom collections.abc import Generator\n\n\ndef gen_func(coinflip: bool) -> Generator[int, None, str]:\n    yield 42\n\n    if coinflip:\n        # this statement returns `None`, which is not assignable to the third type\n        # parameter (`str`) supplied to `Generator` in the return annotation\n        return  # error: [invalid-explicit-return]\n\n    yield 42\n\n    # and this returns a `bytes` object, which is also not assignable to `str`\n    return b\"oops, a bytestring\"  # error: [invalid-explicit-return]\n```\n\n## Rule history\n\n- v0.0.14: Split out `empty-body` (now `invalid-implicit-return-in-stub-function`) for missing\n    returns in stub-like bodies.\n\n- v0.0.101: Split out `invalid-implicit-return` for missing returns in nonempty bodies.\n\n- v0.0.101: Split out `invalid-generator-return-type` for incompatible generator return annotations.\n\n- v0.0.101: Renamed from `invalid-return-type`.\n\n    ty accepts `invalid-return-type` in configuration and ignore comments for explicit returns and\n    warns about the rename. To configure or suppress fallthrough or incompatible generator\n    annotations, use `invalid-implicit-return` or `invalid-generator-return-type`, respectively.",
      "default_level": "error",
      "status": {
        "type": "stable",
        "since": "0.0.101"
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
