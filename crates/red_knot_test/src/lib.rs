use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use ruff_db::files::{system_path_to_file, Files};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use ruff_source_file::OneIndexed;
use std::collections::BTreeMap;
use std::path::Path;

type Failures = BTreeMap<AbsoluteLineNumberPath, matcher::FailuresByLine>;

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

            for (
                AbsoluteLineNumberPath {
                    path,
                    line_number: absolute_line_number,
                },
                by_line,
            ) in failures
            {
                println!("{}", path.as_str().bold());
                for (line_number, failures) in by_line.iter() {
                    for failure in failures {
                        let absolute_line_info = format!(
                            "{}:{}",
                            title,
                            absolute_line_number.saturating_add(line_number.get())
                        );
                        let line_info = format!("line {line_number}:").cyan();
                        println!("{absolute_line_info}    {line_info} {failure}");
                    }
                }
                println!();
            }
        }
    }

    println!("{}\n", "-".repeat(50));

    assert!(!any_failures, "Some tests failed.");
}

#[derive(PartialEq, PartialOrd, Eq, Ord)]
struct AbsoluteLineNumberPath {
    path: SystemPathBuf,
    line_number: OneIndexed,
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
            line_number: file.md_line_number,
        });
    }

    let mut failures = BTreeMap::default();

    for numbered_path in paths {
        let path = numbered_path.path.clone();
        let file = system_path_to_file(db, path.clone()).unwrap();
        let parsed = parsed_module(db, file);

        // TODO allow testing against code with syntax errors
        assert!(
            parsed.errors().is_empty(),
            "Python syntax errors in {}, {:?}: {:?}",
            test.name(),
            path,
            parsed.errors()
        );

        matcher::match_file(db, file, check_types(db, file)).unwrap_or_else(|line_failures| {
            failures.insert(numbered_path, line_failures);
        });
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
