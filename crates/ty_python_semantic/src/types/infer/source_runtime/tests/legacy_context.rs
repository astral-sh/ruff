use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::{KnownInstanceType, TypeVarVariance};

thread_local! {
    static INSERTIONS: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
}

pub(in crate::types::infer) fn observe_context_insert(db: &dyn Db) {
    let previous = INSERTIONS.get();
    INSERTIONS.set(previous + 1);
    if previous == 0 {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        ACTIVE.set(observations::counts().0);
    }
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    INSERTIONS.set(0);
    REMAINING.set(None);
    ACTIVE.set(0);
    CANCEL.set(cancel);
    observations::reset(None);
}

fn context_fixture(base: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        format!(
            "from typing import Generic, Protocol, TypeVar\n\
             T = TypeVar(\"T\", covariant=True)\n\
             U = TypeVar(\"U\", contravariant=True)\n\
             class Product({base}[T, U]): ...\n"
        ),
    )
    .unwrap();
    db
}

fn prepare_context(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.pyi").unwrap();
    prepare_file(db, file).unwrap()
}

fn completed_context<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: Option<&str>,
) -> (StaticClassLiteral<'db>, GenericContext<'db>) {
    completed_context_with_attempts(db, prepared, name).0
}

fn completed_context_with_attempts<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: Option<&str>,
) -> ((StaticClassLiteral<'db>, GenericContext<'db>), usize) {
    let revision = salsa::plumbing::current_revision(db);
    for preceding_work_limit_attempts in 0..4 {
        reset(false);
        let result = controlled_class_context_for(prepared, name, None, &funded());
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(db), revision);
        match result {
            Ok(AnalysisOutcome::Complete((class, Some(context)))) => {
                return ((class, context), preceding_work_limit_attempts);
            }
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }) => {}
            other => panic!("{other:?}"),
        }
    }
    panic!("class context did not complete within four funded caller attempts");
}

fn shape<'db>(
    db: &'db TestDb,
    context: GenericContext<'db>,
) -> Vec<(String, Option<TypeVarVariance>)> {
    context
        .variables(db)
        .map(|variable| {
            (
                variable.name(db).to_string(),
                variable.typevar(db).explicit_variance(db),
            )
        })
        .collect()
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_legacy_contexts_publish_ordered_bound_variables_and_reuse_canonical_results() {
    for base in ["Generic", "Protocol"] {
        let db = context_fixture(base);
        let prepared = prepare_context(&db);
        let mut events_db = db.clone();
        let revision = salsa::plumbing::current_revision(&db);
        events_db.take_salsa_events();
        reset(false);
        let (class, context) = completed_context(&db, &prepared, None);
        assert_eq!(
            shape(&db, context),
            vec![
                ("T".to_owned(), Some(TypeVarVariance::Covariant)),
                ("U".to_owned(), Some(TypeVarVariance::Contravariant)),
            ],
        );
        for variable in context.variables(&db) {
            assert_eq!(
                variable.binding_context(&db).definition(),
                Some(class.definition(&db))
            );
        }
        assert_cleanup();
        let events = events_db.take_salsa_events();
        for query in [
            "infer_definition_types",
            "static_class_generic_context",
            "explicit_bases_inner",
            "infer_deferred_types",
        ] {
            assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
        }
        assert!(INSERTIONS.get() >= 2);

        let definition = class.definition(&db);
        let base_type = Type::KnownInstance(if base == "Generic" {
            KnownInstanceType::SubscriptedGeneric(context)
        } else {
            KnownInstanceType::SubscriptedProtocol(context)
        });
        assert_eq!(class.explicit_bases(&db).as_ref(), &[base_type]);
        assert_eq!(
            infer_definition_types(&db, definition).original_class_type(definition),
            Some(ClassLiteral::Static(class))
        );
        let Some(Stmt::ClassDef(node)) = prepared.parsed_module().syntax().body.last() else {
            panic!("fixture class");
        };
        assert_eq!(
            crate::types::infer::infer_deferred_types(&db, definition)
                .try_expression_type(&node.bases()[0]),
            Some(base_type)
        );
        for variable in context.variables(&db) {
            let typevar = variable.typevar(&db);
            let definition = typevar.definition(&db).unwrap();
            assert_eq!(
                infer_definition_types(&db, definition).completed_binding(definition),
                Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar)))
            );
        }
        assert_eq!(class.generic_context(&db), Some(context));
        assert_eq!(completed_context(&db, &prepared, None), (class, context));
        let events = events_db.take_salsa_events();
        for query in [
            "infer_definition_types",
            "static_class_generic_context",
            "explicit_bases_inner",
            "infer_deferred_types",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let ordinary = context_fixture(base);
        let ordinary_prepared = prepare_context(&ordinary);
        let Some(Stmt::ClassDef(node)) = ordinary_prepared.parsed_module().syntax().body.last()
        else {
            panic!("fixture class");
        };
        let definition = ordinary_prepared
            .semantic_index()
            .expect_single_definition(node);
        let Some(ClassLiteral::Static(ordinary_class)) =
            infer_definition_types(&ordinary, definition).original_class_type(definition)
        else {
            panic!("ordinary class definition");
        };
        let Some(ordinary_context) = ordinary_class.generic_context(&ordinary) else {
            panic!("ordinary generic context");
        };
        assert_eq!(shape(&ordinary, ordinary_context), shape(&db, context));
    }
}

#[test]
fn cold_inherited_context_preserves_ordered_bound_variables_and_reuses_canonical_results()
-> anyhow::Result<()> {
    let fixture = || -> anyhow::Result<TestDb> {
        let mut db = setup_db();
        db.write_file(
            "src/main.pyi",
            "from typing import Generic, TypeVar\n\
             T = TypeVar(\"T\")\n\
             U = TypeVar(\"U\")\n\
             V = TypeVar(\"V\")\n\
             class First(Generic[T, U]): ...\n\
             class Second(Generic[T, U]): ...\n\
             class Child(First[U, T], Second[T, V]): ...\n",
        )?;
        Ok(db)
    };
    let db = fixture()?;
    let prepared = prepare_context(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let (child, context) = completed_context(&db, &prepared, Some("Child"));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            inherited_class_context_ingredient(&db),
            child.as_id(),
        )
        .is_ok()
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            static_class_generic_context_ingredient(&db),
            child.as_id(),
        )
        .is_ok()
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            explicit_bases_ingredient(&db),
            child.as_id(),
        )
        .is_ok()
    );
    let events = events_db.take_salsa_events();
    for query in [
        "inherited_legacy_generic_context_inner",
        "static_class_generic_context",
        "explicit_bases_inner",
    ] {
        assert!(
            find_will_execute_event_by_name(&db, query, Some(child.as_id()), &events).is_some()
        );
    }
    assert_eq!(
        shape(&db, context),
        vec![
            ("U".to_owned(), Some(TypeVarVariance::Invariant)),
            ("T".to_owned(), Some(TypeVarVariance::Invariant)),
            ("V".to_owned(), Some(TypeVarVariance::Invariant)),
        ],
    );

    let ordinary = fixture()?;
    let ordinary_prepared = prepare_context(&ordinary);
    let Some(Stmt::ClassDef(node)) = ordinary_prepared.parsed_module().syntax().body.last() else {
        panic!("fixture class");
    };
    let definition = ordinary_prepared
        .semantic_index()
        .expect_single_definition(node);
    let Some(ClassLiteral::Static(ordinary_child)) =
        infer_definition_types(&ordinary, definition).original_class_type(definition)
    else {
        panic!("ordinary class definition");
    };
    let Some(ordinary_context) = ordinary_child.generic_context(&ordinary) else {
        panic!("ordinary generic context");
    };
    assert_eq!(shape(&ordinary, ordinary_context), shape(&db, context));
    for (db, child, context) in [
        (&db, child, context),
        (&ordinary, ordinary_child, ordinary_context),
    ] {
        assert_eq!(child.generic_context(db), Some(context));
        assert_eq!(child.inherited_legacy_generic_context(db), Some(context));
        let variables: Vec<_> = context.variables(db).collect();
        let [u, t, v] = variables.as_slice() else {
            panic!("Child has three distinct generic parameters");
        };
        for variable in &variables {
            assert_eq!(
                variable.binding_context(db).definition(),
                Some(child.definition(db))
            );
        }
        let bases = child.explicit_bases(db);
        let [Type::GenericAlias(first), Type::GenericAlias(second)] = bases.as_ref() else {
            panic!("Child has two specialized bases: {bases:?}");
        };
        for (name, parent, arguments) in [
            ("First", first, [Type::TypeVar(*u), Type::TypeVar(*t)]),
            ("Second", second, [Type::TypeVar(*t), Type::TypeVar(*v)]),
        ] {
            assert_eq!(parent.origin(db).name(db), name);
            assert_eq!(parent.specialization(db).types(db), &arguments);
            let formals: Vec<_> = parent
                .specialization(db)
                .generic_context(db)
                .variables(db)
                .collect();
            let [formal_t, formal_u] = formals.as_slice() else {
                panic!("each parent has two generic parameters");
            };
            for (formal, child_variable) in [(formal_t, t), (formal_u, u)] {
                assert_eq!(formal.typevar(db), child_variable.typevar(db));
                assert_ne!(formal.identity(db), child_variable.identity(db));
                assert_eq!(
                    formal.binding_context(db).definition(),
                    Some(parent.origin(db).definition(db))
                );
            }
        }
    }

    assert_eq!(
        completed_context(&db, &prepared, Some("Child")),
        (child, context)
    );
    let events = events_db.take_salsa_events();
    for query in [
        "inherited_legacy_generic_context_inner",
        "static_class_generic_context",
        "explicit_bases_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, Some(child.as_id()), &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
    Ok(())
}

#[test]
fn one_declaration_has_distinct_bound_occurrences_in_two_classes() {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        "from typing import Generic, TypeVar\n\
         T = TypeVar(\"T\")\n\
         class First(Generic[T]): ...\n\
         class Second(Generic[T]): ...\n",
    )
    .unwrap();
    let prepared = prepare_context(&db);
    reset(false);
    let (first, first_context) = completed_context(&db, &prepared, Some("First"));
    let (second, second_context) = completed_context(&db, &prepared, Some("Second"));
    let first_var = first_context.variables(&db).next().unwrap();
    let second_var = second_context.variables(&db).next().unwrap();
    assert_eq!(first_var.typevar(&db), second_var.typevar(&db));
    assert_ne!(first_var.identity(&db), second_var.identity(&db));
    assert_eq!(
        first_var.binding_context(&db).definition(),
        Some(first.definition(&db))
    );
    assert_eq!(
        second_var.binding_context(&db).definition(),
        Some(second.definition(&db))
    );
    assert_cleanup();
}

#[test]
fn cold_headers_record_lazy_bounds_and_defaults_without_evaluating_them() {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "from typing import TypeVar\n\
         T = TypeVar(\"T\", bound=\"Later\", default=\"Later\")\n\
         class Later: ...\n\
         left = right = T\n",
    )
    .unwrap();
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let Ok(AnalysisOutcome::Complete(Type::KnownInstance(KnownInstanceType::TypeVar(variable)))) =
        result
    else {
        panic!("{result:?}");
    };
    assert_eq!(
        variable.eager_bounds_with_fields(salsa::FieldReads::new(&db)),
        (None, true)
    );
    assert_eq!(
        variable.eager_default_with_fields(salsa::FieldReads::new(&db)),
        (None, true)
    );
    let events = events_db.take_salsa_events();
    for query in [
        "lazy_bound_unchecked",
        "lazy_default_unchecked",
        "bound_typevar_default_type",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_cleanup();
}

#[test]
fn interrupted_partial_context_cleans_up_and_retries_at_the_same_revision() {
    let measured = context_fixture("Generic");
    let measured_prepared = prepare_context(&measured);
    reset(false);
    let (_, preceding_work_limit_attempts) =
        completed_context_with_attempts(&measured, &measured_prepared, None);
    let Some(remaining) = REMAINING.get() else {
        panic!("context did not insert a variable");
    };
    let first_insertion_work = funded().semantic_work_limit - remaining;

    for cancel in [false, true] {
        let db = context_fixture("Generic");
        let prepared = prepare_context(&db);
        let mut events_db = db.clone();
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..preceding_work_limit_attempts {
            reset(false);
            let result = controlled_class_context_for(&prepared, None, None, &funded());
            assert_cleanup();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            );
        }
        events_db.take_salsa_events();
        reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: first_insertion_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_class_context_for(&prepared, None, None, &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                }),
            ),
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert!(INSERTIONS.get() > 0);
        assert!(ACTIVE.get() > 0);
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        let events = events_db.take_salsa_events();
        if !cancel {
            assert_eq!(INSERTIONS.get(), 1);
            assert_eq!(REMAINING.get(), Some(0));
            let event =
                find_will_execute_event_by_name(&db, "static_class_generic_context", None, &events)
                    .expect("context query started");
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                panic!("context execution event");
            };
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    static_class_generic_context_ingredient(&db),
                    database_key.key_index()
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
        }
        reset(false);
        let (_, context) = completed_context(&db, &prepared, None);
        assert_eq!(context.variables(&db).len(), 2);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn cold_string_literals_preserve_concatenation_and_byte_length_limits() {
    for (expression, value) in [
        ("''".to_owned(), String::new()),
        ("'Type' 'Var'".to_owned(), "TypeVar".to_owned()),
        (format!("'{}'", "é".repeat(2048)), "é".repeat(2048)),
        (format!("'{}'", "é".repeat(2049)), "é".repeat(2049)),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", format!("left = right = {expression}\n"))
            .unwrap();
        let prepared = prepare(&db);
        reset(false);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(ty)) = result else {
            panic!("{result:?}");
        };
        if value.len() <= 4096 {
            assert_eq!(
                ty.as_string_literal().map(|literal| literal.value(&db)),
                Some(value.as_str())
            );
        } else {
            assert_eq!(ty, Type::literal_string());
        }
        assert_cleanup();
    }
}
