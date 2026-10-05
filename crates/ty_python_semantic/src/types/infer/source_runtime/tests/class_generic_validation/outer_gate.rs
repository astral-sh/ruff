//! Disabling the outer generic-class lint skips both class-generic scans.

use super::*;
use crate::types::infer::infer_scope_types;

/// Disabling `invalid-generic-class` skips default-reference and own-variable-shadowing
/// scans even while `shadowed-type-variable` remains enabled. The nested class contains
/// both violations, but cold inference of `Outer`'s body completes without scan or report
/// observations and publishes its canonical scope memo. Same-revision retry reuses that memo;
/// ordinary checking in a fresh database produces the same scope diagnostics and neither
/// selected diagnostic at `Target`'s class header.
#[test]
fn disabled_outer_lint_skips_both_class_generic_scans() -> anyhow::Result<()> {
    let source = "class Outer[T]:\n    class Target[T = U, U = int]: ...\n";
    let registry = crate::default_lint_registry();
    let mut rules = RuleSelection::from_registry(registry);
    rules.disable(registry.get("invalid-generic-class")?);
    assert!(rules.is_enabled(registry.get("shadowed-type-variable")?));
    let db = TestDbBuilder::new()
        .with_rule_selection(rules.clone())
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Outer", "Target"])?;
    let scope_key = InferScope::Bare(scope).as_id();
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
            .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );

    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let result = controlled_scope(&prepared, scope, &funded());
    let events = recording.take_events();
    drop(recording);
    assert!(events.is_empty(), "{result:?}\n{events:?}");
    let Ok(AnalysisOutcome::Complete(canonical)) = result else {
        anyhow::bail!("cold outer-gate scope did not complete: {result:?}");
    };
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
            .is_ok(),
    );

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let retry = controlled_scope(&prepared, scope, &funded());
    let events = recording.take_events();
    drop(recording);
    assert!(events.is_empty(), "{retry:?}\n{events:?}");
    let Ok(AnalysisOutcome::Complete(warm)) = retry else {
        anyhow::bail!("outer-gate scope retry did not complete: {retry:?}");
    };
    assert!(std::ptr::eq(canonical, warm));
    assert!(std::ptr::eq(
        canonical,
        infer_scope_types(&db, scope, TypeContext::default()),
    ));
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_scope_types_impl",
        Some(scope_key),
        &events_db.take_salsa_events(),
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
            .is_ok(),
    );

    let ordinary_db = TestDbBuilder::new()
        .with_rule_selection(rules)
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let ordinary_prepared = prepare(&ordinary_db);
    let (ordinary_scope, target_range) =
        fixture_target(&ordinary_db, &ordinary_prepared, &["Outer", "Target"])?;
    let ordinary = infer_scope_types(&ordinary_db, ordinary_scope, TypeContext::default());
    assert_eq!(canonical.diagnostics(), ordinary.diagnostics());
    assert!(canonical.diagnostics().is_none());
    let reports = crate::check_file_unwrap(&ordinary_db, ordinary_prepared.program_file())
        .into_iter()
        .filter(|diagnostic| {
            [
                DiagnosticId::lint("invalid-generic-class"),
                DiagnosticId::lint("shadowed-type-variable"),
            ]
            .contains(&diagnostic.id())
                && diagnostic.primary_span().and_then(|span| span.range()) == Some(target_range)
        })
        .collect::<Vec<_>>();
    assert!(reports.is_empty(), "{reports:?}");
    Ok(())
}
