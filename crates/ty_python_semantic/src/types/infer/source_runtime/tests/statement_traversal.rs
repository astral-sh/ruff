use std::cell::RefCell;
use std::panic::AssertUnwindSafe;

use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FinalSourceMemo;

use super::*;

const SOURCE: &str = "if False:\n    11\n    if True:\n        12\n    elif False:\n        13\n    else:\n        14\n    15\nelif True:\n    21\nelse:\n    31\n41\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Statement(TextRange),
    ConditionEntered(TextRange),
    ConditionCompleted(TextRange),
    SuitePostcheck(Option<TextRange>),
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    events: Vec<Event>,
    conditions: Vec<(TextRange, usize)>,
    live: usize,
    created: usize,
    dropped: usize,
    max_frames: usize,
    builders_at_drop: Vec<usize>,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

struct Recording;

impl Recording {
    fn start() -> Self {
        assert!(!ENABLED.replace(true));
        JOURNAL.with_borrow_mut(|journal| {
            assert_eq!(journal.live, 0);
            *journal = Snapshot::default();
        });
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ENABLED.set(false);
    }
}

pub(in crate::types::infer) struct TraversalGuard(bool);

impl TraversalGuard {
    pub(in crate::types::infer) fn new(_db: &dyn Db) -> Self {
        let enabled = ENABLED.get();
        if enabled {
            JOURNAL.with_borrow_mut(|journal| {
                journal.live += 1;
                journal.created += 1;
            });
        }
        Self(enabled)
    }
}

impl Drop for TraversalGuard {
    fn drop(&mut self) {
        if self.0 {
            JOURNAL.with_borrow_mut(|journal| {
                journal.live -= 1;
                journal.dropped += 1;
                journal.builders_at_drop.push(observations::counts().0);
            });
        }
    }
}

fn observe(event: Event) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.events.push(event));
    }
}

pub(in crate::types::infer) fn frame_depth(_db: &dyn Db, depth: usize) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.max_frames = journal.max_frames.max(depth));
    }
}

pub(in crate::types::infer) fn statement_entered(_db: &dyn Db, range: TextRange) {
    observe(Event::Statement(range));
}

pub(in crate::types::infer) fn condition_entered(_db: &dyn Db, range: TextRange) {
    observe(Event::ConditionEntered(range));
}

pub(in crate::types::infer) fn condition_completed(db: &dyn Db, range: TextRange) {
    if ENABLED.get() {
        let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
        JOURNAL.with_borrow_mut(|journal| {
            journal.events.push(Event::ConditionCompleted(range));
            journal.conditions.push((range, remaining));
        });
    }
}

pub(in crate::types::infer) fn suite_postcheck(_db: &dyn Db, suite: &[ast::Stmt]) {
    observe(Event::SuitePostcheck(suite.first().map(Ranged::range)));
}

fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

fn database(stub: bool) -> TestDb {
    let mut db = setup_db();
    db.write_file(if stub { "src/main.pyi" } else { "src/main.py" }, SOURCE)
        .unwrap();
    db
}

fn prepared(db: &TestDb, stub: bool) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, if stub { "src/main.pyi" } else { "src/main.py" }).unwrap();
    prepare_file(db, file).unwrap()
}

fn expected_order(prepared: &PreparedAnalysisFile<'_>, complete: bool) -> Vec<Event> {
    let module = &prepared.parsed_module().syntax().body;
    let Stmt::If(outer) = &module[0] else {
        panic!("outer if")
    };
    let Stmt::If(inner) = &outer.body[1] else {
        panic!("inner if")
    };
    let inner_elif = &inner.elif_else_clauses[0];
    let inner_else = &inner.elif_else_clauses[1];
    let outer_elif = &outer.elif_else_clauses[0];
    let outer_else = &outer.elif_else_clauses[1];
    let mut events = vec![
        Event::Statement(outer.range()),
        Event::ConditionEntered(outer.test.range()),
        Event::ConditionCompleted(outer.test.range()),
        Event::Statement(outer.body[0].range()),
        Event::Statement(inner.range()),
        Event::ConditionEntered(inner.test.range()),
        Event::ConditionCompleted(inner.test.range()),
        Event::Statement(inner.body[0].range()),
        Event::SuitePostcheck(Some(inner.body[0].range())),
        Event::ConditionEntered(inner_elif.test.as_ref().unwrap().range()),
        Event::ConditionCompleted(inner_elif.test.as_ref().unwrap().range()),
        Event::Statement(inner_elif.body[0].range()),
        Event::SuitePostcheck(Some(inner_elif.body[0].range())),
        Event::Statement(inner_else.body[0].range()),
        Event::SuitePostcheck(Some(inner_else.body[0].range())),
        Event::Statement(outer.body[2].range()),
        Event::SuitePostcheck(Some(outer.body[0].range())),
    ];
    if complete {
        events.extend([
            Event::ConditionEntered(outer_elif.test.as_ref().unwrap().range()),
            Event::ConditionCompleted(outer_elif.test.as_ref().unwrap().range()),
            Event::Statement(outer_elif.body[0].range()),
            Event::SuitePostcheck(Some(outer_elif.body[0].range())),
            Event::Statement(outer_else.body[0].range()),
            Event::SuitePostcheck(Some(outer_else.body[0].range())),
            Event::Statement(module[1].range()),
            Event::SuitePostcheck(Some(outer.range())),
        ]);
    }
    events
}

fn expression_nodes<'a>(suite: &'a [Stmt], nodes: &mut Vec<&'a ast::Expr>) {
    for statement in suite {
        match statement {
            Stmt::If(statement) => {
                nodes.push(&statement.test);
                expression_nodes(&statement.body, nodes);
                for clause in &statement.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        nodes.push(test);
                    }
                    expression_nodes(&clause.body, nodes);
                }
            }
            Stmt::Expr(statement) => nodes.push(&statement.value),
            _ => panic!("fixture contains only if and expression statements"),
        }
    }
}

fn condition_nodes<'a>(prepared: &'a PreparedAnalysisFile<'_>) -> [&'a ast::Expr; 4] {
    let Stmt::If(outer) = &prepared.parsed_module().syntax().body[0] else {
        panic!("outer if")
    };
    let Stmt::If(inner) = &outer.body[1] else {
        panic!("inner if")
    };
    [
        &outer.test,
        &inner.test,
        inner.elif_else_clauses[0].test.as_ref().unwrap(),
        outer.elif_else_clauses[0].test.as_ref().unwrap(),
    ]
}

fn assert_cleanup() {
    let journal = snapshot();
    assert_eq!(journal.live, 0);
    assert_eq!(journal.created, journal.dropped);
    assert!(journal.builders_at_drop.iter().all(|&live| live > 0));
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_stub_nested_if_traversal_infers_every_body_and_reuses_canonical_results() {
    let db = database(true);
    let prepared = prepared(&db, true);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    let cold = capture(&db, || check_file_with_policy(&prepared, &funded())).unwrap();
    drop(recording);
    assert!(
        matches!(&cold.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
        "{:?}",
        cold.value
    );
    cold.check_root_reads().unwrap();
    assert_eq!(snapshot().events, expected_order(&prepared, true));
    assert!(snapshot().max_frames > 1);
    assert_cleanup();

    let scope = ty_python_core::global_scope(&db, prepared.program_file());
    let scope_key = scope_inference_ingredient(&db).database_key_index(scope.as_id());
    let cold_scope = cold
        .reads
        .iter()
        .find(|read| read.key == scope_key)
        .unwrap();
    let mut reader = db.clone();
    reader.take_salsa_events();
    let canonical = super::super::super::infer_scope_types(&db, scope, TypeContext::default());
    let mut nodes = Vec::new();
    expression_nodes(&prepared.parsed_module().syntax().body, &mut nodes);
    let types: Vec<_> = nodes
        .iter()
        .map(|node| canonical.expression_type(*node))
        .collect();
    assert_eq!(
        types,
        [
            Type::bool_literal(false),
            Type::int_literal(11),
            Type::bool_literal(true),
            Type::int_literal(12),
            Type::bool_literal(false),
            Type::int_literal(13),
            Type::int_literal(14),
            Type::int_literal(15),
            Type::bool_literal(true),
            Type::int_literal(21),
            Type::int_literal(31),
            Type::int_literal(41),
        ]
    );
    for node in condition_nodes(&prepared) {
        let expression = prepared.semantic_index().expression(node);
        let ingredient = expression_inference_ingredient(&db);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, expression.as_id()).is_ok());
        let key = ingredient.database_key_index(expression.as_id());
        let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
        let native = capture(&db, || {
            infer_expression_types(&db, expression, TypeContext::default())
        })
        .unwrap();
        assert_eq!(
            native.value.expression_type(node),
            canonical.expression_type(node)
        );
        assert!(
            native
                .reads
                .iter()
                .any(|read| read.key == key && read.memo_address == cold_read.memo_address)
        );
    }

    let ordinary_db = database(true);
    let ordinary_prepared = self::prepared(&ordinary_db, true);
    let ordinary_scope =
        ty_python_core::global_scope(&ordinary_db, ordinary_prepared.program_file());
    let ordinary = super::super::super::infer_scope_types(
        &ordinary_db,
        ordinary_scope,
        TypeContext::default(),
    );
    let mut ordinary_nodes = Vec::new();
    expression_nodes(
        &ordinary_prepared.parsed_module().syntax().body,
        &mut ordinary_nodes,
    );
    assert_eq!(
        types,
        ordinary_nodes
            .iter()
            .map(|node| ordinary.expression_type(*node))
            .collect::<Vec<_>>()
    );

    observations::reset(None);
    let recording = Recording::start();
    let warm = capture(&db, || check_file_with_policy(&prepared, &funded())).unwrap();
    drop(recording);
    assert!(
        matches!(&warm.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
    );
    warm.check_root_reads().unwrap();
    assert!(warm.reads.iter().any(|read| read.key == scope_key
        && read.memo_address == cold_scope.memo_address
        && read.stamp == cold_scope.stamp));
    assert!(snapshot().events.is_empty());
    let events = reader.take_salsa_events();
    for query in ["infer_scope_types_impl", "infer_expression_types_impl"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn python_nested_suite_postcheck_refuses_before_enclosing_clauses_resume() {
    let db = database(false);
    let prepared = prepared(&db, false);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = check_file_with_policy(&prepared, &funded());
    drop(recording);
    assert_eq!(result, Ok(unavailable(OperationId::SuiteRedundantIf)));
    assert_eq!(snapshot().events, expected_order(&prepared, false));
    assert_cleanup();
    let conditions = condition_nodes(&prepared);
    let ingredient = expression_inference_ingredient(&db);
    for condition in &conditions[..3] {
        let expression = prepared.semantic_index().expression(*condition);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, expression.as_id()).is_ok());
    }
    let last = prepared.semantic_index().expression(conditions[3]);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, last.as_id()).is_err());

    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start();
    assert_eq!(check_file_with_policy(&prepared, &funded()), result);
    drop(recording);
    assert_eq!(snapshot().events, expected_order(&prepared, false));
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_expression_types_impl",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn interrupted_nested_stub_traversal_retires_frames_before_same_revision_retry() {
    let measured = database(true);
    let measured_prepared = prepared(&measured, true);
    observations::reset(None);
    let recording = Recording::start();
    let result = check_file_with_policy(&measured_prepared, &funded());
    drop(recording);
    assert!(matches!(result, Ok(AnalysisOutcome::Complete(Ok(_)))));
    let limited = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - snapshot().conditions[1].1,
        ..funded()
    };
    assert_cleanup();

    for cancel in [false, true] {
        let db = database(true);
        let prepared = prepared(&db, true);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::ExpressionMerged));
        let recording = Recording::start();
        let policy = if cancel { funded() } else { limited };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            check_file_with_policy(&prepared, &policy)
        }));
        drop(recording);
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            ),
            other => panic!("{other:?}"),
        }
        assert!(snapshot().max_frames > 1);
        if !cancel {
            assert_eq!(snapshot().conditions.len(), 2);
        }
        assert_cleanup();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(())
        );

        let first = prepared
            .semantic_index()
            .expression(condition_nodes(&prepared)[0]);
        let mut reader = db.clone();
        reader.take_salsa_events();
        observations::reset(None);
        let recording = Recording::start();
        let retry = check_file_with_policy(&prepared, &funded());
        drop(recording);
        assert!(
            matches!(&retry, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
            "{retry:?}"
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            Some(first.as_id()),
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}
