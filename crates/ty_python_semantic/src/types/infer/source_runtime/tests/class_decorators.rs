use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;

use super::*;
use crate::types::definition_expression_type;
use crate::types::function::KnownFunction;

thread_local! {
    static INSERTIONS: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static CLASS: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
}

pub(in crate::types::infer) fn observe_decorator_insert(
    db: &dyn Db,
    class: StaticClassLiteral<'_>,
) {
    let previous = INSERTIONS.get();
    INSERTIONS.set(previous + 1);
    if previous == 0 {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        ACTIVE.set(observations::counts().0);
        CLASS.set(Some(class.as_id()));
    }
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    INSERTIONS.set(0);
    REMAINING.set(None);
    ACTIVE.set(0);
    CLASS.set(None);
    CANCEL.set(cancel);
    observations::reset(None);
}

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        "from typing import final, type_check_only\n\
         @final\n\
         @type_check_only\n\
         class Leaf: ...\n\
         factory: type[Leaf]\n\
         left = right = factory\n",
    )
    .unwrap();
    db
}

fn prepare_fixture(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.pyi").unwrap();
    prepare_file(db, file).unwrap()
}

fn leaf<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::StmtClassDef {
    prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap()
}

fn completed_class<'db>(prepared: &PreparedAnalysisFile<'db>) -> StaticClassLiteral<'db> {
    let result = expression_type_with_policy(prepared, expression_key(prepared), &funded());
    let Ok(AnalysisOutcome::Complete(Type::ClassLiteral(ClassLiteral::Static(class)))) = result
    else {
        panic!("{result:?}");
    };
    class
}

fn canonical_decorators<'db>(db: &'db TestDb, class: StaticClassLiteral<'db>) -> &'db [Type<'db>] {
    let ingredient = class_decorators_ingredient(db);
    assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, class.as_id()).is_ok());
    salsa::attach(db, || {
        ingredient
            .fetch(db as &dyn Db, db.zalsa(), db.zalsa_local(), class.as_id())
            .as_ref()
    })
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn decorators_executed(db: &dyn Db, class: salsa::Id, events: &[salsa::Event]) -> bool {
    let key = class_decorators_ingredient(db).database_key_index(class);
    events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    })
}

#[test]
fn cold_finality_completes_all_decorators_and_reuses_the_canonical_slice() {
    let db = fixture();
    let prepared = prepare_fixture(&db);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(leaf(&prepared));
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let class = completed_class(&prepared);
    assert_eq!(CLASS.get(), Some(class.as_id()));
    assert_eq!(INSERTIONS.get(), 2);
    assert!(ACTIVE.get() > 0);
    assert_cleanup();
    let events = events_db.take_salsa_events();
    assert!(decorators_executed(&db, class.as_id(), &events));
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events,
        )
        .is_some()
    );

    let decorators = canonical_decorators(&db, class);
    let expected = leaf(&prepared)
        .decorator_list
        .iter()
        .map(|decorator| definition_expression_type(&db, definition, &decorator.expression))
        .collect::<Vec<_>>();
    assert_eq!(decorators, expected);
    assert_eq!(
        decorators
            .iter()
            .map(|ty| ty
                .as_function_literal()
                .and_then(|function| function.known(&db)))
            .collect::<Vec<_>>(),
        [
            Some(KnownFunction::Final),
            Some(KnownFunction::TypeCheckOnly)
        ],
    );
    assert_eq!(
        infer_definition_types(&db, definition).original_class_type(definition),
        Some(ClassLiteral::Static(class)),
    );
    assert!(class.is_final(&db));
    assert_eq!(completed_class(&prepared), class);
    let events = events_db.take_salsa_events();
    assert!(!decorators_executed(&db, class.as_id(), &events));
    for query in [
        "infer_definition_types",
        "infer_deferred_types",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(INSERTIONS.get(), 2);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();

    let ordinary_db = fixture();
    let ordinary_prepared = prepare_fixture(&ordinary_db);
    let expression = ordinary_prepared
        .semantic_index()
        .expression(expression_key(&ordinary_prepared));
    let ordinary = infer_expression_types(&ordinary_db, expression, TypeContext::default());
    let Type::ClassLiteral(ClassLiteral::Static(ordinary_class)) =
        ordinary.expression_type(expression_key(&ordinary_prepared))
    else {
        panic!("ordinary final class annotation");
    };
    assert_eq!(ordinary_class.name(&ordinary_db).as_str(), "Leaf");
    assert_eq!(
        ordinary_class
            .known_function_decorators(&ordinary_db)
            .collect::<Vec<_>>(),
        [KnownFunction::Final, KnownFunction::TypeCheckOnly],
    );
}

#[test]
fn interrupted_decorator_slice_drains_owners_and_retries_with_completed_children() {
    let measured = fixture();
    let measured_prepared = prepare_fixture(&measured);
    reset(false);
    completed_class(&measured_prepared);
    assert_eq!(INSERTIONS.get(), 2);
    let work = funded().semantic_work_limit - REMAINING.get().unwrap();

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare_fixture(&db);
        let definition = prepared
            .semantic_index()
            .expect_single_definition(leaf(&prepared));
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert!(INSERTIONS.get() > 0);
        assert!(ACTIVE.get() > 0);
        assert_cleanup();
        let class_id = CLASS.get().unwrap();
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
        if !cancel {
            assert_eq!(INSERTIONS.get(), 1);
            assert_eq!(REMAINING.get(), Some(0));
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    class_decorators_ingredient(&db),
                    class_id,
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
        }
        events_db.take_salsa_events();
        reset(false);
        let class = completed_class(&prepared);
        assert_eq!(class.as_id(), class_id);
        assert_eq!(canonical_decorators(&db, class).len(), 2);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events,
        );
        if !cancel {
            assert!(decorators_executed(&db, class_id, &events));
            assert_eq!(INSERTIONS.get(), 2);
        }
    }
}
