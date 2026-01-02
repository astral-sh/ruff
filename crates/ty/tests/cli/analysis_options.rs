use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

/// ty ignores `type: ignore` comments when setting `respect-type-ignore-comments=false`
#[test]
fn respect_type_ignore_comments_is_turned_off() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
            y = a + 5  # type: ignore
            "#,
    )?;

    // Assert that there's an `unresolved-reference` diagnostic (error).
    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command().arg("--config").arg("analysis.respect-type-ignore-comments=false"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `a` used when not defined
     --> test.py:2:5
      |
    2 | y = a + 5  # type: ignore
      |     ^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}
