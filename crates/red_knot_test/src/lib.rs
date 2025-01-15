use crate::config::Log;
use camino::Utf8Path;
use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::Program;
use ruff_db::diagnostic::{Diagnostic, ParseDiagnostic};
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::panic::catch_unwind;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use ruff_db::testing::{setup_logging, setup_logging_with_filter};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::TextSize;
use salsa::Setter;

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

        Program::get(&db)
            .set_python_version(&mut db)
            .to(test.configuration().python_version().unwrap_or_default());
        Program::get(&db)
            .set_python_platform(&mut db)
            .to(test.configuration().python_platform().unwrap_or_default());

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
    let workspace_root = db.workspace_root().to_path_buf();

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(embedded.lang, "py" | "pyi"),
                "Non-Python files not supported yet."
            );
            let full_path = workspace_root.join(embedded.path);
            db.write_file(&full_path, embedded.code).unwrap();
            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                backtick_offset: embedded.md_offset,
            })
        })
        .collect();

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
