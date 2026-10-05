use std::io::{BufWriter, Write};
use std::time::Instant;

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;

use super::super::{Incomplete, run_selected};
use super::{LEGACY, PEP695, assert_expected, code};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};

const ALLOWANCE: usize = 1_000_000;
const SAMPLES: usize = 10;
const PATH: &str = "/src/forwarding.py";

fn database(source: &str) -> anyhow::Result<(TestDb, File)> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(PATH, source)?;
    let file = system_path_to_file(&db, PATH)?;
    db.clear_salsa_events();
    Ok((db, file))
}

fn validate(source: &str, outcome: Result<Vec<Diagnostic>, Incomplete>) -> anyhow::Result<String> {
    let diagnostics =
        outcome.map_err(|reason| anyhow::anyhow!("timing sample did not complete: {reason:?}"))?;
    assert_expected(source, &diagnostics);
    Ok(format!(
        "{:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.id().as_str(),
                diagnostic.headline_message().to_string(),
                diagnostic.primary_span().and_then(|span| span.range()),
            ))
            .collect::<Vec<_>>()
    ))
}

fn timed_check(
    db: &TestDb,
    file: File,
    source: &str,
    mro_effects: bool,
) -> anyhow::Result<(u128, String)> {
    let started = Instant::now();
    let result = run_selected(db, ALLOWANCE, false, mro_effects, || db.check_file(file));
    let elapsed = started.elapsed().as_nanos();
    let diagnostics = validate(source, result.0)?;
    Ok((elapsed, diagnostics))
}

#[test]
#[ignore]
fn mapping_start_timing() -> anyhow::Result<()> {
    let mut output = BufWriter::new(std::fs::File::create("/tmp/ty-mapping-start-timing.tsv")?);
    writeln!(output, "syntax\tcase\tmode\tphase\tsample\tns\tdiagnostics")?;
    for (syntax_name, syntax) in [("pep695", PEP695), ("legacy", LEGACY)] {
        for (case, heading) in [
            "Constructor stopping types introduced by forwarding",
            "Constructor stopping types supplied through inherited aliases",
            "Constructor stopping types changed by later forwarding",
            "Alternative constructor paths with different stopping types",
        ]
        .into_iter()
        .enumerate()
        {
            let source = code(syntax, heading)?;
            for (mode, mro_effects) in [("ordinary", false), ("installed", true)] {
                for sample in 0..SAMPLES {
                    let (db, file) = database(&source)?;
                    let (ns, diagnostics) = timed_check(&db, file, &source, mro_effects)?;
                    writeln!(
                        output,
                        "{syntax_name}\t{case}\t{mode}\tcold\t{sample}\t{ns}\t{diagnostics}"
                    )?;
                }

                for phase in ["unchanged-warm", "edited"] {
                    let (mut db, file) = database(&source)?;
                    let (seed, _) =
                        run_selected(&db, ALLOWANCE, false, mro_effects, || db.check_file(file));
                    validate(&source, seed)?;
                    db.clear_salsa_events();

                    for sample in 0..SAMPLES {
                        let edited;
                        let current_source = if phase == "edited" {
                            edited = format!("{source}\n\nrevision_marker = {sample}\n");
                            db.write_file(PATH, &edited)?;
                            &edited
                        } else {
                            &source
                        };
                        db.clear_salsa_events();
                        let (ns, diagnostics) =
                            timed_check(&db, file, current_source, mro_effects)?;
                        writeln!(
                            output,
                            "{syntax_name}\t{case}\t{mode}\t{phase}\t{sample}\t{ns}\t{diagnostics}"
                        )?;
                        db.clear_salsa_events();
                    }
                }
            }
        }
    }
    output.flush()?;
    Ok(())
}
