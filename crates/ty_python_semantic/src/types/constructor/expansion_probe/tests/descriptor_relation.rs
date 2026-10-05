use std::io::Write;

use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithWritableSystem, OsSystem, System, SystemPath, WritableSystem};
use ruff_python_ast::PythonVersion;

use super::super::{charge_ledger, descriptor_observation, run_mro_observed};
use super::{LEGACY, PEP695, assert_expected, code};
use crate::Db;
use crate::db::tests::TestDbBuilder;

fn source(syntax: &str, case: &str) -> anyhow::Result<String> {
    if case == "overload" {
        return code(
            if syntax == "legacy" { LEGACY } else { PEP695 },
            "Recursive protocol methods in rejected descriptor overloads",
        );
    }
    let child = if case == "finite" {
        "object"
    } else {
        "P[list[T]]"
    };
    let (imports, protocol, class) = if syntax == "legacy" {
        (
            "from typing import Generic, TypeVar\nT = TypeVar('T')\n",
            "P(Protocol[T])",
            "C(Generic[T])",
        )
    } else {
        ("", "P[T](Protocol)", "C[T]")
    };
    Ok(format!(
        "from __future__ import annotations\nfrom collections.abc import Callable\nfrom typing import Protocol\n{imports}\nclass {protocol}:\n    child: {child}\n\nclass Descriptor:\n    def __get__(self, instance: P[int], owner: object) -> Callable[[], None]:\n        raise NotImplementedError\n\nclass {class}:\n    child: C[list[T]]\n    __init__: Descriptor\n\nC[int]()\n"
    ))
}

#[test]
#[ignore]
fn observe_descriptor_relation() -> anyhow::Result<()> {
    let system = OsSystem::new("/tmp");
    let syntax = system.env_var("TY_DESCRIPTOR_SYNTAX")?;
    let case = system.env_var("TY_DESCRIPTOR_CASE")?;
    let mode = system.env_var("TY_DESCRIPTOR_MODE")?;
    let allowance: usize = system.env_var("TY_DESCRIPTOR_ALLOWANCE")?.parse()?;
    let output = system.env_var("TY_DESCRIPTOR_OUTPUT")?;
    anyhow::ensure!(matches!(syntax.as_str(), "legacy" | "pep695"));
    anyhow::ensure!(matches!(case.as_str(), "finite" | "overload" | "growing"));
    anyhow::ensure!(matches!(mode.as_str(), "ordinary" | "installed"));
    let source = source(&syntax, &case)?;
    system.write_file(SystemPath::new(&format!("{output}.py")), &source)?;
    let mut result = std::fs::File::create(format!("{output}.result"))?;
    writeln!(
        result,
        "syntax={syntax} case={case} mode={mode} allowance={allowance}"
    )?;
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/descriptor.py", &source)?;
    let file = system_path_to_file(&db, "/src/descriptor.py")?;
    let _trace = if system.env_var("TY_DESCRIPTOR_TRACE")?.as_str() == "1" {
        Some(descriptor_observation::install(&format!("{output}.trace"))?)
    } else {
        None
    };
    let diagnostics = if mode == "ordinary" {
        Some(db.check_file(file))
    } else {
        let ((outcome, statistics), ledger) = charge_ledger::capture(true, || {
            run_mro_observed(&db, allowance, || db.check_file(file))
        });
        writeln!(result, "outcome={:?}", outcome.as_ref().map(|_| ()))?;
        writeln!(result, "statistics={statistics:#?}")?;
        writeln!(result, "ledger={ledger:#?}")?;
        outcome.ok()
    };
    if let Some(diagnostics) = diagnostics {
        writeln!(
            result,
            "diagnostics={:#?}",
            diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.id().as_str(),
                    diagnostic.headline_message().to_string(),
                    diagnostic.primary_span().and_then(|span| span.range()),
                ))
                .collect::<Vec<_>>()
        )?;
        if case == "overload" {
            assert_expected(&source, &diagnostics);
        }
    }
    Ok(())
}
