//! Exploratory source diagnostics; these controls do not assert initializer support.

use ruff_db::files::{File, system_path_to_file};
use ruff_python_ast::PythonVersion;
use salsa::Database as _;

use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::constructor::expansion_probe;

fn database(source: &str) -> anyhow::Result<(TestDb, File)> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/initializer.py", source)
        .build()?;
    let file = system_path_to_file(&db, "/src/initializer.py")?;
    Ok((db, file))
}

fn source(target: &str, callable: bool) -> String {
    let demand = if callable {
        format!("factory: Callable[[int], {target}] = {target}\nresult = factory(1)")
    } else {
        format!("result = {target}(1)")
    };
    format!(
        r#"from typing import Callable, Self, reveal_type

class Owner:
    __init__: Self
    def __call__(self, value: int) -> None: ...

class Receiver(Owner): ...

{demand}
reveal_type({target}.__init__)
reveal_type({target}.__call__)
reveal_type(result)
"#
    )
}

#[test]
#[ignore = "Exploratory source trace; does not assert initializer support"]
fn explore_source_self_initializer_direct_and_callable() -> anyhow::Result<()> {
    for target in ["Owner", "Receiver"] {
        for callable in [false, true] {
            let source = source(target, callable);
            let entry = if callable {
                "Callable assignment"
            } else {
                "direct call"
            };
            eprintln!("candidate {target}, {entry}:\n{source}");

            // The constructor demand precedes all reveals, and nothing is inferred before checking.
            let (ordinary_db, ordinary_file) = database(&source)?;
            let diagnostics = ordinary_db.check_file(ordinary_file);
            eprintln!("ordinary {target}, {entry}: {diagnostics:#?}");

            let (mut installed_db, installed_file) = database(&source)?;
            let (outcome, statistics) = expansion_probe::run_mro(&installed_db, 1_000_000, || {
                installed_db.check_file(installed_file)
            });
            eprintln!("installed {target}, {entry}: {outcome:#?}\nstatistics: {statistics:#?}");
            let events = installed_db.take_salsa_events();
            let reached = events
                .into_iter()
                .filter_map(|event| {
                    let salsa::EventKind::WillExecute { database_key } = event.kind else {
                        return None;
                    };
                    let name = installed_db.ingredient_debug_name(database_key.ingredient_index());
                    [
                        "constructor",
                        "initializer",
                        "instance_flags",
                        "try_mro",
                        "class_mro",
                        "lazy_bound",
                    ]
                    .iter()
                    .any(|needle| name.contains(*needle))
                    .then(|| name.into_owned())
                })
                .collect::<Vec<_>>();
            eprintln!("installed owner queries {target}, {entry}: {reached:#?}");
        }
    }
    Ok(())
}
