use std::panic::AssertUnwindSafe;

use ruff_python_ast::helpers::any_over_expr;
use ruff_text_size::Ranged;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::infer::builder::expression_search::{
    contains_string_literal_with, observations as scan_observations,
};

#[derive(Clone, Copy)]
enum Operation {
    Scan,
    Definition,
}

#[derive(Debug, PartialEq, Eq)]
enum Output<'db> {
    Scan(bool),
    Definition(&'db DefinitionInference<'db>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    operation: Operation,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Output<'db>>, AnalysisFailure> {
    let class_node = subject(prepared);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(class_node);
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            match operation {
                Operation::Scan => {
                    let effects = SourceEffects::new(&access, session.program());
                    contains_string_literal_with(class_node.bases(), &effects)
                        .await
                        .map(Output::Scan)
                }
                Operation::Definition => {
                    access.definition(definition).await.map(Output::Definition)
                }
            }
        })
    })
}

fn fixture(bases: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        format!("class Base: ...\nclass Child({bases}): ...\n"),
    )
    .unwrap();
    db
}

fn subject<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::StmtClassDef {
    let Some(Stmt::ClassDef(class)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture ends with a class definition");
    };
    class
}

fn growing_bases() -> String {
    let mut base = "\"Later\"".to_owned();
    for _ in 0..16 {
        base = format!("Base[({base}, Tail)]");
    }
    base
}

fn reset(cancel_after_growth: Option<usize>) {
    observations::reset(None);
    scan_observations::reset(cancel_after_growth);
}

fn assert_cleanup() {
    let progress = scan_observations::progress();
    assert_eq!(progress.live, 0);
    assert_eq!(progress.created, progress.retired);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_missing_definition(db: &TestDb, prepared: &PreparedAnalysisFile<'_>) {
    let definition = prepared
        .semantic_index()
        .expect_single_definition(subject(prepared));
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
}

fn completed_definition<'db>(
    prepared: &PreparedAnalysisFile<'db>,
) -> &'db DefinitionInference<'db> {
    let result = controlled(prepared, Operation::Definition, &funded());
    let Ok(AnalysisOutcome::Complete(Output::Definition(inference))) = result else {
        panic!("class definition: {result:?}");
    };
    assert_cleanup();
    inference
}

fn assert_deferred<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    inference: &DefinitionInference<'db>,
) {
    let class = subject(prepared);
    let definition = prepared.semantic_index().expect_single_definition(class);
    let Some(ClassLiteral::Static(literal)) = inference.original_class_type(definition) else {
        panic!("definition has a static class binding");
    };
    assert_eq!(literal.name(db).as_str(), "Child");
    let Some(DefinitionInferenceExtra::Deferred(deferred)) = inference.extra.as_deref() else {
        panic!("string-containing bases defer class inference");
    };
    assert_eq!(deferred.as_ref(), &[definition]);
    for base in class.bases() {
        assert_eq!(inference.try_expression_type(base), None);
    }
}

#[test]
fn source_scan_matches_ordinary_search_and_stops_at_the_first_string() {
    for (bases, expected, visits) in [
        ("", false, 0),
        ("Base[Leaf]", false, 3),
        ("Base[(Leaf, Other)]", false, 5),
        ("Base[\"Later\"], untouched()", true, 3),
        ("Base[(Leaf, \"Later\", untouched())], other()", true, 5),
    ] {
        let db = fixture(bases);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut ordinary_visits = Vec::new();
        let ordinary = subject(&prepared).bases().iter().any(|base| {
            any_over_expr(base, |expression| {
                ordinary_visits.push(expression.range());
                expression.is_string_literal_expr()
            })
        });
        assert_eq!(ordinary, expected, "{bases}");
        assert_eq!(ordinary_visits.len(), visits, "{bases}");
        reset(None);
        let result = capture(&db, || controlled(&prepared, Operation::Scan, &funded())).unwrap();
        assert_eq!(
            result.value,
            Ok(AnalysisOutcome::Complete(Output::Scan(expected))),
            "{bases}",
        );
        assert!(result.reads.is_empty());
        let progress = scan_observations::progress();
        assert_eq!(progress.visits, ordinary_visits);
        assert!(progress.before_growth.is_empty(), "{bases}");
        assert_cleanup();
        assert_missing_definition(&db, &prepared);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn canonical_class_definition_preserves_ordinary_base_inference_and_deferral() {
    for (bases, deferred) in [("Base", false), ("Base[\"Later\"], later_call()", true)] {
        let db = fixture(bases);
        let prepared = prepare(&db);
        let definition = prepared
            .semantic_index()
            .expect_single_definition(subject(&prepared));
        let revision = salsa::plumbing::current_revision(&db);
        assert_missing_definition(&db, &prepared);
        reset(None);
        let canonical = completed_definition(&prepared);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
        if deferred {
            assert_deferred(&db, &prepared, canonical);
        } else {
            assert!(canonical.extra.is_none());
            assert!(
                canonical
                    .try_expression_type(&subject(&prepared).bases()[0])
                    .is_some()
            );
        }

        let program_file = prepared.program_file();
        let env = ProgramEnvironment::from_file(program_file);
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            program_file.file(&db),
            program_file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(canonical, &ordinary);

        let ordinary_db = fixture(bases);
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_definition = ordinary_prepared
            .semantic_index()
            .expect_single_definition(subject(&ordinary_prepared));
        let independent = infer_definition_types(&ordinary_db, ordinary_definition);
        if deferred {
            assert_deferred(&ordinary_db, &ordinary_prepared, independent);
        } else {
            let ordinary_base =
                independent.expression_type(&subject(&ordinary_prepared).bases()[0]);
            let controlled_base = canonical.expression_type(&subject(&prepared).bases()[0]);
            let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
            assert_eq!(
                controlled_base.display(&db, &env).to_string(),
                ordinary_base
                    .display(&ordinary_db, &ordinary_env)
                    .to_string(),
            );
        }

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(None);
        assert_eq!(completed_definition(&prepared), canonical);
        assert_eq!(infer_definition_types(&db, definition), canonical);
        assert_eq!(scan_observations::progress().created, 0);
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn scan_growth_refusal_leaves_the_definition_unpublished_and_retries_at_the_same_revision() {
    let bases = growing_bases();
    let measured_db = fixture(&bases);
    let measured_prepared = prepare(&measured_db);
    reset(None);
    completed_definition(&measured_prepared);
    let measured = scan_observations::progress();
    let Some(growth) = measured.after_growth.get(1) else {
        panic!("nested bases require at least two scan stack growths");
    };
    assert_eq!(growth.ordinal, 2);
    assert!(growth.len > 0 && growth.capacity > 0);
    let work = funded().semantic_work_limit - growth.remaining.unwrap();

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture(&bases);
        let prepared = prepare(&db);
        reset(None);
        let outcome = controlled(
            &prepared,
            Operation::Definition,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        assert!(
            matches!(
                outcome,
                Ok(AnalysisOutcome::Complete(Output::Definition(_)))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        completed: (),
                    })
            ),
            "{outcome:?}",
        );
        if scan_observations::progress().after_growth.len() >= 2 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_cleanup();
    }
    assert!(work > 0 && upper > 0);

    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work - 1,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let db = fixture(&bases);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        assert_missing_definition(&db, &prepared);
        reset(None);
        assert_eq!(
            controlled(&prepared, Operation::Definition, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            }),
        );
        let progress = scan_observations::progress();
        assert_eq!(progress.before_growth.len(), 2);
        assert_eq!(progress.after_growth.len(), 1);
        assert!(progress.before_growth[1].len > 0);
        assert!(!progress.visits.is_empty());
        assert_eq!(progress.created, 1);
        assert_cleanup();
        assert_missing_definition(&db, &prepared);
        reset(None);
        let inference = completed_definition(&prepared);
        assert_deferred(&db, &prepared, inference);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                prepared
                    .semantic_index()
                    .expect_single_definition(subject(&prepared))
                    .as_id(),
            )
            .is_ok()
        );
    }
}

#[test]
fn native_cancellation_retires_the_scan_and_preserves_completed_definition_memos() {
    let bases = growing_bases();
    for operation in [Operation::Scan, Operation::Definition] {
        let db = fixture(&bases);
        let prepared = prepare(&db);
        let definition = prepared
            .semantic_index()
            .expect_single_definition(subject(&prepared));
        let revision = salsa::plumbing::current_revision(&db);
        assert_missing_definition(&db, &prepared);
        reset(Some(2));
        let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, operation, &funded())
        }));
        assert!(
            matches!(cancelled, Err(salsa::Cancelled::Local)),
            "{cancelled:?}"
        );
        let progress = scan_observations::progress();
        assert_eq!(progress.created, 1);
        assert!(progress.after_growth.len() >= 2);
        assert!(progress.after_growth[1].len > 0);
        assert_cleanup();
        match operation {
            Operation::Scan => assert_missing_definition(&db, &prepared),
            Operation::Definition => {
                // The claimed definition completes before Salsa delivers local cancellation.
                // Its final memo remains available even though the caller was cancelled.
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        definition_inference_ingredient(&db),
                        definition.as_id(),
                    )
                    .is_ok()
                );
            }
        }
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(None);
        match operation {
            Operation::Scan => assert_eq!(
                controlled(&prepared, operation, &funded()),
                Ok(AnalysisOutcome::Complete(Output::Scan(true))),
            ),
            Operation::Definition => {
                assert_deferred(&db, &prepared, completed_definition(&prepared));
                assert_eq!(scan_observations::progress().created, 0);
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    None,
                    &events_db.take_salsa_events(),
                );
            }
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}
