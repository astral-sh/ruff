use std::cell::RefCell;
use std::panic::AssertUnwindSafe;

use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FinalSourceMemo;

use super::function_decorators::{Request, Value, controlled, database, definition, prepared};
use super::*;
use crate::types::function::{
    FunctionDecorators, FunctionType, OverloadLiteral, overloads_and_implementation_ingredient,
};
use crate::types::infer::DefinitionTypes;

type Collection<'db> = (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>);

const CHAIN: &str = "from typing import overload\n@overload\ndef target(first): ...\n@overload\ndef target(second): ...\n@overload\ndef target(third): ...\n";
const GROWING_CHAIN: &str = "from typing import overload\n@overload\ndef target(first): ...\n@overload\ndef target(second): ...\n@overload\ndef target(third): ...\n@overload\ndef target(fourth): ...\n@overload\ndef target(fifth): ...\n";
const IMPLEMENTATION: &str = "from typing import overload\n@overload\ndef target(first): ...\n@overload\ndef target(second): ...\ndef target(implementation): ...\n";
const COLD_IMPORT: &str = "from typing import overload\nfrom cold import overload as first_overload\n@first_overload\ndef target(first): ...\n@overload\ndef target(second): ...\n@overload\ndef target(third): ...\n";

#[derive(Clone, Copy)]
enum Cancellation {
    None,
    Append,
    Finalize,
    SignatureBinding,
}

#[derive(Clone, Copy, Debug)]
struct Boundary {
    len: usize,
    capacity: usize,
    remaining: usize,
}

#[derive(Clone, Copy, Debug)]
struct SignatureBinding {
    definition: salsa::Id,
    remaining: usize,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    key: Option<salsa::Id>,
    live: usize,
    created: usize,
    retired: usize,
    before_append: Vec<Boundary>,
    appended: Vec<Boundary>,
    reversing: Vec<Boundary>,
    reversed: Vec<Boundary>,
    finalizing: Vec<Boundary>,
    finalized: usize,
    signature_bindings: Vec<SignatureBinding>,
    cancellation_check_returned: bool,
}

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static TARGET: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CURRENT: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CANCEL: Cell<Cancellation> = const { Cell::new(Cancellation::None) };
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

struct Recording;

impl Recording {
    fn start(cancel: Cancellation) -> Self {
        assert!(!RECORDING.replace(true));
        TARGET.set(None);
        CURRENT.set(None);
        CANCEL.set(cancel);
        JOURNAL.with_borrow_mut(|journal| *journal = Snapshot::default());
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
        TARGET.set(None);
        CURRENT.set(None);
        CANCEL.set(Cancellation::None);
    }
}

pub(super) fn select(last: OverloadLiteral<'_>) {
    if RECORDING.get() {
        TARGET.set(Some(last.as_id()));
    }
}

pub(in crate::types::infer) struct OwnerLifetime {
    previous: Option<salsa::Id>,
}

impl OwnerLifetime {
    pub(in crate::types::infer) fn new() -> Self {
        Self {
            previous: CURRENT.replace(None),
        }
    }
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        if selected() {
            JOURNAL.with_borrow_mut(|journal| {
                journal.live -= 1;
                journal.retired += 1;
            });
        }
        CURRENT.set(self.previous);
    }
}

fn selected() -> bool {
    RECORDING.get() && TARGET.get().is_some() && TARGET.get() == CURRENT.get()
}

pub(in crate::types::infer) fn collection_started(_db: &dyn Db, last: OverloadLiteral<'_>) {
    CURRENT.set(Some(last.as_id()));
    if selected() {
        JOURNAL.with_borrow_mut(|journal| {
            journal.key = Some(last.as_id());
            journal.live += 1;
            journal.created += 1;
        });
    }
}

fn boundary(db: &dyn Db, len: usize, capacity: usize) -> Boundary {
    JOURNAL.with_borrow(|journal| assert!(journal.live > 0));
    Boundary {
        len,
        capacity,
        remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
    }
}

fn cancel(db: &dyn Db) {
    CANCEL.set(Cancellation::None);
    db.cancellation_token().cancel();
    db.unwind_if_revision_cancelled();
    JOURNAL.with_borrow_mut(|journal| journal.cancellation_check_returned = true);
}

pub(in crate::types::infer) fn before_append(db: &dyn Db, len: usize, capacity: usize) {
    if selected() {
        let entry = boundary(db, len, capacity);
        JOURNAL.with_borrow_mut(|journal| journal.before_append.push(entry));
    }
}

pub(in crate::types::infer) fn appended(db: &dyn Db, len: usize, capacity: usize) {
    if selected() {
        let entry = boundary(db, len, capacity);
        JOURNAL.with_borrow_mut(|journal| journal.appended.push(entry));
        if matches!(CANCEL.get(), Cancellation::Append) {
            cancel(db);
        }
    }
}

pub(in crate::types::infer) fn reversing(db: &dyn Db, len: usize, capacity: usize) {
    if selected() {
        let entry = boundary(db, len, capacity);
        JOURNAL.with_borrow_mut(|journal| journal.reversing.push(entry));
    }
}

pub(in crate::types::infer) fn reversed(db: &dyn Db, len: usize, capacity: usize) {
    if selected() {
        let entry = boundary(db, len, capacity);
        JOURNAL.with_borrow_mut(|journal| journal.reversed.push(entry));
    }
}

pub(in crate::types::infer) fn finalizing(db: &dyn Db, len: usize, capacity: usize) {
    if selected() {
        let entry = boundary(db, len, capacity);
        JOURNAL.with_borrow_mut(|journal| journal.finalizing.push(entry));
        if matches!(CANCEL.get(), Cancellation::Finalize) {
            cancel(db);
        }
    }
}

pub(in crate::types::infer) fn finalized(_db: &dyn Db) {
    if selected() {
        JOURNAL.with_borrow_mut(|journal| journal.finalized += 1);
    }
}

pub(in crate::types::infer) fn signature_binding_requested(
    db: &dyn Db,
    definition: Definition<'_>,
) {
    if RECORDING.get() {
        let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
        JOURNAL.with_borrow_mut(|journal| {
            journal.signature_bindings.push(SignatureBinding {
                definition: definition.as_id(),
                remaining,
            });
        });
        if matches!(CANCEL.get(), Cancellation::SignatureBinding) {
            cancel(db);
        }
    }
}

fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

fn assert_cleanup() {
    let journal = snapshot();
    assert_eq!(journal.live, 0);
    assert_eq!(journal.created, journal.retired);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn last<'db>(db: &'db TestDb, function: FunctionType<'db>) -> OverloadLiteral<'db> {
    function.literal(db).last_definition
}

fn payload<'db>(
    db: &'db TestDb,
    overloads: &[OverloadLiteral<'db>],
    implementation: Option<OverloadLiteral<'db>>,
) -> (
    Vec<(TextRange, FunctionDecorators)>,
    Option<(TextRange, FunctionDecorators)>,
) {
    let entry = |overload: OverloadLiteral<'db>| {
        let definition = overload.definition(db);
        let scope = overload.body_scope(db);
        let file = definition.program_file(db);
        let module = parsed_module(db, file.python_file(db)).load(db);
        (
            scope.node(db).expect_function().node(&module).range(),
            overload.decorators(db),
        )
    };
    (
        overloads.iter().copied().map(entry).collect(),
        implementation.map(entry),
    )
}

fn assert_memo(db: &TestDb, key: salsa::Id, complete: bool) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            overloads_and_implementation_ingredient(db),
            key,
        )
        .is_ok(),
        complete,
    );
}

fn assert_ordinary_and_reuse<'db>(
    source: &str,
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    function: FunctionType<'db>,
    value: &'db Collection<'db>,
) {
    let ordinary_db = database(source);
    let ordinary_prepared = self::prepared(&ordinary_db);
    let ordinary_definition = definition(&ordinary_prepared);
    let ordinary_function = infer_definition_types(&ordinary_db, ordinary_definition)
        .function_type(ordinary_definition)
        .unwrap();
    let ordinary = ordinary_function.overloads_and_implementation(&ordinary_db);
    assert_eq!(
        payload(db, &value.0, value.1),
        payload(&ordinary_db, ordinary.0, ordinary.1)
    );
    let key = last(db, function).as_id();
    assert_memo(db, key, true);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let native = function.overloads_and_implementation(db);
    assert_eq!(native.1, value.1);
    if !value.0.is_empty() {
        assert!(std::ptr::eq(native.0, value.0.as_ref()));
    }
    assert_eq!(
        controlled(prepared, Request::Overloads, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Overloads(function, value)))
    );
    assert_function_query_was_not_run_by_name(
        db,
        "overloads_and_implementation_inner",
        Some(key),
        &events_db.take_salsa_events(),
    );
    assert_cleanup();
}

#[test]
fn cold_collection_preserves_source_order_and_implementation_and_reuses_the_canonical_memo() {
    for (source, overload_count, has_implementation) in [
        (CHAIN, 3, false),
        (IMPLEMENTATION, 2, true),
        (
            "from typing import overload\n@overload\ndef target(): ...\n",
            1,
            false,
        ),
        ("def target(): ...\n", 0, true),
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(Cancellation::None);
        let cold = capture(&db, || controlled(&prepared, Request::Overloads, &funded())).unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) = cold.value else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        assert_eq!(value.0.len(), overload_count);
        assert_eq!(value.1.is_some(), has_implementation);
        let expected = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .filter_map(Stmt::as_function_def_stmt)
            .map(|function| prepared.semantic_index().expect_single_definition(function))
            .collect::<Vec<_>>();
        assert_eq!(
            value
                .0
                .iter()
                .map(|overload| overload.definition(&db))
                .collect::<Vec<_>>(),
            expected[..overload_count]
        );
        assert_eq!(
            value.1.map(|implementation| implementation.definition(&db)),
            has_implementation.then(|| *expected.last().unwrap())
        );
        let journal = snapshot();
        assert_eq!(journal.created, 1);
        assert_eq!(journal.appended.len(), overload_count);
        assert_eq!(journal.reversing.len(), 1);
        assert_eq!(journal.reversed.len(), 1);
        // Predecessors are collected from bottom to top, then reversed into source order.
        // The final definition is appended afterward only when it is itself an overload.
        assert_eq!(
            journal.reversing[0].len,
            overload_count - usize::from(!has_implementation)
        );
        assert_eq!(journal.finalizing.len(), 1);
        assert_eq!(journal.finalized, 1);
        assert_cleanup();
        assert_ordinary_and_reuse(source, &db, &prepared, function, value);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cold_overload_collection_itself_requests_preceding_definitions() {
    let db = database(CHAIN);
    let prepared = prepared(&db);
    let previous_node = prepared.parsed_module().syntax().body[2]
        .as_function_def_stmt()
        .unwrap();
    let previous = prepared
        .semantic_index()
        .expect_single_definition(previous_node);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(Cancellation::None);
    let cold = capture(&db, || controlled(&prepared, Request::Overloads, &funded())).unwrap();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let previous_key = definition_inference_ingredient(&db).database_key_index(previous.as_id());
    let collection_key = overloads_and_implementation_ingredient(&db)
        .database_key_index(last(&db, function).as_id());
    // An overload's identity does not need its predecessors. This dependency therefore belongs
    // to collecting the overloads, after the final definition has already completed.
    assert!(
        cold.reads
            .iter()
            .any(|read| read.key == previous_key && read.parent == Some(collection_key))
    );
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_definition_types",
            Some(previous.as_id()),
            &events_db.take_salsa_events()
        )
        .is_some()
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            previous.as_id()
        )
        .is_ok()
    );
    assert_cleanup();
    assert_ordinary_and_reuse(CHAIN, &db, &prepared, function, value);
}

#[test]
fn cold_overloaded_signatures_read_canonical_bindings_and_reuse_their_memos() {
    for (source, names) in [
        (CHAIN, &["first", "second", "third"][..]),
        (IMPLEMENTATION, &["first", "second"][..]),
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(Cancellation::None);
        let cold = capture(&db, || controlled(&prepared, Request::Signature, &funded())).unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Signature(function, signature))) = cold.value
        else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        assert_eq!(signature.overloads.len(), names.len());
        for (index, (overload, name)) in signature.overloads.iter().zip(names).enumerate() {
            assert_eq!(overload.parameters().len(), 1);
            assert_eq!(
                overload
                    .parameters()
                    .iter()
                    .next()
                    .unwrap()
                    .name()
                    .unwrap()
                    .as_str(),
                *name
            );
            assert_eq!(overload.return_ty, Type::unknown());
            assert_eq!(overload.source_overload_index(), Some(index));
        }
        let definitions = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .filter_map(Stmt::as_function_def_stmt)
            .map(|node| prepared.semantic_index().expect_single_definition(node))
            .collect::<Vec<_>>();
        // When there is no implementation, the final overload supplies its raw signature.
        // Earlier overloads read their bindings so decorated callables can supply signatures.
        let bindings = &definitions[..definitions.len() - 1];
        assert_eq!(
            snapshot()
                .signature_bindings
                .iter()
                .map(|binding| binding.definition)
                .collect::<Vec<_>>(),
            bindings
                .iter()
                .map(|definition| definition.as_id())
                .collect::<Vec<_>>()
        );
        let signature_ingredient = function_literal_signature_ingredient(&db);
        let signature_key = signature_ingredient.database_key_index(function.as_id());
        let binding_reads = cold
            .reads
            .iter()
            .filter(|read| {
                read.parent == Some(signature_key)
                    && db.ingredient_debug_name(read.key.ingredient_index())
                        == "infer_definition_types"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            binding_reads
                .iter()
                .map(|read| read.key.key_index())
                .collect::<Vec<_>>(),
            bindings
                .iter()
                .map(|definition| definition.as_id())
                .collect::<Vec<_>>()
        );
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, signature_ingredient, function.as_id())
                .is_ok()
        );
        let signature_read = cold
            .reads
            .iter()
            .find(|read| read.key == signature_key)
            .unwrap();
        let ordinary_db = database(source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        let ordinary_definition = definition(&ordinary_prepared);
        let ordinary_function = infer_definition_types(&ordinary_db, ordinary_definition)
            .function_type(ordinary_definition)
            .unwrap();
        let ordinary_signature = ordinary_function.signature(&ordinary_db);
        assert_eq!(
            signature.overloads.len(),
            ordinary_signature.overloads.len()
        );
        for (overload, ordinary) in signature
            .overloads
            .iter()
            .zip(&ordinary_signature.overloads)
        {
            assert_eq!(overload.parameters(), ordinary.parameters());
            assert_eq!(overload.return_ty, ordinary.return_ty);
            assert_eq!(
                overload.source_overload_index(),
                ordinary.source_overload_index()
            );
        }
        assert_eq!(
            signature
                .overloads
                .iter()
                .map(|overload| overload.definition)
                .collect::<Vec<_>>(),
            definitions[..names.len()]
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>()
        );
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let native = capture(&db, || {
            assert!(std::ptr::eq(signature, function.signature(&db)));
            for definition in bindings {
                infer_definition_types(&db, *definition);
            }
        })
        .unwrap();
        for cold_read in binding_reads.into_iter().chain([signature_read]) {
            assert!(native.reads.iter().any(|read| {
                read.key == cold_read.key && read.memo_address == cold_read.memo_address
            }));
        }
        assert_eq!(
            controlled(&prepared, Request::Signature, &funded()),
            cold.value
        );
        let events = events_db.take_salsa_events();
        for query in [
            "function_literal_signature",
            "infer_definition_types",
            "overloads_and_implementation_inner",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn cold_overloaded_call_completes_with_the_first_matching_signature() {
    let source = "from typing import Any, overload\n@overload\ndef target(value) -> Any: ...\n@overload\ndef target(value, other): ...\ntarget(True)\n";
    let database = || {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()
            .unwrap();
        db.write_file("src/main.py", source).unwrap();
        db
    };
    let db = database();
    let prepared = prepare(&db);
    observations::reset(None);
    let cold = capture(&db, || {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    })
    .unwrap();
    assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(Type::any())));
    cold.check_root_reads().unwrap();
    assert_eq!(observations::signature_ready().0, 1);
    let ordinary_db = database();
    let ordinary_prepared = prepare(&ordinary_db);
    let expression = ordinary_prepared
        .semantic_index()
        .expression(expression_key(&ordinary_prepared));
    assert_eq!(
        infer_expression_types(&ordinary_db, expression, TypeContext::default())
            .expression_type(expression.node_ref(&ordinary_db)),
        Type::any()
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_signature_preserves_a_previously_inferred_decorated_overload() {
    let source = "from typing import Any, Callable, overload\ndef decorate(function) -> Callable[[Any, Any], Any]: ...\n@overload\n@decorate\ndef target(): ...\n@overload\ndef target(value): ...\n";
    let db = database(source);
    let prepared = prepared(&db);
    let first = prepared.parsed_module().syntax().body[2]
        .as_function_def_stmt()
        .unwrap();
    let first_definition = prepared.semantic_index().expect_single_definition(first);
    // Decorator application is inferred ordinarily before the controlled run. The final
    // function's signature is still cold; its first overload already has a decorated callable binding.
    let Type::Callable(decorated) =
        infer_definition_types(&db, first_definition).binding_type(first_definition)
    else {
        panic!("fixture decorator must replace the first overload with a callable");
    };
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || controlled(&prepared, Request::Signature, &funded())).unwrap();
    let Ok(AnalysisOutcome::Complete(Value::Signature(function, signature))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    assert_eq!(signature.overloads.len(), 2);
    assert_eq!(signature.overloads[0].parameters().len(), 2);
    assert_eq!(signature.overloads[0].return_ty, Type::any());
    assert_eq!(
        signature.overloads[0],
        decorated.signatures(&db).overloads[0]
            .clone()
            .with_source_overload_index(Some(0))
    );
    assert_eq!(signature.overloads[1].parameters().len(), 1);
    assert_eq!(signature.overloads[1].source_overload_index(), Some(1));
    let events = events_db.take_salsa_events();
    assert_eq!(observations::signature_ready().0, 1);
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(first_definition.as_id()),
        &events,
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id()
        )
        .is_ok()
    );
    assert!(std::ptr::eq(signature, function.signature(&db)));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn signature_binding_interruption_cleans_up_and_retries_in_the_same_revision() {
    let measured = database(CHAIN);
    let measured_prepared = prepared(&measured);
    observations::reset(None);
    let recording = Recording::start(Cancellation::None);
    assert!(matches!(
        controlled(&measured_prepared, Request::Signature, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    drop(recording);
    let binding_work = funded().semantic_work_limit - snapshot().signature_bindings[0].remaining;
    assert_cleanup();
    for cancellation in [false, true] {
        let db = database(CHAIN);
        let prepared = self::prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(if cancellation {
            Cancellation::SignatureBinding
        } else {
            Cancellation::None
        });
        let policy = if cancellation {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: binding_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Request::Signature, &policy)
        }));
        drop(recording);
        if cancellation {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        let journal = snapshot();
        assert!(!journal.signature_bindings.is_empty());
        assert_eq!(journal.cancellation_check_returned, cancellation);
        assert_cleanup();
        let definition = definition(&prepared);
        let function = infer_definition_types(&db, definition)
            .function_type(definition)
            .unwrap();
        // Prepared dependencies mask local cancellation until the query finishes, allowing
        // signature publication. Exhausted work stops at the binding read before publication.
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                function_literal_signature_ingredient(&db),
                function.as_id()
            )
            .is_ok(),
            cancellation
        );
        for binding in &journal.signature_bindings {
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    binding.definition
                )
                .is_ok()
            );
        }
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert!(matches!(
            controlled(&prepared, Request::Signature, &funded()),
            Ok(AnalysisOutcome::Complete(Value::Signature(_, _)))
        ));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
        if cancellation {
            assert_function_query_was_not_run_by_name(
                &db,
                "function_literal_signature",
                None,
                &events,
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn borrowed_binding_reader_preserves_constructed_cycle_fallbacks_and_missing_bindings() {
    let db = database("def target(): ...\n");
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let fallback = Type::divergent(definition.as_id());
    // These constructed inference values isolate the shared binding reader. They do not
    // represent a natural signature cycle or exercise definition-query acquisition.
    for (binding, keep_fallback, expected) in [
        (
            None,
            true,
            Ok(AnalysisOutcome::Complete(Value::Binding(fallback))),
        ),
        (
            Some(Type::bool_literal(true)),
            true,
            Ok(AnalysisOutcome::Complete(Value::Binding(
                Type::bool_literal(true),
            ))),
        ),
        (None, false, Ok(unavailable(OperationId::MissingBinding))),
    ] {
        let mut inference = DefinitionInference::cycle_initial(&db, definition, fallback);
        assert_eq!(inference.completed_binding(definition), None);
        inference.types = binding.map_or(DefinitionTypes::Empty, DefinitionTypes::Binding);
        if !keep_fallback {
            inference.extra = None;
        }
        observations::reset(None);
        let result = capture(&db, || {
            controlled(&prepared, Request::Binding(&inference), &funded())
        })
        .unwrap();
        assert_eq!(result.value, expected);
        assert!(matches!(
            result.check_root_reads(),
            Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
        ));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn function_metadata_uses_the_collector_only_for_overloaded_functions() {
    for (source, overloaded) in [(CHAIN, true), ("def target(): ...\n", false)] {
        let db = database(source);
        let prepared = prepared(&db);
        observations::reset(None);
        let recording = Recording::start(Cancellation::None);
        let result = controlled(&prepared, Request::Metadata, &funded());
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Metadata(function, overloads, implementation))) =
            result
        else {
            panic!("{result:?}");
        };
        assert_eq!(snapshot().created, usize::from(overloaded));
        assert_eq!(overloads.len(), if overloaded { 3 } else { 0 });
        assert_eq!(implementation, (!overloaded).then(|| last(&db, function)));
        assert_memo(&db, last(&db, function).as_id(), overloaded);
        let ordinary = function.overloads_and_implementation(&db);
        assert_eq!((overloads, implementation), ordinary);
        assert_cleanup();
    }
}

#[test]
fn runtime_visibility_reads_type_check_only_on_preceding_overloads_and_implementations() {
    for (source, visible) in [
        (CHAIN, true),
        (
            "from typing import overload, type_check_only\n@type_check_only\n@overload\ndef target(first): ...\n@overload\ndef target(second): ...\n",
            false,
        ),
        (
            "from typing import overload, type_check_only\n@overload\ndef target(first): ...\n@type_check_only\ndef target(implementation): ...\n",
            false,
        ),
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let cold = capture(&db, || {
            controlled(&prepared, Request::RuntimeVisibility, &funded())
        })
        .unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(Value::RuntimeVisibility(visible)))
        );
        cold.check_root_reads().unwrap();
        let root_definition = definition(&prepared);
        let function = infer_definition_types(&db, root_definition)
            .function_type(root_definition)
            .unwrap();
        assert_memo(&db, last(&db, function).as_id(), true);
        let collection_key = overloads_and_implementation_ingredient(&db)
            .database_key_index(last(&db, function).as_id());
        let visibility_key =
            runtime_visibility_ingredient(&db).database_key_index(root_definition.as_id());
        assert!(
            cold.reads
                .iter()
                .any(|read| read.key == collection_key && read.parent == Some(visibility_key))
        );
        let ordinary_db = database(source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        assert_eq!(
            may_exist_at_runtime(&ordinary_db, definition(&ordinary_prepared)),
            visible
        );
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert_eq!(may_exist_at_runtime(&db, root_definition), visible);
        assert_eq!(
            controlled(&prepared, Request::RuntimeVisibility, &funded()),
            cold.value
        );
        let events = events_db.take_salsa_events();
        for query in ["may_exist_at_runtime", "overloads_and_implementation_inner"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Growth,
    Finalization,
}

impl Stage {
    fn source(self) -> &'static str {
        match self {
            Self::Growth => GROWING_CHAIN,
            Self::Finalization => CHAIN,
        }
    }

    fn completed(self, journal: &Snapshot) -> bool {
        match self {
            Self::Growth => journal.appended.len() == 5,
            Self::Finalization => journal.finalized > 0,
        }
    }

    fn assert_refused(self, journal: &Snapshot) {
        assert!(journal.created > 0);
        match self {
            Self::Growth => {
                assert_eq!(journal.before_append.len(), 5);
                assert_eq!(journal.before_append[4].len, 4);
                assert_eq!(journal.before_append[4].capacity, 4);
                assert_eq!(journal.appended.len(), 4);
                assert!(journal.finalizing.is_empty());
            }
            Self::Finalization => {
                assert_eq!(journal.appended.len(), 3);
                assert_eq!(journal.finalizing.len(), 1);
                assert_eq!(journal.finalizing[0].len, 3);
                assert_eq!(journal.finalized, 0);
            }
        }
    }
}

fn completing_bytes(stage: Stage) -> usize {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = database(stage.source());
        let prepared = prepared(&db);
        observations::reset(None);
        let recording = Recording::start(Cancellation::None);
        let result = controlled(
            &prepared,
            Request::Overloads,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        drop(recording);
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(_))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{stage:?}: {result:?}"
        );
        if stage.completed(&snapshot()) {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_cleanup();
    }
    upper
}

#[test]
fn growth_and_finalization_admit_work_and_bytes_before_mutating_and_retry_in_the_same_revision() {
    for stage in [Stage::Growth, Stage::Finalization] {
        let db = database(stage.source());
        let prepared = prepared(&db);
        observations::reset(None);
        let recording = Recording::start(Cancellation::None);
        assert!(matches!(
            controlled(&prepared, Request::Overloads, &funded()),
            Ok(AnalysisOutcome::Complete(_))
        ));
        drop(recording);
        let measured = snapshot();
        let remaining = match stage {
            Stage::Growth => {
                assert_eq!(
                    measured.before_append[4].len,
                    measured.before_append[4].capacity
                );
                measured.before_append[4].remaining
            }
            Stage::Finalization => measured.finalizing[0].remaining,
        };
        assert_cleanup();
        let bytes = completing_bytes(stage);
        assert!(bytes > 0);
        for (policy, reason) in [
            (
                AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - remaining,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: bytes - 1,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            let db = database(stage.source());
            let prepared = self::prepared(&db);
            let revision = salsa::plumbing::current_revision(&db);
            observations::reset(None);
            let recording = Recording::start(Cancellation::None);
            assert_eq!(
                controlled(&prepared, Request::Overloads, &policy),
                Ok(AnalysisOutcome::Incomplete {
                    reason,
                    completed: ()
                })
            );
            drop(recording);
            let journal = snapshot();
            stage.assert_refused(&journal);
            assert_memo(&db, journal.key.unwrap(), false);
            assert_cleanup();
            let previous_node = prepared.parsed_module().syntax().body[2]
                .as_function_def_stmt()
                .unwrap();
            let previous = prepared
                .semantic_index()
                .expect_single_definition(previous_node);
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    previous.as_id()
                )
                .is_ok()
            );
            let mut events_db = db.clone();
            events_db.take_salsa_events();
            let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) =
                controlled(&prepared, Request::Overloads, &funded())
            else {
                panic!("funded collection retry failed");
            };
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_definition_types",
                Some(previous.as_id()),
                &events_db.take_salsa_events(),
            );
            assert_ordinary_and_reuse(stage.source(), &db, &prepared, function, value);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

#[test]
fn local_cancellation_during_collection_and_finalization_preserves_completed_memos() {
    for cancellation in [Cancellation::Append, Cancellation::Finalize] {
        let db = database(CHAIN);
        let prepared = prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(cancellation);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Request::Overloads, &funded())
        }));
        drop(recording);
        assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        let journal = snapshot();
        // Prepared dependencies keep Local cancellation masked until the fixpoint query finishes.
        assert!(journal.cancellation_check_returned);
        assert_eq!(journal.appended.len(), 3);
        assert_eq!(journal.finalized, 1);
        assert_memo(&db, journal.key.unwrap(), true);
        assert_cleanup();
        assert!(matches!(
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                salsa::prepared_source_probe::try_with_preparation(&db, || ())
            })),
            Ok(Ok(()))
        ));
        let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) =
            controlled(&prepared, Request::Overloads, &funded())
        else {
            panic!("funded collection retry failed");
        };
        assert_ordinary_and_reuse(CHAIN, &db, &prepared, function, value);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cold_predecessor_import_unmasks_local_cancellation_while_the_collection_is_unpublished() {
    let mut db = database(COLD_IMPORT);
    db.write_file("src/cold.pyi", "from typing import overload as overload\n")
        .unwrap();
    let prepared = prepared(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(Cancellation::Append);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Overloads, &funded())
    }));
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let journal = snapshot();
    assert!(journal.cancellation_check_returned);
    // The middle overload is collected first. Finding its predecessor then needs the first
    // overload's decorator, whose unprepared import unmasks the pending cancellation.
    assert_eq!(journal.appended.len(), 1);
    assert!(journal.finalizing.is_empty());
    assert_eq!(journal.finalized, 0);
    assert_memo(&db, journal.key.unwrap(), false);
    assert_cleanup();
    let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) =
        controlled(&prepared, Request::Overloads, &funded())
    else {
        panic!("funded collection retry failed");
    };
    assert_eq!(value.0.len(), 3);
    assert!(value.1.is_none());
    assert_memo(&db, last(&db, function).as_id(), true);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[derive(Clone, Copy, Debug, Default)]
struct CycleSnapshot {
    initial: usize,
    self_edges: usize,
    bodies: usize,
    recoveries: usize,
    first_previous_is_empty: bool,
}

thread_local! {
    static FORCE_CYCLE: Cell<bool> = const { Cell::new(false) };
    static CYCLE: Cell<CycleSnapshot> = const { Cell::new(CycleSnapshot {
        initial: 0,
        self_edges: 0,
        bodies: 0,
        recoveries: 0,
        first_previous_is_empty: false,
    }) };
}

struct CycleMode;

impl CycleMode {
    fn enter() -> Self {
        assert!(!FORCE_CYCLE.replace(true));
        CYCLE.set(CycleSnapshot::default());
        Self
    }
}

impl Drop for CycleMode {
    fn drop(&mut self) {
        FORCE_CYCLE.set(false);
    }
}

pub(in crate::types::infer::source_runtime) struct CycleProvider<'db, MakeAccess> {
    inner: FunctionOverloadsProvider<'db, MakeAccess>,
}

impl<'db, MakeAccess> CycleProvider<'db, MakeAccess> {
    pub(in crate::types::infer::source_runtime) fn new(
        inner: FunctionOverloadsProvider<'db, MakeAccess>,
    ) -> Self {
        Self { inner }
    }
}

fn force_cycle(input: OverloadLiteral<'_>) -> bool {
    FORCE_CYCLE.get() && TARGET.get() == Some(input.as_id())
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for CycleProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = OverloadLiteral<'a>,
            Output<'a> = Collection<'a>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        <FunctionOverloadsProvider<'db, MakeAccess> as CallableRouteProvider<'run, 'db, C>>::native_value(&self.inner, endpoint, db, operation).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: OverloadLiteral<'db>,
    ) -> RunResult<Collection<'db>>
    where
        'run: 'call,
    {
        let value = <FunctionOverloadsProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::initial(&self.inner, endpoint, db, id, input)
        .await?;
        if force_cycle(input) {
            assert!(value.0.is_empty());
            assert!(value.1.is_none());
            let mut journal = CYCLE.get();
            journal.initial += 1;
            CYCLE.set(journal);
        }
        Ok(value)
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: OverloadLiteral<'db>,
    ) -> RunResult<Collection<'db>>
    where
        'run: 'call,
    {
        if force_cycle(input) {
            let access = create_source_access(&endpoint, &self.inner.access).await?;
            // This synthetic self-edge exercises the real query's initial callback. The ordinary
            // collector below still supplies the candidate that recovery eventually publishes.
            let previous = access.function_overloads(input).await?;
            let mut journal = CYCLE.get();
            if journal.self_edges == 0 {
                assert!(previous.0.is_empty());
                assert!(previous.1.is_none());
            }
            journal.self_edges += 1;
            CYCLE.set(journal);
        }
        let value = <FunctionOverloadsProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::body(&self.inner, endpoint, db, input)
        .await?;
        if force_cycle(input) {
            assert_eq!(value.0.as_ref(), &[input]);
            assert!(value.1.is_none());
            let mut journal = CYCLE.get();
            journal.bodies += 1;
            CYCLE.set(journal);
        }
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Collection<'db>,
        value: Collection<'db>,
        input: OverloadLiteral<'db>,
    ) -> RunResult<Collection<'db>>
    where
        'run: 'call,
    {
        let address = value.0.as_ptr();
        let length = value.0.len();
        let implementation = value.1;
        if force_cycle(input) {
            let mut journal = CYCLE.get();
            if journal.recoveries == 0 {
                journal.first_previous_is_empty = last.0.is_empty() && last.1.is_none();
            }
            journal.recoveries += 1;
            CYCLE.set(journal);
        }
        let value = <FunctionOverloadsProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::recover(&self.inner, endpoint, db, cycle, last, value, input)
        .await?;
        if force_cycle(input) {
            assert_eq!(value.0.as_ptr(), address);
            assert_eq!(value.0.len(), length);
            assert_eq!(value.1, implementation);
        }
        Ok(value)
    }
}

#[test]
fn synthetic_cycle_preserves_the_empty_initial_result_and_publishes_the_collected_candidate() {
    let source = "from typing import overload\n@overload\ndef target(): ...\n";
    let db = database(source);
    let prepared = prepared(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(Cancellation::None);
    let cycle = CycleMode::enter();
    let cold = capture(&db, || controlled(&prepared, Request::Overloads, &funded())).unwrap();
    drop(cycle);
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Value::Overloads(function, value))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let journal = CYCLE.get();
    assert!(journal.initial > 0);
    assert!(journal.self_edges > 0);
    assert!(journal.bodies > 0);
    assert!(journal.recoveries > 0);
    assert!(journal.first_previous_is_empty);
    assert_eq!(value.0.as_ref(), &[last(&db, function)]);
    assert!(value.1.is_none());
    assert_cleanup();
    assert_ordinary_and_reuse(source, &db, &prepared, function, value);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}
