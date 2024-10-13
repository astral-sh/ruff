use colored::Colorize;
use parser as test_parser;
use red_knot_python_semantic::types::check_types;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use std::collections::BTreeMap;
use std::path::PathBuf;

type Failures = BTreeMap<SystemPathBuf, matcher::FailuresByLine>;

mod assertion;
mod db;
mod diagnostic;
mod matcher;
mod parser;

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
#[allow(clippy::print_stdout)]
pub fn run(path: &PathBuf, title: &str) {
    let source = std::fs::read_to_string(path).unwrap();
    let suite = match test_parser::parse(title, &source) {
        Ok(suite) => suite,
        Err(err) => {
            panic!("Error parsing `{}`: {err}", path.to_str().unwrap())
        }
    };

    let mut any_failures = false;
    for test in suite.tests() {
        if let Err(failures) = run_test(&test) {
            any_failures = true;
            println!("\n{}\n", test.name().bold().underline());

            for (path, by_line) in failures {
                println!("{}", path.as_str().bold());
                for (line_number, failures) in by_line.iter() {
                    for failure in failures {
                        let line_info = format!("line {line_number}:").cyan();
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

fn run_test(test: &parser::MarkdownTest) -> Result<(), Failures> {
    let workspace_root = SystemPathBuf::from("/src");
    let mut db = db::Db::setup(workspace_root.clone());

    let mut system_paths = vec![];

    for file in test.files() {
        assert!(
            matches!(file.lang, "py" | "pyi"),
            "Non-Python files not supported yet."
        );
        let full_path = workspace_root.join(file.path);
        db.write_file(&full_path, file.code).unwrap();
        system_paths.push(full_path);
    }

    let mut failures = BTreeMap::default();

    for path in system_paths {
        let file = system_path_to_file(&db, path.clone()).unwrap();
        let parsed = parsed_module(&db, file);

        // TODO allow testing against code with syntax errors
        assert!(
            parsed.errors().is_empty(),
            "Python syntax errors in {}, {:?}: {:?}",
            test.name(),
            path,
            parsed.errors()
        );

        matcher::match_file(&db, file, check_types(&db, file)).unwrap_or_else(|line_failures| {
            failures.insert(path, line_failures);
        });
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
