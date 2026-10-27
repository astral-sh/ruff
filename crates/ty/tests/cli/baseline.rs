use std::fs;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn update_and_use_configured_baseline() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r"
            [tool.ty]
            baseline = 'baseline.json'
            ",
        ),
        (
            "test.py",
            r#"
            from typing_extensions import reveal_type

            x: int = "wrong"
            reveal_type(x)
            "#,
        ),
    ])?;

    // `reveal_type` is not a lint, so it remains visible when the assignment error is baselined.
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
    assert_snapshot!(serialized, @r#"
    {
      "version": 0,
      "files": {
        "test.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 16,
            "lineCount": 1
          }
        ]
      }
    }
    "#);

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
    Ok(())
}

#[test]
fn matches_duplicate_ranges_in_source_order() -> anyhow::Result<()> {
    let source = "\
𐐀: int = '''a
🍎'''
x: int = 1.5
";
    let case = CliTest::with_file("test.py", source)?;

    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");
    assert_snapshot!(fs::read_to_string(case.root().join("baseline.json"))?, @r#"
    {
      "version": 0,
      "files": {
        "test.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 4,
            "lineCount": 2
          },
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 12,
            "lineCount": 1
          }
        ]
      }
    }
    "#);

    // The inserted assignment has the same rule and columns as the final assignment.
    // The multiline diagnostic between them lets Myers distinguish the two occurrences.
    case.write_file("test.py", &format!("y: int = 0.5\n{source}"))?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--output-format=concise"]), @"
    success: false
    exit_code: 1
    ----- stdout -----
    test.py:1:10: error[invalid-assignment] Object of type `float*` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn update_baseline_is_idempotent() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r"
        x: int = 'wrong'
        print(missing)
        ",
    )?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline", "--warn", "invalid-assignment"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");
    let baseline = fs::read_to_string(case.root().join("baseline.json"))?;

    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline", "--warn", "unresolved-reference"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");
    assert_eq!(
        fs::read_to_string(case.root().join("baseline.json"))?,
        baseline
    );
    Ok(())
}

#[test]
fn ignore_comment_replaces_baseline_entry() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r"
            [tool.ty]
            baseline = 'baseline.json'

            [tool.ty.rules]
            unused-ignore-comment = 'error'
            ",
        ),
        (
            "test.py",
            r"
            x: int = 'wrong'
            ",
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");

    case.write_file(
        "test.py",
        r"
        x: int = 'wrong'  # ty: ignore[invalid-assignment]
        ",
    )?;
    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 0 diagnostics.

    ----- stderr -----
    ");
    assert_snapshot!(fs::read_to_string(case.root().join("baseline.json"))?, @r#"
    {
      "version": 0,
      "files": {}
    }
    "#);
    Ok(())
}

#[test]
fn cli_baseline_path() -> anyhow::Result<()> {
    let source_path = r"nested/a\b.py";
    let case = CliTest::with_file(
        source_path,
        r"
        x: int = 'wrong'
        ",
    )?;

    assert_cmd_snapshot!(case.command().args(["--baseline", "state/baseline.json", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/state/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");
    let baseline: serde_json::Value =
        serde_json::from_slice(&fs::read(case.root().join("state/baseline.json"))?)?;
    // A backslash separates directories on Windows and is part of the filename on Unix.
    let expected_path = if cfg!(windows) {
        "nested/a/b.py"
    } else {
        source_path
    };
    assert_eq!(
        baseline["files"][expected_path][0]["rule"],
        "invalid-assignment"
    );

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
fn update_baseline_requires_path() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r"
        x: int = 'wrong'
        ",
    )?;
    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: `--update-baseline` requires a baseline path from `--baseline` or configuration
    ");
    Ok(())
}

#[test]
fn update_baseline_conflicts_with_fix() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r"
        x: int = 'wrong'
        ",
    )?;
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
fn update_baseline_does_not_suppress_syntax_errors() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r"
        x: int = 'wrong'

        if:
        ",
    )?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline"]), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]: Expected an expression
     --> test.py:4:3
      |
    4 | if:
      |   ^

    error[invalid-syntax]: Expected an indented block after `if` statement
     --> test.py:4:4
      |
    4 | if:
      |    ^

    Found 2 diagnostics
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");

    let baseline = fs::read_to_string(case.root().join("baseline.json"))?;
    assert_snapshot!(baseline, @r#"
    {
      "version": 0,
      "files": {
        "test.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 16,
            "lineCount": 1
          }
        ]
      }
    }
    "#);
    Ok(())
}

#[test]
fn missing_baseline_is_reported() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "")?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "missing.json", "--output-format=concise"]), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-baseline] Failed to read baseline `<temp_dir>/missing.json`
    Found 1 diagnostic

    ----- stderr -----
    ");
    Ok(())
}

#[test]
fn unsupported_baseline_version_is_reported() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", ""),
        ("baseline.json", r#"{"version":999,"files":{}}"#),
    ])?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json"]), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-baseline]: Unsupported baseline version `999`
     --> baseline.json:1:1
      |
    1 | {"version":999,"files":{}}
      | ^
    info: ty supports baseline version `0`

    Found 1 diagnostic

    ----- stderr -----
    "#);
    Ok(())
}

#[test]
fn update_baseline_replaces_invalid_json() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r"
            [tool.ty]
            baseline = 'baseline.json'
            ",
        ),
        ("baseline.json", "{ not json"),
        (
            "test.py",
            r"
            x: int = 'wrong'
            ",
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--output-format=concise"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    baseline.json:1:1: error[invalid-baseline] Failed to parse baseline `<temp_dir>/baseline.json`
    test.py:2:10: error[invalid-assignment] Object of type `Literal["wrong"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");
    let baseline = fs::read_to_string(case.root().join("baseline.json"))?;
    assert_snapshot!(baseline, @r#"
    {
      "version": 0,
      "files": {
        "test.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 16,
            "lineCount": 1
          }
        ]
      }
    }
    "#);
    Ok(())
}

#[test]
fn baseline_entries_are_sorted_and_matched_by_file_path() -> anyhow::Result<()> {
    let source = "x: int = 'wrong'\n";
    let case = CliTest::with_files([("b.py", source), ("a.py", source)])?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 2 diagnostics.

    ----- stderr -----
    ");

    let baseline = fs::read_to_string(case.root().join("baseline.json"))?;
    assert_snapshot!(baseline, @r#"
    {
      "version": 0,
      "files": {
        "a.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 16,
            "lineCount": 1
          }
        ],
        "b.py": [
          {
            "rule": "invalid-assignment",
            "startColumn": 9,
            "endColumn": 16,
            "lineCount": 1
          }
        ]
      }
    }
    "#);

    // Both files have identical baseline entries. Only the error in the renamed file should reappear.
    fs::rename(case.root().join("a.py"), case.root().join("renamed.py"))?;
    assert_cmd_snapshot!(case.command().args(["--baseline", "baseline.json", "--output-format=concise"]), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    renamed.py:1:10: error[invalid-assignment] Object of type `Literal["wrong"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);
    Ok(())
}
