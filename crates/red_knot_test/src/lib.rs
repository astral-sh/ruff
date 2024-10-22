use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use ruff_db::files::{system_path_to_file, Files};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use ruff_source_file::LineIndex;
use ruff_text_size::TextSize;
use std::path::Path;

mod assertion;
mod db;
mod diagnostic;
mod matcher;
mod parser;

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
#[allow(clippy::print_stdout)]
pub fn run(path: &Path, title: &str) {
    let source = std::fs::read_to_string(path).unwrap();
    let suite = match test_parser::parse(title, &source) {
        Ok(suite) => suite,
        Err(err) => {
            panic!("Error parsing `{}`: {err}", path.to_str().unwrap())
        }
    };

    let mut db = db::Db::setup(SystemPathBuf::from("/src"));

    let mut any_failures = false;
    for test in suite.tests() {
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
                        let line_info = format!("{title}:{absolute_line_number}").cyan();
                        println!("    {line_info} {failure}");
                    }
                }
            }
        }
    }

    println!("\n{}\n", "-".repeat(50));

    assert!(!any_failures, "Some tests failed.");
}

struct AbsoluteLineNumberPath {
    path: SystemPathBuf,

    // Offset of the backticks that starts the code block in the Markdown file
    backtick_offset: TextSize,
}

fn run_test(db: &mut db::Db, test: &parser::MarkdownTest) -> Result<(), Failures> {
    let workspace_root = db.workspace_root().to_path_buf();

    let mut paths: Vec<AbsoluteLineNumberPath> = Vec::with_capacity(test.files().count());

    for file in test.files() {
        assert!(
            matches!(file.lang, "py" | "pyi"),
            "Non-Python files not supported yet."
        );
        let full_path = workspace_root.join(file.path);
        db.write_file(&full_path, file.code).unwrap();
        paths.push(AbsoluteLineNumberPath {
            path: full_path,
            backtick_offset: file.md_offset,
        });
    }

    let mut failures = Vec::with_capacity(paths.len());

    for contextual_path in paths {
        let file = system_path_to_file(db, contextual_path.path.clone()).unwrap();
        let parsed = parsed_module(db, file);

        // TODO allow testing against code with syntax errors
        assert!(
            parsed.errors().is_empty(),
            "Python syntax errors in {}, {:?}: {:?}",
            test.name(),
            contextual_path.path,
            parsed.errors()
        );

        match matcher::match_file(db, file, check_types(db, file)) {
            Ok(()) => {}
            Err(line_failures) => {
                failures.push(TestFailures {
                    backtick_offset: contextual_path.backtick_offset,
                    by_line: line_failures,
                });
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

type Failures = Vec<TestFailures>;

/// Failures of a single markdown test.
struct TestFailures {
    /// The offset of the backticks that starts the code block in the Markdown file
    backtick_offset: TextSize,
    /// The failures by lines in the code block.
    by_line: matcher::FailuresByLine,
}
