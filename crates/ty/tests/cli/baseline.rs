use std::fs;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn update_and_use_configured_baseline() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("pyproject.toml", "[tool.ty]\nbaseline = 'baseline.json'\n"),
        (
            "test.py",
            r#"
            from typing_extensions import reveal_type

            x: int = "wrong"
            reveal_type(x)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    5 | reveal_type(x)
      |             ^ `int`

    Found 1 diagnostic
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");

    let serialized = fs::read_to_string(case.root().join("baseline.json"))?;
    assert!(serialized.ends_with('\n'));
    let value: serde_json::Value = serde_json::from_str(&serialized)?;
    assert_eq!(value["version"], 0);
    assert_eq!(value["files"]["test.py"].as_array().unwrap().len(), 1);
    assert_eq!(value["files"]["test.py"][0]["rule"], "invalid-assignment");
    assert!(value["files"]["test.py"][0].get("message").is_none());

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    5 | reveal_type(x)
      |             ^ `int`

    Found 1 diagnostic

    ----- stderr -----
    ");

    case.write_file(
        "test.py",
        r#"
        from typing_extensions import reveal_type

        x: int = "wrong"
        y: int = "new"
        reveal_type(x)
        "#,
    )?;
    assert_cmd_snapshot!(case.command().arg("--output-format=concise"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:5:10: error[invalid-assignment] Object of type `Literal["new"]` is not assignable to `int`
    test.py:6:13: info[revealed-type] Revealed type: `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:6:13
      |
    6 | reveal_type(x)
      |             ^ `int`

    Found 1 diagnostic
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");
    let updated = fs::read_to_string(case.root().join("baseline.json"))?;
    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:6:13
      |
    6 | reveal_type(x)
      |             ^ `int`

    Found 1 diagnostic
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");
    assert_eq!(
        fs::read_to_string(case.root().join("baseline.json"))?,
        updated
    );
    Ok(())
}

#[test]
fn cli_baseline_path() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x: int = 'wrong'\n")?;

    assert_cmd_snapshot!(case.command().args(["--baseline", "state/baseline.json", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/state/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");
    assert!(case.root().join("state/baseline.json").is_file());

    assert_cmd_snapshot!(case.command().args(["--baseline", "state/baseline.json"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn update_requires_path_and_rejects_conflicts() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x: int = 'wrong'\n")?;
    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: `--update-baseline` requires a baseline path from `--baseline` or configuration
    ");

    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline", "--fix"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--update-baseline' cannot be used with '--fix'

    Usage: ty check --baseline <PATH> --update-baseline [PATH]...

    For more information, try '--help'.
    ");
    Ok(())
}

#[test]
fn update_fails_when_non_baselineable_diagnostics_remain() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "if:\n")?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline"]), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]: Expected an expression
     --> test.py:1:3
      |
    1 | if:
      |   ^

    error[invalid-syntax]: Expected an indented block after `if` statement
     --> test.py:1:4
      |
    1 | if:
      |    ^

    Found 2 diagnostics
    Updated baseline `<temp_dir>/baseline.json` with 0 diagnostics.

    ----- stderr -----
    ");

    let baseline: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(case.root().join("baseline.json"))?)?;
    assert_eq!(baseline["files"], serde_json::json!({}));
    Ok(())
}

#[test]
fn invalid_baseline_is_reported_once() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("pyproject.toml", "[tool.ty]\nbaseline = 'baseline.json'\n"),
        ("baseline.json", "{ not json"),
        ("test.py", "x: int = 'wrong'\n"),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--output-format=concise"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    baseline.json:1:1: error[invalid-baseline] Failed to parse baseline `<temp_dir>/baseline.json`
    test.py:1:10: error[invalid-assignment] Object of type `Literal["wrong"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    // Update mode intentionally ignores the invalid old baseline and replaces it.
    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn baseline_json_is_path_keyed() -> anyhow::Result<()> {
    let case = CliTest::with_files([("b.py", "b: int = 'b'\n"), ("a.py", "a: int = 'a'\n")])?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");

    let baseline = fs::read_to_string(case.root().join("baseline.json"))?;
    assert_snapshot!(baseline.lines().take(6).collect::<Vec<_>>().join("\n"), @r#"
    {
      "version": 0,
      "files": {
        "a.py": [
          {
            "rule": "invalid-assignment",
    "#);
    Ok(())
}
