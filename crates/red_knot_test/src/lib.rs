use crate::config::Log;
use crate::parser::{BacktickOffsets, EmbeddedFileSourceMap};
use camino::Utf8Path;
use colored::Colorize;
use config::SystemKind;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::{Program, ProgramSettings, PythonPath, SearchPathSettings};
use ruff_db::diagnostic::{create_parse_diagnostic, Diagnostic, DisplayDiagnosticConfig};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::panic::catch_unwind;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
use ruff_db::testing::{setup_logging, setup_logging_with_filter};
use ruff_source_file::{LineIndex, OneIndexed};
use std::fmt::Write;

mod assertion;
mod config;
mod db;
mod diagnostic;
mod matcher;
mod parser;

const MDTEST_TEST_FILTER: &str = "MDTEST_TEST_FILTER";

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
#[allow(clippy::print_stdout)]
pub fn run(
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    short_title: &str,
    test_name: &str,
    output_format: OutputFormat,
) {
    let source = std::fs::read_to_string(absolute_fixture_path).unwrap();
    let suite = match test_parser::parse(short_title, &source) {
        Ok(suite) => suite,
        Err(err) => {
            panic!("Error parsing `{absolute_fixture_path}`: {err:?}")
        }
    };

    let mut db = db::Db::setup();

    let filter = std::env::var(MDTEST_TEST_FILTER).ok();
    let mut any_failures = false;
    for test in suite.tests() {
        if filter.as_ref().is_some_and(|f| !test.name().contains(f)) {
            continue;
        }

        let _tracing = test.configuration().log.as_ref().and_then(|log| match log {
            Log::Bool(enabled) => enabled.then(setup_logging),
            Log::Filter(filter) => setup_logging_with_filter(filter),
        });

        if let Err(failures) = run_test(&mut db, relative_fixture_path, snapshot_path, &test) {
            any_failures = true;

            if output_format.is_cli() {
                println!("\n{}\n", test.name().bold().underline());
            }

            let md_index = LineIndex::from_source_text(&source);

            for test_failures in failures {
                let source_map =
                    EmbeddedFileSourceMap::new(&md_index, test_failures.backtick_offsets);

                for (relative_line_number, failures) in test_failures.by_line.iter() {
                    let absolute_line_number =
                        source_map.to_absolute_line_number(relative_line_number);

                    for failure in failures {
                        match output_format {
                            OutputFormat::Cli => {
                                let line_info =
                                    format!("{relative_fixture_path}:{absolute_line_number}")
                                        .cyan();
                                println!("  {line_info} {failure}");
                            }
                            OutputFormat::GitHub => println!(
                                "::error file={absolute_fixture_path},line={absolute_line_number}::{failure}"
                            ),
                        }
                    }
                }
            }

            let escaped_test_name = test.name().replace('\'', "\\'");

            if output_format.is_cli() {
                println!(
                    "\nTo rerun this specific test, set the environment variable: {MDTEST_TEST_FILTER}='{escaped_test_name}'",
                );
                println!(
                    "{MDTEST_TEST_FILTER}='{escaped_test_name}' cargo test -p red_knot_python_semantic --test mdtest -- {test_name}",
                );
            }
        }
    }

    println!("\n{}\n", "-".repeat(50));

    assert!(!any_failures, "Some tests failed.");
}

/// Defines the format in which mdtest should print an error to the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// The format `cargo test` should use by default.
    Cli,
    /// A format that will provide annotations from GitHub Actions
    /// if mdtest fails on a PR.
    /// See <https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions#setting-an-error-message>
    GitHub,
}

impl OutputFormat {
    const fn is_cli(self) -> bool {
        matches!(self, OutputFormat::Cli)
    }
}

fn run_test(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &parser::MarkdownTest,
) -> Result<(), Failures> {
    // Initialize the system and remove all files and directories to reset the system to a clean state.
    match test.configuration().system.unwrap_or_default() {
        SystemKind::InMemory => {
            db.use_in_memory_system();
        }
        SystemKind::Os => {
            let dir = tempfile::TempDir::new().expect("Creating a temporary directory to succeed");
            let root_path = dir
                .path()
                .canonicalize()
                .expect("Canonicalizing to succeed");
            let root_path = SystemPathBuf::from_path_buf(root_path)
                .expect("Temp directory to be a valid UTF8 path")
                .simplified()
                .to_path_buf();

            db.use_os_system_with_temp_dir(root_path, dir);
        }
    }

    let project_root = SystemPathBuf::from("/src");
    db.create_directory_all(&project_root)
        .expect("Creating the project root to succeed");

    let src_path = project_root.clone();
    let custom_typeshed_path = test.configuration().typeshed();
    let mut typeshed_files = vec![];
    let mut has_custom_versions_file = false;

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(embedded.lang, "py" | "pyi" | "python" | "text"),
                "Supported file types are: py (or python), pyi, text, and ignore"
            );

            let full_path = embedded.full_path(&project_root);

            if let Some(typeshed_path) = custom_typeshed_path {
                if let Ok(relative_path) = full_path.strip_prefix(typeshed_path.join("stdlib")) {
                    if relative_path.as_str() == "VERSIONS" {
                        has_custom_versions_file = true;
                    } else if relative_path.extension().is_some_and(|ext| ext == "pyi") {
                        typeshed_files.push(relative_path.to_path_buf());
                    }
                }
            }

            db.write_file(&full_path, &embedded.code).unwrap();

            if !full_path.starts_with(&src_path) || embedded.lang == "text" {
                // These files need to be written to the file system (above), but we don't run any checks on them.
                return None;
            }

            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                backtick_offsets: embedded.backtick_offsets.clone(),
            })
        })
        .collect();

    // Create a custom typeshed `VERSIONS` file if none was provided.
    if let Some(typeshed_path) = custom_typeshed_path {
        if !has_custom_versions_file {
            let versions_file = typeshed_path.join("stdlib/VERSIONS");
            let contents = typeshed_files
                .iter()
                .fold(String::new(), |mut content, path| {
                    // This is intentionally kept simple:
                    let module_name = path
                        .as_str()
                        .trim_end_matches(".pyi")
                        .trim_end_matches("/__init__")
                        .replace('/', ".");
                    let _ = writeln!(content, "{module_name}: 3.8-");
                    content
                });
            db.write_file(&versions_file, contents).unwrap();
        }
    }

    let settings = ProgramSettings {
        python_version: test.configuration().python_version().unwrap_or_default(),
        python_platform: test.configuration().python_platform().unwrap_or_default(),
        search_paths: SearchPathSettings {
            src_roots: vec![src_path],
            extra_paths: test
                .configuration()
                .extra_paths()
                .unwrap_or_default()
                .to_vec(),
            custom_typeshed: custom_typeshed_path.map(SystemPath::to_path_buf),
            python_path: PythonPath::KnownSitePackages(vec![]),
        },
    };

    match Program::try_get(db) {
        Some(program) => program.update_from_settings(db, settings),
        None => Program::from_settings(db, settings).map(|_| ()),
    }
    .expect("Failed to update Program settings in TestDb");

    // When snapshot testing is enabled, this is populated with
    // all diagnostics. Otherwise it remains empty.
    let mut snapshot_diagnostics = vec![];

    let failures: Failures = test_files
        .into_iter()
        .filter_map(|test_file| {
            let parsed = parsed_module(db, test_file.file);

            let mut diagnostics: Vec<Diagnostic> = parsed
                .errors()
                .iter()
                .map(|error| create_parse_diagnostic(test_file.file, error))
                .collect();

            let type_diagnostics = match catch_unwind(|| check_types(db, test_file.file)) {
                Ok(type_diagnostics) => type_diagnostics,
                Err(info) => {
                    let mut by_line = matcher::FailuresByLine::default();
                    let mut messages = vec![];
                    match info.location {
                        Some(location) => messages.push(format!("panicked at {location}")),
                        None => messages.push("panicked at unknown location".to_string()),
                    }
                    match info.payload {
                        Some(payload) => messages.push(payload),
                        // Mimic the default panic hook's rendering of the panic payload if it's
                        // not a string.
                        None => messages.push("Box<dyn Any>".to_string()),
                    }
                    if let Some(backtrace) = info.backtrace {
                        if std::env::var("RUST_BACKTRACE").is_ok() {
                            messages.extend(backtrace.to_string().split('\n').map(String::from));
                        }
                    }
                    by_line.push(OneIndexed::from_zero_indexed(0), messages);
                    return Some(FileFailures {
                        backtick_offsets: test_file.backtick_offsets,
                        by_line,
                    });
                }
            };
            diagnostics.extend(type_diagnostics.into_iter().cloned());

            let failure = match matcher::match_file(db, test_file.file, &diagnostics) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offsets: test_file.backtick_offsets,
                    by_line: line_failures,
                }),
            };
            if test.should_snapshot_diagnostics() {
                snapshot_diagnostics.extend(diagnostics);
            }
            failure
        })
        .collect();

    if snapshot_diagnostics.is_empty() && test.should_snapshot_diagnostics() {
        panic!(
            "Test `{}` requested snapshotting diagnostics but it didn't produce any.",
            test.name()
        );
    } else if !snapshot_diagnostics.is_empty() {
        let snapshot =
            create_diagnostic_snapshot(db, relative_fixture_path, test, snapshot_diagnostics);
        let name = test.name().replace(' ', "_").replace(':', "__");
        insta::with_settings!(
            {
                snapshot_path => snapshot_path,
                input_file => name.clone(),
                filters => vec![(r"\\", "/")],
                prepend_module_to_snapshot => false,
            },
            { insta::assert_snapshot!(name, snapshot) }
        );
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

type Failures = Vec<FileFailures>;

/// The failures for a single file in a test by line number.
struct FileFailures {
    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    backtick_offsets: Vec<BacktickOffsets>,

    /// The failures by lines in the file.
    by_line: matcher::FailuresByLine,
}

/// File in a test.
struct TestFile {
    file: File,

    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    backtick_offsets: Vec<BacktickOffsets>,
}

fn create_diagnostic_snapshot(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    test: &parser::MarkdownTest,
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> String {
    let display_config = DisplayDiagnosticConfig::default().color(false);

    let mut snapshot = String::new();
    writeln!(snapshot).unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot, "mdtest name: {}", test.name()).unwrap();
    writeln!(snapshot, "mdtest path: {relative_fixture_path}").unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot).unwrap();

    writeln!(snapshot, "# Python source files").unwrap();
    writeln!(snapshot).unwrap();
    for file in test.files() {
        writeln!(snapshot, "## {}", file.relative_path()).unwrap();
        writeln!(snapshot).unwrap();
        // Note that we don't use ```py here because the line numbering
        // we add makes it invalid Python. This sacrifices syntax
        // highlighting when you look at the snapshot on GitHub,
        // but the line numbers are extremely useful for analyzing
        // snapshots. So we keep them.
        writeln!(snapshot, "```").unwrap();

        let line_number_width = file.code.lines().count().to_string().len();
        for (i, line) in file.code.lines().enumerate() {
            let line_number = i + 1;
            writeln!(snapshot, "{line_number:>line_number_width$} | {line}").unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        writeln!(snapshot).unwrap();
    }

    writeln!(snapshot, "# Diagnostics").unwrap();
    writeln!(snapshot).unwrap();
    for (i, diag) in diagnostics.into_iter().enumerate() {
        if i > 0 {
            writeln!(snapshot).unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        write!(snapshot, "{}", diag.display(db, &display_config)).unwrap();
        writeln!(snapshot, "```").unwrap();
    }
    snapshot
}
