use ordermap::map::OrderMap;
use red_knot_python_semantic::types::check_types;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::path::PathBuf;

type FxOrderMap<K, V> = OrderMap<K, V, BuildHasherDefault<FxHasher>>;

type Failures = FxOrderMap<SystemPathBuf, matcher::FailuresByLine>;

mod assertion;
mod db;
mod diagnostic;
mod matcher;
mod parser;

/// Run given file path as a markdown test suite.
///
/// Panic on test failure, and print failure details.
#[allow(clippy::print_stdout)]
pub fn run(path: &PathBuf) {
    let relpath: PathBuf = path
        .components()
        .rev()
        .take_while(|component| {
            if let std::path::Component::Normal(os_str) = component {
                os_str.to_str().is_some_and(|s| s != "mdtest")
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .collect();
    let source = std::fs::read_to_string(path).unwrap();
    let suite = parser::parse(relpath.to_str().unwrap(), &source).unwrap();

    let mut any_failures = false;
    for test in suite.tests() {
        if let Err(failures) = run_test(&test) {
            any_failures = true;
            println!("{}", test.name());

            for (path, by_line) in failures {
                println!("  {path}");
                for (line, failures) in by_line {
                    for failure in failures {
                        println!("    line {line}: {failure}");
                    }
                }
                println!();
            }
        }
    }

    assert!(!any_failures, "Some tests failed.");
}

fn run_test(test: &parser::MarkdownTest) -> Result<(), Failures> {
    let workspace_root = SystemPathBuf::from("/src");
    let mut db = db::TestDb::setup(workspace_root.clone());

    let mut system_paths = vec![];

    for file in test.files() {
        assert!(
            file.lang == "py" || file.lang == "pyi",
            "Non-Python files not supported yet."
        );
        let full_path = workspace_root.join(file.path);
        db.write_file(&full_path, file.code).unwrap();
        system_paths.push(full_path);
    }

    let mut failures = FxOrderMap::default();

    for path in system_paths {
        let file = system_path_to_file(&db, path.clone()).unwrap();
        let parsed = parsed_module(&db, file);

        // TODO allow testing against code with syntax errors
        assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());

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
