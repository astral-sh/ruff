use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn html_report() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("pkg/precise.py", "x: int = 1\n"),
        ("pkg/dynamic.py", "def f(y):\n    return y\n"),
    ])?;

    assert_cmd_snapshot!(case.command().args(["--html-report", "report"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!
    Wrote coverage report to <temp_dir>/report/index.html (66.67% imprecise)

    ----- stderr -----
    ");

    let index = std::fs::read_to_string(case.root().join("report/index.html"))?;
    assert!(index.contains(r#"<a href="files/dynamic.py.html">dynamic.py</a>"#));
    assert!(index.contains(r#"<a href="files/precise.py.html">precise.py</a>"#));

    let page = std::fs::read_to_string(case.root().join("report/files/dynamic.py.html"))?;
    assert!(page.contains(r#"<div class="line dynamic" id="L1""#));
    assert!(page.contains("<code>def f(y):</code>"));

    Ok(())
}

#[test]
fn html_report_summary_respects_output_format() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x: int = 1\n")?;

    // A machine-readable format keeps stdout parseable, so the summary line is left out.
    assert_cmd_snapshot!(
        case.command().args(["--html-report", "report", "--output-format", "gitlab"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    []
    ----- stderr -----
    "
    );
    assert!(case.root().join("report/index.html").exists());

    Ok(())
}

#[test]
fn html_report_summary_is_shown_when_quiet() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x: int = 1\n")?;

    // The user asked for the report, so `--quiet` still says where it was written.
    assert_cmd_snapshot!(case.command().args(["--html-report", "report", "--quiet"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Wrote coverage report to <temp_dir>/report/index.html (0.00% imprecise)

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn html_report_conflicts_with_fix() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x = 1\n")?;

    assert_cmd_snapshot!(case.command().args(["--html-report", "report", "--fix"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--html-report <DIR>' cannot be used with '--fix'

    Usage: ty check --html-report <DIR> [PATH]...

    For more information, try '--help'.
    ");

    Ok(())
}
