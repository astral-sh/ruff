use crate::config::Log;
use crate::parser::{CodeBlockDimensions, CodeBlockStructure};
use camino::Utf8Path;
use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::{Program, ProgramSettings, SearchPathSettings, SitePackages};
use ruff_db::diagnostic::{Diagnostic, ParseDiagnostic};
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::panic::catch_unwind;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
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
) {
    let source = std::fs::read_to_string(absolute_fixture_path).unwrap();
    let suite = match test_parser::parse(short_title, &source) {
        Ok(suite) => suite,
        Err(err) => {
            panic!("Error parsing `{absolute_fixture_path}`: {err:?}")
        }
    };

    let mut db = db::Db::setup(SystemPathBuf::from("/src"));

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

        // Remove all files so that the db is in a "fresh" state.
        db.memory_file_system().remove_all();
        Files::sync_all(&mut db);

        if let Err(failures) = run_test(&mut db, relative_fixture_path, snapshot_path, &test) {
            any_failures = true;
            println!("\n{}\n", test.name().bold().underline());

            let md_index = LineIndex::from_source_text(&source);

            for test_failures in failures {
                let code_block_structure =
                    CodeBlockStructure::new(&md_index, test_failures.code_block_dimensions);

                for (relative_line_number, failures) in test_failures.by_line.iter() {
                    let absolute_line_number =
                        code_block_structure.to_absolute_line_number(relative_line_number);

                    for failure in failures {
                        let line_info =
                            format!("{relative_fixture_path}:{absolute_line_number}").cyan();
                        println!("  {line_info} {failure}");
                    }
                }
            }

            println!(
                "\nTo rerun this specific test, set the environment variable: {MDTEST_TEST_FILTER}=\"{}\"",
                test.name()
            );
            println!(
                "{MDTEST_TEST_FILTER}=\"{}\" cargo test -p red_knot_python_semantic --test mdtest -- {test_name}",
                test.name()
            );
        }
    }

    println!("\n{}\n", "-".repeat(50));

    assert!(!any_failures, "Some tests failed.");
}

fn run_test<'s>(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &'s parser::MarkdownTest,
) -> Result<(), Failures<'s>> {
    let project_root = db.project_root().to_path_buf();
    let src_path = SystemPathBuf::from("/src");
    let custom_typeshed_path = test.configuration().typeshed().map(SystemPathBuf::from);
    let mut typeshed_files = vec![];
    let mut has_custom_versions_file = false;

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(embedded.lang, "py" | "pyi" | "text"),
                "Supported file types are: py, pyi, text"
            );

            let full_path = if embedded.path_str().starts_with('/') {
                SystemPathBuf::from(embedded.path_str())
            } else {
                project_root.join(embedded.path_str())
            };

            if let Some(ref typeshed_path) = custom_typeshed_path {
                if let Ok(relative_path) = full_path.strip_prefix(typeshed_path.join("stdlib")) {
                    if relative_path.as_str() == "VERSIONS" {
                        has_custom_versions_file = true;
                    } else if relative_path.extension().is_some_and(|ext| ext == "pyi") {
                        typeshed_files.push(relative_path.to_path_buf());
                    }
                }
            }

            db.write_file(&full_path, embedded.code()).unwrap();

            if !full_path.starts_with(&src_path) || embedded.lang == "text" {
                // These files need to be written to the file system (above), but we don't run any checks on them.
                return None;
            }

            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                code_block_dimensions: Box::new(embedded.code_block_dimensions()),
            })
        })
        .collect();

    // Create a custom typeshed `VERSIONS` file if none was provided.
    if let Some(ref typeshed_path) = custom_typeshed_path {
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

    Program::get(db)
        .update_from_settings(
            db,
            ProgramSettings {
                python_version: test.configuration().python_version().unwrap_or_default(),
                python_platform: test.configuration().python_platform().unwrap_or_default(),
                search_paths: SearchPathSettings {
                    src_roots: vec![src_path],
                    extra_paths: vec![],
                    custom_typeshed: custom_typeshed_path,
                    site_packages: SitePackages::Known(vec![]),
                },
            },
        )
        .expect("Failed to update Program settings in TestDb");

    // When snapshot testing is enabled, this is populated with
    // all diagnostics. Otherwise it remains empty.
    let mut snapshot_diagnostics = vec![];

    let failures: Failures = test_files
        .into_iter()
        .filter_map(|test_file| {
            let parsed = parsed_module(db, test_file.file);

            let mut diagnostics: Vec<Box<_>> = parsed
                .errors()
                .iter()
                .cloned()
                .map(|error| {
                    let diagnostic: Box<dyn Diagnostic> =
                        Box::new(ParseDiagnostic::new(test_file.file, error));
                    diagnostic
                })
                .collect();

            let type_diagnostics = match catch_unwind(|| check_types(db, test_file.file)) {
                Ok(type_diagnostics) => type_diagnostics,
                Err(info) => {
                    let mut by_line = matcher::FailuresByLine::default();
                    let mut messages = vec![];
                    match info.location {
                        Some(location) => messages.push(format!("panicked at {location}")),
                        None => messages.push("panicked at unknown location".to_string()),
                    };
                    match info.payload {
                        Some(payload) => messages.push(payload),
                        // Mimic the default panic hook's rendering of the panic payload if it's
                        // not a string.
                        None => messages.push("Box<dyn Any>".to_string()),
                    };
                    if let Some(backtrace) = info.backtrace {
                        if std::env::var("RUST_BACKTRACE").is_ok() {
                            messages.extend(backtrace.to_string().split('\n').map(String::from));
                        }
                    }
                    by_line.push(OneIndexed::from_zero_indexed(0), messages);
                    return Some(FileFailures {
                        code_block_dimensions: test_file.code_block_dimensions,
                        by_line,
                    });
                }
            };
            diagnostics.extend(type_diagnostics.into_iter().map(|diagnostic| {
                let diagnostic: Box<dyn Diagnostic> = Box::new((*diagnostic).clone());
                diagnostic
            }));

            let failure =
                match matcher::match_file(db, test_file.file, diagnostics.iter().map(|d| &**d)) {
                    Ok(()) => None,
                    Err(line_failures) => Some(FileFailures {
                        code_block_dimensions: test_file.code_block_dimensions,
                        by_line: line_failures,
                    }),
                };
            if test.should_snapshot_diagnostics() {
                snapshot_diagnostics.extend(diagnostics);
            }
            failure
        })
        .collect();

    if !snapshot_diagnostics.is_empty() {
        let snapshot =
            create_diagnostic_snapshot(db, relative_fixture_path, test, snapshot_diagnostics);
        let name = test.name().replace(' ', "_");
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

type Failures<'s> = Vec<FileFailures<'s>>;

/// The failures for a single file in a test by line number.
struct FileFailures<'s> {
    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    code_block_dimensions: Box<dyn Iterator<Item = CodeBlockDimensions> + 's>,

    /// The failures by lines in the file.
    by_line: matcher::FailuresByLine,
}

/// File in a test.
struct TestFile<'s> {
    file: File,

    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    code_block_dimensions: Box<dyn Iterator<Item = CodeBlockDimensions> + 's>,
}

fn create_diagnostic_snapshot<D: Diagnostic>(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    test: &parser::MarkdownTest,
    diagnostics: impl IntoIterator<Item = D>,
) -> String {
    // TODO(ag): Do something better than requiring this
    // global state to be twiddled everywhere.
    colored::control::set_override(false);

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
        writeln!(snapshot, "## {}", file.path_str()).unwrap();
        writeln!(snapshot).unwrap();
        // Note that we don't use ```py here because the line numbering
        // we add makes it invalid Python. This sacrifices syntax
        // highlighting when you look at the snapshot on GitHub,
        // but the line numbers are extremely useful for analyzing
        // snapshots. So we keep them.
        writeln!(snapshot, "```").unwrap();

        let line_number_width = file.code().lines().count().to_string().len();
        for (i, line) in file.code().lines().enumerate() {
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
        writeln!(snapshot, "{}", diag.display(db)).unwrap();
        writeln!(snapshot, "```").unwrap();
    }
    snapshot
}
