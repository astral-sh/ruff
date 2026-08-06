use std::process::{Command, Output};

use anyhow::{Context, bail};

use crate::CliTest;

fn git(case: &CliTest, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(case.root())
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn commit_baseline(case: &CliTest) -> anyhow::Result<()> {
    git(case, &["init", "--quiet", "--initial-branch=main"])?;
    git(case, &["add", "--all"])?;
    git(
        case,
        &[
            "-c",
            "user.name=ty tests",
            "-c",
            "user.email=ty@example.com",
            "commit",
            "--quiet",
            "--message=baseline",
        ],
    )
}

fn check_diff(case: &CliTest) -> anyhow::Result<Output> {
    case.command()
        .arg("--diff")
        .arg("HEAD")
        .arg("--output-format")
        .arg("concise")
        .output()
        .context("Failed to run ty in Git diff mode")
}

fn stdout(output: &Output) -> anyhow::Result<&str> {
    std::str::from_utf8(&output.stdout).context("ty returned non-UTF-8 output")
}

#[test]
fn existing_diagnostics_moved_by_insertions_are_suppressed() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "existing: int = 'old'\n")?;
    commit_baseline(&case)?;

    case.write_file(
        "example.py",
        "header = 1\nexisting: int = 'old'\nintroduced: str = 42\n",
    )?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("example.py:3:"), "{output_text}");
    assert!(!output_text.contains("example.py:2:"), "{output_text}");
    assert!(output_text.contains("Found 1 diagnostic"), "{output_text}");

    Ok(())
}

#[test]
fn diagnostics_introduced_in_unchanged_files_are_reported() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("provider.py", "def provide() -> int:\n    return 1\n"),
        (
            "consumer.py",
            "from provider import provide\n\nvalue: int = provide()\n",
        ),
    ])?;
    commit_baseline(&case)?;

    case.write_file("provider.py", "def provide() -> str:\n    return 'text'\n")?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("consumer.py:3:"), "{output_text}");
    assert!(output_text.contains("invalid-assignment"), "{output_text}");

    Ok(())
}

#[test]
fn unchanged_baseline_errors_do_not_fail() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "existing: int = 'old'\n")?;
    commit_baseline(&case)?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(output.status.success(), "{output_text}");
    assert!(output_text.contains("All checks passed!"), "{output_text}");

    Ok(())
}

#[test]
fn the_default_revision_compares_against_the_default_branch() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "existing: int = 'old'\n")?;
    commit_baseline(&case)?;
    git(&case, &["checkout", "--quiet", "-b", "feature"])?;
    case.write_file(
        "example.py",
        "header = 1\nexisting: int = 'old'\nintroduced: str = 42\n",
    )?;
    git(&case, &["add", "--all"])?;
    git(
        &case,
        &[
            "-c",
            "user.name=ty tests",
            "-c",
            "user.email=ty@example.com",
            "commit",
            "--quiet",
            "--message=feature",
        ],
    )?;

    let output = case
        .command()
        .arg("--diff")
        .arg("--output-format")
        .arg("concise")
        .output()?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("example.py:3:"), "{output_text}");
    assert!(!output_text.contains("example.py:2:"), "{output_text}");

    Ok(())
}

#[test]
fn staged_changes_are_included() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "value = 1\n")?;
    commit_baseline(&case)?;
    case.write_file("example.py", "value: int = 'staged'\n")?;
    git(&case, &["add", "example.py"])?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("example.py:1:"), "{output_text}");

    Ok(())
}

#[test]
fn unrelated_binary_changes_do_not_prevent_checking() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "value = 1\n")?;
    std::fs::write(case.root().join("image.bin"), [0xff, 0xfe, 0x00])?;
    commit_baseline(&case)?;
    std::fs::write(case.root().join("image.bin"), [0xfe, 0xff, 0x00])?;
    case.write_file("example.py", "value: int = 'new'\n")?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("example.py:1:"), "{output_text}");

    Ok(())
}

#[test]
fn untracked_python_files_are_checked() -> anyhow::Result<()> {
    let case = CliTest::with_file("existing.py", "value = 1\n")?;
    commit_baseline(&case)?;
    case.write_file("untracked.py", "value: int = 'new'\n")?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("untracked.py:1:"), "{output_text}");

    Ok(())
}

#[test]
fn renamed_existing_diagnostics_are_suppressed() -> anyhow::Result<()> {
    let case = CliTest::with_file("before.py", "existing: int = 'old'\n")?;
    commit_baseline(&case)?;
    git(&case, &["mv", "before.py", "after.py"])?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(output.status.success(), "{output_text}");
    assert!(output_text.contains("All checks passed!"), "{output_text}");

    Ok(())
}

#[test]
fn deleting_an_imported_module_reports_the_new_import_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("provider.py", "value = 1\n"),
        ("consumer.py", "from provider import value\n"),
    ])?;
    commit_baseline(&case)?;
    std::fs::remove_file(case.root().join("provider.py"))?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("consumer.py:1:"), "{output_text}");
    assert!(output_text.contains("unresolved-import"), "{output_text}");

    Ok(())
}

#[test]
fn deleting_an_imported_package_reports_the_new_import_error() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("package/__init__.py", ""),
        ("package/provider.py", "value = 1\n"),
        ("consumer.py", "from package.provider import value\n"),
    ])?;
    commit_baseline(&case)?;
    std::fs::remove_dir_all(case.root().join("package"))?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("consumer.py:1:"), "{output_text}");
    assert!(output_text.contains("unresolved-import"), "{output_text}");

    Ok(())
}

#[test]
fn changed_diagnostic_messages_are_reported() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "value: int = 'old'\n")?;
    commit_baseline(&case)?;
    case.write_file("example.py", "value: int = 'new'\n")?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("Literal[\"new\"]"), "{output_text}");

    Ok(())
}

#[test]
fn configuration_changes_recheck_the_project() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("example.py", "value: int = 'existing'\n"),
        ("ty.toml", "[rules]\ninvalid-assignment = 'ignore'\n"),
    ])?;
    commit_baseline(&case)?;
    case.write_file("ty.toml", "[rules]\ninvalid-assignment = 'error'\n")?;

    let output = check_diff(&case)?;
    let output_text = stdout(&output)?;
    assert!(!output.status.success(), "{output_text}");
    assert!(output_text.contains("example.py:1:"), "{output_text}");

    Ok(())
}

#[test]
fn invalid_git_revisions_fail_clearly() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "value = 1\n")?;
    commit_baseline(&case)?;

    let output = case
        .command()
        .arg("--diff")
        .arg("does-not-exist")
        .output()?;
    let stderr = std::str::from_utf8(&output.stderr)?;
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("does-not-exist"), "{stderr}");

    Ok(())
}

#[test]
fn diff_mode_rejects_incompatible_modes() -> anyhow::Result<()> {
    let case = CliTest::with_file("example.py", "value = 1\n")?;

    for incompatible in ["--watch", "--fix", "--add-ignore"] {
        let output = case
            .command()
            .arg("--diff")
            .arg("HEAD")
            .arg(incompatible)
            .output()?;
        let stderr = std::str::from_utf8(&output.stderr)?;
        assert!(!output.status.success(), "{stderr}");
        assert!(stderr.contains(incompatible), "{stderr}");
    }

    Ok(())
}
