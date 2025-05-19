use std::path::Path;
use std::process::Command;

use anyhow::Context;
use assert_fs::TempDir;
use assert_fs::fixture::PathChild;
use insta::assert_snapshot;
use itertools::Itertools;
use ty_project::sys_path::SystemPath;

use ruff_cache::CACHE_DIR_NAME;
use ruff_db::program::ProgramSettings;
use ruff_db::source::SourceOrigin;
use ruff_db::system::{MemorySystem, SystemPathBuf};
use ty_project::settings::SettingsBindDropGuard;

// TODO: Copied from ruff_cli.
// Consider extracting to a shared test support crate.

/// Set of options to run the `ty` command with.
#[derive(Debug, Default)]
struct TestConfig {
    /// Arguments to pass to the `ty` command.
    args: Vec<String>,
    /// Files to write to the temporary directory.
    files: Vec<(&'static str, &'static str)>,
    /// Whether to run the command with `PYTHONPATH` set to the `site-packages` directory.
    python_path: bool,
}

#[track_caller]
fn run_cli(config: TestConfig) -> anyhow::Result<String> {
    let case = TestCase::with_files(config.files)?;
    let mut cmd = case.command();
    if config.python_path {
        cmd.env("PYTHONPATH", case.site_packages());
    }
    let output = cmd
        .args(config.args)
        .output()
        .context("Failed to run command")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Command failed with status {}:\nstderr:{}\nstdout:{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ));
    }

    let stdout = String::from_utf8(output.stdout)?;
    // let stderr = String::from_utf8(output.stderr)?;
    Ok(stdout)
}

#[test]
fn check_json_output() -> anyhow::Result<()> {
    let case = TestCase::with_files([("test.py", "a: int = \"hello\"")])?;
    let mut cmd = case.command();
    cmd.arg("--output-format")
        .arg("json")
        .arg(case.root().join("test.py"));

    let output = cmd.output()?;

    assert_eq!(
        output.status.code(),
        Some(1),
        "Command should exit with 1 due to type errors, even with JSON output."
    );

    let stdout = String::from_utf8(output.stdout)?;

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct JsonLocation {
        row: usize,
        column: usize,
    }

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct ExpectedJsonDiagnostic {
        code: String,
        severity: String,
        message: String,
        path: String,
        location: JsonLocation,
    }

    let diagnostics: Vec<ExpectedJsonDiagnostic> = serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse JSON output: {}", stdout))?;

    assert_eq!(diagnostics.len(), 1, "Expected one diagnostic");
    let diagnostic = &diagnostics[0];

    // This TYC0001 might need to be TYC0002 if TYC0001 is reserved or used for something else.
    // It depends on how error codes are assigned in `ty`.
    assert_eq!(diagnostic.code, "TYC0002");
    assert_eq!(diagnostic.severity, "error");
    assert!(
        diagnostic
            .message
            .contains("Expected `int` but got `LiteralString[\"hello\"]`")
    );

    assert!(
        diagnostic.path.ends_with("test.py"),
        "Path should end with test.py, got: {}",
        diagnostic.path
    );

    assert_eq!(diagnostic.location, JsonLocation { row: 1, column: 11 });

    // Regression test: ensure --output-format xml (or any invalid) fails
    let mut cmd_invalid = case.command();
    cmd_invalid
        .arg("--output-format")
        .arg("xml")
        .arg(case.root().join("test.py"));
    let output_invalid = cmd_invalid.output()?;
    assert_eq!(
        output_invalid.status.code(),
        Some(2),
        "Command with invalid format should fail with exit code 2 (clap error)."
    );

    let stderr_invalid = String::from_utf8(output_invalid.stderr)?;
    assert!(stderr_invalid.contains("invalid value 'xml' for '--output-format <OUTPUT_FORMAT>'"));
    assert!(stderr_invalid.contains("possible values: full, concise, json"));

    Ok(())
}

// Helper struct and functions for setting up test cases
struct TestCase {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
}

impl TestCase {
    fn with_files<const N: usize>(
        files: [(&'static str, &'static str); N],
    ) -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        for (name, content) in files {
            std::fs::write(temp_dir.child(name), content)?;
        }

        let program_settings = ProgramSettings {
            target_version: Default::default(),
            interpreter: Default::default(),
            search_paths: Default::default(),
            extra_paths: vec![SystemPathBuf::from_std_path(temp_dir.path()).unwrap()],
            workspace_root: SystemPathBuf::from_std_path(temp_dir.path()).unwrap(),
            site_packages: None, // Adjust if needed for specific tests
            source_origin: SourceOrigin::Cli(None),
        };

        let settings_scope = ty_project::settings::global_program_settings().bind(program_settings);

        Ok(Self {
            _temp_dir: temp_dir,
            _settings_scope: settings_scope,
        })
    }

    fn root(&self) -> &Path {
        self._temp_dir.path()
    }

    fn site_packages(&self) -> &Path {
        // This might need adjustment if your test setup for site-packages is different
        self._temp_dir
            .path()
            .join(".venv")
            .join("lib")
            .join("pythonX.Y")
            .join("site-packages")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ty"));
        cmd.current_dir(self.root());
        cmd.env_remove("PYTHONPATH"); // Ensure clean environment
        // Add any other common env vars or setup needed for ty commands
        cmd
    }
}

// Minimal example test using the helper
#[test]
fn basic_type_error_concise_output() -> anyhow::Result<()> {
    let case = TestCase::with_files([("example.py", "a: int = 'world'")])?;
    let mut cmd = case.command();
    cmd.arg("check")
        .arg("--output-format")
        .arg("concise")
        .arg("example.py");

    let output = cmd.output()?;
    assert!(!output.status.success());

    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("error[TYC0002]: Expected `int` but got `LiteralString[\"world\"]`"));
    assert!(stdout.contains("example.py:1:11"));
    Ok(())
}
