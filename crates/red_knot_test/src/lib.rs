use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use ruff_db::files::{system_path_to_file, Files};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::TextSize;
use std::path::Path;

type Failures = Vec<matcher::FailuresByLine>;

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

        if let Err(failures) = run_test(&mut db, &test, suite.line_index()) {
            any_failures = true;
            println!("\n{}\n", test.name().bold().underline());

            for by_line in failures {
                for (line_number, failures) in by_line.iter() {
                    for failure in failures {
                        let line_info = format!("{title}:{line_number}").cyan();

                        println!("    {line_info} {failure}");
                    }
                }
                println!();
            }
        }
    }

    println!("{}\n", "-".repeat(50));

    assert!(!any_failures, "Some tests failed.");
}

struct AbsoluteLineNumberPath {
    path: SystemPathBuf,

    // Offset of the backticks that starts the code block in the Markdown file
    md_offset: TextSize,
}

fn run_test(db: &mut db::Db, test: &parser::MarkdownTest, line_index: &LineIndex) -> Result<(), Failures> {
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
            md_offset: file.md_offset,
        });
    }

    let mut failures = Vec::with_capacity(paths.len());

    for contextual_path in paths {
        let file = system_path_to_file(db, contextual_path.path.clone()).unwrap();
        let parsed = parsed_module(db, file);

        let mut cached_starting_line_number: Option<OneIndexed> = None;

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
                let offset = cached_starting_line_number.unwrap_or_else(|| {
                    let offset = line_index.line_index(contextual_path.md_offset);
                    cached_starting_line_number = Some(offset);
                    offset
                });

                failures.push(line_failures.offset_errors(offset));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
