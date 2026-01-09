use anyhow::Result;

use crate::CliTest;

#[test]
fn external_ast_reports_all_parse_errors() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "lint/external/alpha.toml",
        r#"
name = "Alpha"

[[rule]]
code = "AXX001"
name = "AlphaRule"
targets = ["stmt:FunctionDef"]
script = "rules/alpha.py"
"#,
    )?;
    test.write_file(
        "lint/external/beta.toml",
        r#"
name = "Beta"

[[rule]]
code = "BXX001"
name = "BetaRule"
targets = ["stmt:FunctionDef"]
script = "rules/beta.py"
"#,
    )?;
    test.write_file("lint/external/rules/alpha.py", "def alpha(:\n")?;
    test.write_file("lint/external/rules/beta.py", "def beta(:\n")?;

    let config = format!(
        r#"
[lint.external-ast.alpha]
path = "{}"

[lint.external-ast.beta]
path = "{}"
"#,
        test.root().join("lint/external/alpha.toml").display(),
        test.root().join("lint/external/beta.toml").display(),
    );
    test.write_file("ruff.toml", &config)?;

    let output = test
        .command()
        .args(["check", "--config", "ruff.toml", "."])
        .output()?;

    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = std::str::from_utf8(&output.stderr)?;
    assert!(
        stderr.contains("alpha"),
        "stderr missing alpha error: {stderr}"
    );
    assert!(
        stderr.contains("beta"),
        "stderr missing beta error: {stderr}"
    );
    Ok(())
}

#[test]
fn external_ast_executes_rules() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "lint/external/demo.toml",
        r#"
name = "Demo"

[[rule]]
code = "EXT001"
name = "ExampleRule"
targets = ["stmt:FunctionDef"]
script = "rules/example.py"
"#,
    )?;
    test.write_file(
        "lint/external/rules/example.py",
        r#"
def check_stmt(node, ctx):
    if node["_kind"] == "FunctionDef":
        ctx.report("hello from script")
"#,
    )?;
    let linter_path = test.root().join("lint/external/demo.toml");
    let config = format!(
        r#"
[lint.external-ast.demo]
path = "{}"
"#,
        linter_path.display()
    );
    test.write_file("ruff.toml", &config)?;
    test.write_file(
        "src/example.py",
        r#"
def demo(value=0):
    return value
"#,
    )?;

    let output = test
        .command()
        .args([
            "check",
            "--config",
            "ruff.toml",
            "--select",
            "RUF300",
            "--select-external",
            "EXT001",
            "src/example.py",
        ])
        .output()?;
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("EXT001"), "stdout missing EXT001: {stdout}");
    assert!(
        stdout.contains("hello from script"),
        "stdout missing message: {stdout}"
    );
    Ok(())
}

#[test]
fn external_ast_respects_noqa() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "lint/external/demo.toml",
        r#"
name = "Demo"

[[rule]]
code = "EXT001"
name = "ExampleRule"
targets = ["stmt:FunctionDef"]
script = "rules/example.py"
"#,
    )?;
    test.write_file(
        "lint/external/rules/example.py",
        r#"
def check_stmt(node, ctx):
    if node["_kind"] == "FunctionDef":
        ctx.report("hello from script")
"#,
    )?;
    let linter_path = test.root().join("lint/external/demo.toml");
    let config = format!(
        r#"
[lint.external-ast.demo]
path = "{}"
"#,
        linter_path.display()
    );
    test.write_file("ruff.toml", &config)?;
    test.write_file(
        "src/example.py",
        r#"
def demo(value=0):  # noqa: EXT001
    return value
"#,
    )?;

    let output = test
        .command()
        .args([
            "check",
            "--config",
            "ruff.toml",
            "--select",
            "RUF300",
            "--select-external",
            "EXT001",
            "src/example.py",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "command unexpectedly failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("EXT001"),
        "stdout unexpectedly reported EXT001: {stdout}"
    );
    Ok(())
}

#[test]
fn external_logging_linter_reports_interpolation() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(
        "lint/external/logging.toml",
        r#"
name = "Logging"

[[rule]]
code = "EXT801"
name = "LoggingInterpolation"
targets = ["expr:Call"]
call_callee_regex = "(?i).*log.*\\.(debug|info|warning|warn|error|exception|critical|fatal)$"
script = "logging/logging_interpolation.py"
"#,
    )?;
    test.write_file(
        "lint/external/logging/logging_interpolation.py",
        include_str!("../fixtures/external/logging_linter.py"),
    )?;
    let linter_path = test.root().join("lint/external/logging.toml");
    let config = format!(
        r#"
[lint.external-ast.logging]
path = "{}"
"#,
        linter_path.display()
    );
    test.write_file("ruff.toml", &config)?;
    test.write_file(
        "src/logging_cases.py",
        include_str!("../fixtures/external/logging_cases.py"),
    )?;

    let output = test
        .command()
        .args([
            "check",
            "--config",
            "ruff.toml",
            "--select",
            "RUF300",
            "--select-external",
            "EXT801",
            "src/logging_cases.py",
        ])
        .output()?;
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hits: Vec<_> = stdout
        .lines()
        .filter(|line| line.contains("EXT801"))
        .collect();
    assert!(
        !hits.is_empty(),
        "stdout missing EXT801 logging violations: {stdout}"
    );
    assert_eq!(
        hits.len(),
        3,
        "expected three logging interpolation diagnostics: {stdout}"
    );
    assert!(
        hits.iter()
            .all(|line| line.contains("LoggingInterpolation: Logging message")),
        "stdout missing logging interpolation message: {stdout}"
    );
    Ok(())
}
