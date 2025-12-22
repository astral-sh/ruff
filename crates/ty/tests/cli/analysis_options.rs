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

    assert_cmd_snapshot!(case.command().arg("--config").arg("analysis.respect-type-ignore-comments=false"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
    3 |
    4 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}
