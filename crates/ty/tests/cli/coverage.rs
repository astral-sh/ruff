use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn html_report() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("pkg/precise.py", "x: int = 1\n"),
        ("pkg/dynamic.py", "def f(y):\n    return y\n"),
    ])?;

    assert_cmd_snapshot!(
        case.command_with_subcommand("coverage").args(["--html", "report"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    Wrote coverage report to <temp_dir>/report/index.html (66.67% imprecise)

    ----- stderr -----
    "
    );

    let index = std::fs::read_to_string(case.root().join("report/index.html"))?;
    assert!(index.contains(r#"<a href="files/dynamic.py.html">dynamic.py</a>"#));
    assert!(index.contains(r#"<a href="files/precise.py.html">precise.py</a>"#));

    let page = std::fs::read_to_string(case.root().join("report/files/dynamic.py.html"))?;
    assert!(page.contains(r#"<span class="line-dynamic">def f(y):</span>"#));

    Ok(())
}

#[test]
fn nonexistent_path() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "x = 1\n")?;

    assert_cmd_snapshot!(
        case.command_with_subcommand("coverage").args(["missing.py", "--html", "report"]),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: path does not exist: <temp_dir>/missing.py
    "
    );

    Ok(())
}
