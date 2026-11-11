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
    assert!(page.contains(r#"<span class="line-dynamic">def f(y):</span>"#));

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
