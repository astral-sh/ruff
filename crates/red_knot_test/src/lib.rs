use crate::config::Log;
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
use ruff_text_size::TextSize;
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
pub fn run(path: &Utf8Path, long_title: &str, short_title: &str, test_name: &str) {
    let source = std::fs::read_to_string(path).unwrap();
    let suite = match test_parser::parse(short_title, &source) {
        Ok(suite) => suite,
        Err(err) => {
            panic!("Error parsing `{path}`: {err:?}")
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

        if let Err(failures) = run_test(&mut db, &test) {
            any_failures = true;
            println!("\n{}\n", test.name().bold().underline());

            let md_index = LineIndex::from_source_text(&source);

            for test_failures in failures {
                let backtick_line = md_index.line_index(test_failures.backtick_offset);

                for (relative_line_number, failures) in test_failures.by_line.iter() {
                    for failure in failures {
                        let absolute_line_number =
                            backtick_line.checked_add(relative_line_number).unwrap();
                        let line_info = format!("{long_title}:{absolute_line_number}").cyan();
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

fn run_test(db: &mut db::Db, test: &parser::MarkdownTest) -> Result<(), Failures> {
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

            let full_path = if embedded.path.starts_with('/') {
                SystemPathBuf::from(embedded.path)
            } else {
                project_root.join(embedded.path)
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

            db.write_file(&full_path, embedded.code).unwrap();

            if !full_path.starts_with(&src_path) || embedded.lang == "text" {
                // These files need to be written to the file system (above), but we don't run any checks on them.
                return None;
            }

            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                backtick_offset: embedded.md_offset,
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
                        backtick_offset: test_file.backtick_offset,
                        by_line,
                    });
                }
            };
            diagnostics.extend(type_diagnostics.into_iter().map(|diagnostic| {
                let diagnostic: Box<dyn Diagnostic> = Box::new((*diagnostic).clone());
                diagnostic
            }));

            match matcher::match_file(db, test_file.file, diagnostics) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offset: test_file.backtick_offset,
                    by_line: line_failures,
                }),
            }
        })
        .collect();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

type Failures = Vec<FileFailures>;

/// The failures for a single file in a test by line number.
struct FileFailures {
    /// The offset of the backticks that starts the code block in the Markdown file
    backtick_offset: TextSize,
    /// The failures by lines in the code block.
    by_line: matcher::FailuresByLine,
}

/// File in a test.
struct TestFile {
    file: File,

    // Offset of the backticks that starts the code block in the Markdown file
    backtick_offset: TextSize,
}
