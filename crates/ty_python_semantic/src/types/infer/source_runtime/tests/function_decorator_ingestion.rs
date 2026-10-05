use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::panic::AssertUnwindSafe;

use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHasher};
use salsa::execution_probe::FinalSourceMemo;

use super::function_decorators::{
    Request, Value, controlled, database, definition, prepared, selected_function,
};
use super::*;
use crate::analysis::CallableConversionOperation;
use crate::types::TypeCheckDiagnostics;
use crate::types::function::{FunctionDecoratorKind, FunctionDecorators, FunctionType};
use crate::types::infer::builder::source_definition::controlled::FunctionDefinitionEffects;
use crate::types::infer::{
    FunctionDecoratorInference, InferenceFlags, function_decorator_inference_ingredient,
    function_known_decorators,
};

const MARKERS: &str = "from typing import no_type_check, type_check_only\n@no_type_check\n@type_check_only\ndef target(): ...\n";
const OVERLOAD_BODY: &str = "from typing import overload\n@overload\ndef target():\n    pass\n    'not a docstring'\n    ...\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) struct BuilderState<'db> {
    pub(in crate::types::infer) definition: Option<Definition<'db>>,
    pub(in crate::types::infer) flags: InferenceFlags,
    // Each collection records its length and capacity, in that order.
    pub(in crate::types::infer) expressions: (usize, usize),
    pub(in crate::types::infer) bindings: (usize, usize),
    pub(in crate::types::infer) called: (usize, usize),
    pub(in crate::types::infer) aliases: (usize, usize),
    pub(in crate::types::infer) diagnostics: (usize, usize),
    pub(in crate::types::infer) used_suppressions: (usize, usize),
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::types::infer) struct BuilderContents<'db> {
    pub(in crate::types::infer) expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,
    pub(in crate::types::infer) bindings: Vec<(Definition<'db>, Type<'db>)>,
    pub(in crate::types::infer) called: Vec<FunctionType<'db>>,
    pub(in crate::types::infer) aliases: Vec<Definition<'db>>,
    pub(in crate::types::infer) diagnostics: TypeCheckDiagnostics,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Collections {
    expressions: (usize, usize),
    bindings: (usize, usize),
    called: (usize, usize),
    aliases: (usize, usize),
    diagnostics: (usize, usize),
    used_suppressions: (usize, usize),
}

impl From<BuilderState<'_>> for Collections {
    fn from(state: BuilderState<'_>) -> Self {
        Self {
            expressions: state.expressions,
            bindings: state.bindings,
            called: state.called,
            aliases: state.aliases,
            diagnostics: state.diagnostics,
            used_suppressions: state.used_suppressions,
        }
    }
}

#[derive(Clone, Debug)]
struct Merge {
    remaining: usize,
    storage: Collections,
    // Number of live unpublished inference builders at this merge boundary.
    live: usize,
}

#[derive(Clone, Debug)]
struct Classification {
    ty: u64,
    decorators: FunctionDecorators,
    unknown: bool,
    flags: InferenceFlags,
}

#[derive(Clone, Debug)]
struct Candidates {
    decorators: FunctionDecorators,
    flags: InferenceFlags,
    transforming: bool,
    entries: Vec<(u64, TextRange)>,
}

#[derive(Clone, Debug)]
struct OverloadStatement {
    range: TextRange,
    remaining: usize,
    live: usize,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    before: Vec<Merge>,
    merged: Vec<Merge>,
    classified: Vec<Classification>,
    candidates: Vec<Candidates>,
    statements: Vec<OverloadStatement>,
    refused_statements: Vec<(TextRange, Collections)>,
    // Live-builder counts at definition entry and when that definition's owner scope exits.
    retired: Vec<(usize, usize)>,
    cancellation_requested: bool,
    cancellation_check_returned: bool,
}

thread_local! {
    static TARGET: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CANCEL_MERGED: Cell<bool> = const { Cell::new(false) };
    static CANCEL_STATEMENT: Cell<Option<usize>> = const { Cell::new(None) };
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

pub(super) struct Recording;

impl Recording {
    pub(super) fn start(definition: Definition<'_>, cancel_merged: bool) -> Self {
        assert!(TARGET.replace(Some(definition.as_id())).is_none());
        CANCEL_MERGED.set(cancel_merged);
        JOURNAL.with_borrow_mut(|journal| *journal = Snapshot::default());
        Self
    }

    fn cancel_at_statement(definition: Definition<'_>, index: usize) -> Self {
        let recording = Self::start(definition, false);
        CANCEL_STATEMENT.set(Some(index));
        recording
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        TARGET.set(None);
        CANCEL_MERGED.set(false);
        CANCEL_STATEMENT.set(None);
    }
}

pub(in crate::types::infer) struct OwnerLifetime {
    baseline: Option<usize>,
}

impl OwnerLifetime {
    pub(in crate::types::infer) fn new(definition: Definition<'_>) -> Self {
        Self {
            baseline: (TARGET.get() == Some(definition.as_id())).then(|| observations::counts().0),
        }
    }
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        if let Some(baseline) = self.baseline {
            JOURNAL.with_borrow_mut(|journal| {
                journal.retired.push((baseline, observations::counts().0));
            });
        }
    }
}

fn selected(builder: &TypeInferenceBuilder<'_, '_>) -> bool {
    builder
        .function_decorator_test_state()
        .definition
        .is_some_and(|definition| TARGET.get() == Some(definition.as_id()))
}

fn type_fingerprint(ty: Type<'_>) -> u64 {
    let mut hasher = FxHasher::default();
    ty.hash(&mut hasher);
    hasher.finish()
}

fn merge_observation(db: &dyn Db, builder: &TypeInferenceBuilder<'_, '_>) -> Merge {
    Merge {
        remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
        storage: builder.function_decorator_test_state().into(),
        live: observations::counts().0,
    }
}

pub(in crate::types::infer) fn merge_start(
    db: &dyn Db,
    builder: &TypeInferenceBuilder<'_, '_>,
    _inference: &FunctionDecoratorInference<'_>,
) {
    if selected(builder) {
        JOURNAL.with_borrow_mut(|journal| journal.before.push(merge_observation(db, builder)));
    }
}

pub(in crate::types::infer) fn merged(db: &dyn Db, builder: &TypeInferenceBuilder<'_, '_>) {
    if !selected(builder) {
        return;
    }
    JOURNAL.with_borrow_mut(|journal| journal.merged.push(merge_observation(db, builder)));
    if CANCEL_MERGED.replace(false) {
        JOURNAL.with_borrow_mut(|journal| journal.cancellation_requested = true);
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
        JOURNAL.with_borrow_mut(|journal| journal.cancellation_check_returned = true);
    }
}

pub(in crate::types::infer) fn classified(
    _db: &dyn Db,
    builder: &TypeInferenceBuilder<'_, '_>,
    ty: Type<'_>,
    kind: FunctionDecoratorKind,
) {
    if selected(builder) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.classified.push(Classification {
                ty: type_fingerprint(ty),
                decorators: kind.flags(),
                unknown: kind.is_unknown(),
                flags: builder.function_decorator_test_state().flags,
            });
        });
    }
}

pub(in crate::types::infer) fn candidates(
    _db: &dyn Db,
    definition: Definition<'_>,
    decorators: FunctionDecorators,
    flags: InferenceFlags,
    candidates: &[(Type<'_>, &ast::Decorator)],
    transforming: bool,
) {
    if TARGET.get() == Some(definition.as_id()) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.candidates.push(Candidates {
                decorators,
                flags,
                transforming,
                entries: candidates
                    .iter()
                    .map(|(ty, decorator)| (type_fingerprint(*ty), decorator.expression.range()))
                    .collect(),
            });
        });
    }
}

pub(in crate::types::infer) fn overload_statement(
    db: &dyn Db,
    definition: Definition<'_>,
    statement: &ast::Stmt,
) {
    if TARGET.get() != Some(definition.as_id()) {
        return;
    }
    let index = JOURNAL.with_borrow_mut(|journal| {
        let index = journal.statements.len();
        journal.statements.push(OverloadStatement {
            range: statement.range(),
            remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
            live: observations::counts().0,
        });
        index
    });
    if CANCEL_STATEMENT.get() == Some(index) {
        CANCEL_STATEMENT.set(None);
        JOURNAL.with_borrow_mut(|journal| journal.cancellation_requested = true);
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
        JOURNAL.with_borrow_mut(|journal| journal.cancellation_check_returned = true);
    }
}

pub(in crate::types::infer) fn overload_body_refused(
    _db: &dyn Db,
    builder: &TypeInferenceBuilder<'_, '_>,
    statement: &ast::Stmt,
) {
    if selected(builder) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.refused_statements.push((
                statement.range(),
                builder.function_decorator_test_state().into(),
            ));
        });
    }
}

fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

pub(super) fn assert_empty_cycle_seed_was_ingested() {
    let journal = snapshot();
    assert_eq!(journal.before.len(), 1);
    assert_eq!(journal.merged.len(), 1);
    assert_eq!(journal.merged[0].storage.expressions.0, 0);
    assert_eq!(journal.merged[0].storage.bindings.0, 0);
    assert_eq!(journal.classified.len(), 1);
    assert!(journal.classified[0].unknown);
    assert_eq!(journal.candidates.len(), 1);
    assert!(journal.candidates[0].transforming);
    assert_eq!(journal.candidates[0].entries.len(), 1);
    assert!(!journal.retired.is_empty());
    assert_cleanup();
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert!(
        snapshot()
            .retired
            .iter()
            .all(|(before, after)| before == after)
    );
}

fn assert_memo(db: &TestDb, definition: Definition<'_>, complete: bool) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok(),
        complete,
    );
}

fn assert_child_memo(db: &TestDb, definition: Definition<'_>) {
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            function_decorator_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok()
    );
}

#[derive(Debug, Eq, PartialEq)]
struct DefinitionPayload {
    binding: String,
    declaration: Option<String>,
    undecorated: Option<String>,
    decorators: Vec<String>,
    diagnostics: Vec<String>,
}

fn definition_diagnostics<'a>(
    inference: &'a DefinitionInference<'_>,
) -> Option<&'a TypeCheckDiagnostics> {
    match inference.extra.as_deref() {
        Some(DefinitionInferenceExtra::Diagnostics(diagnostics)) => Some(diagnostics),
        Some(DefinitionInferenceExtra::Other(extra)) => Some(&extra.diagnostics),
        _ => None,
    }
}

fn definition_payload<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    inference: &DefinitionInference<'db>,
) -> DefinitionPayload {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    DefinitionPayload {
        binding: inference
            .binding_type(definition(prepared))
            .display(db, &env)
            .to_string(),
        declaration: inference
            .completed_declaration(definition(prepared))
            .map(|ty| ty.inner_type().display(db, &env).to_string()),
        undecorated: inference
            .undecorated_type()
            .map(|ty| ty.display(db, &env).to_string()),
        decorators: selected_function(prepared)
            .decorator_list
            .iter()
            .map(|decorator| {
                inference
                    .expression_type(&decorator.expression)
                    .display(db, &env)
                    .to_string()
            })
            .collect(),
        diagnostics: definition_diagnostics(inference)
            .map(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.headline_message().to_string())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn assert_ordinary_overload_and_memo_reuse<'db>(
    source: &str,
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    inference: &'db DefinitionInference<'db>,
) {
    let ordinary_db = database(source);
    let ordinary_prepared = self::prepared(&ordinary_db);
    let ordinary = infer_definition_types(&ordinary_db, definition(&ordinary_prepared));
    assert_eq!(
        definition_payload(db, prepared, inference),
        definition_payload(&ordinary_db, &ordinary_prepared, ordinary),
        "{source}"
    );
    let definition = definition(prepared);
    assert_memo(db, definition, true);
    if !selected_function(prepared).decorator_list.is_empty() {
        assert_child_memo(db, definition);
    }
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert!(std::ptr::eq(
        inference,
        infer_definition_types(db, definition)
    ));
    assert_eq!(
        controlled(prepared, Request::Definition, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Definition(inference)))
    );
    let events = events_db.take_salsa_events();
    for query in ["infer_definition_types", "function_known_decorators"] {
        assert_function_query_was_not_run_by_name(db, query, Some(definition.as_id()), &events);
    }
    assert_cleanup();
}

#[test]
fn cold_marker_definitions_match_ordinary_inference_and_reuse_the_canonical_memo() {
    for (decorators, expected) in [
        ("@no_type_check\n", FunctionDecorators::NO_TYPE_CHECK),
        ("@type_check_only\n", FunctionDecorators::TYPE_CHECK_ONLY),
        (
            "@no_type_check\n@type_check_only\n",
            FunctionDecorators::NO_TYPE_CHECK | FunctionDecorators::TYPE_CHECK_ONLY,
        ),
        (
            "@type_check_only\n@no_type_check\n",
            FunctionDecorators::NO_TYPE_CHECK | FunctionDecorators::TYPE_CHECK_ONLY,
        ),
    ] {
        let source = format!(
            "from typing import no_type_check, type_check_only\n{decorators}def target(): ...\n"
        );
        let db = database(&source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(definition, false);
        let cold = capture(&db, || {
            controlled(&prepared, Request::Definition, &funded())
        })
        .unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Definition(inference))) = cold.value else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        let child_key =
            function_decorator_inference_ingredient(&db).database_key_index(definition.as_id());
        let parent_key =
            definition_inference_ingredient(&db).database_key_index(definition.as_id());
        assert!(
            cold.reads
                .iter()
                .any(|read| read.key == child_key && read.parent == Some(parent_key))
        );
        let journal = snapshot();
        assert_eq!(journal.before.len(), 1);
        assert_eq!(journal.merged.len(), 1);
        assert!(journal.merged[0].live > 0);
        assert!(journal.merged[0].storage.expressions.0 > journal.before[0].storage.expressions.0);
        assert_eq!(journal.candidates.len(), 1);
        assert_eq!(journal.candidates[0].decorators, expected);
        assert!(!journal.candidates[0].transforming);
        assert!(journal.candidates[0].entries.is_empty());
        assert_eq!(
            journal.candidates[0]
                .flags
                .contains(InferenceFlags::IN_NO_TYPE_CHECK),
            expected.contains(FunctionDecorators::NO_TYPE_CHECK)
        );
        let mut suppressed = false;
        for entry in &journal.classified {
            assert!(!entry.unknown);
            assert_eq!(
                entry.flags.contains(InferenceFlags::IN_NO_TYPE_CHECK),
                suppressed
            );
            suppressed |= entry.decorators.contains(FunctionDecorators::NO_TYPE_CHECK);
        }
        assert_eq!(
            journal.classified.len(),
            selected_function(&prepared).decorator_list.len()
        );
        assert!(!journal.retired.is_empty());
        assert_cleanup();

        let ordinary_db = database(&source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        let ordinary = infer_definition_types(&ordinary_db, self::definition(&ordinary_prepared));
        assert_eq!(
            definition_payload(&db, &prepared, inference),
            definition_payload(&ordinary_db, &ordinary_prepared, ordinary),
            "{source}"
        );
        assert_memo(&db, definition, true);
        assert_child_memo(&db, definition);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert!(std::ptr::eq(
            inference,
            infer_definition_types(&db, definition)
        ));
        assert_eq!(
            controlled(&prepared, Request::Definition, &funded()),
            cold.value
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "function_known_decorators"] {
            assert_function_query_was_not_run_by_name(
                &db,
                query,
                Some(definition.as_id()),
                &events,
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn cold_overload_bodies_match_ordinary_inference_and_reuse_the_canonical_memo() {
    for (body, implementation) in [
        ("    pass\n", ""),
        ("    'body'\n", ""),
        ("    ...\n", ""),
        ("    pass\n    'not a docstring'\n    ...\n", ""),
        (
            "    pass\n    'not a docstring'\n    ...\n",
            "def target(): ...\n",
        ),
    ] {
        let source = format!(
            "from typing import overload\n@overload\ndef target():\n{body}{implementation}"
        );
        let db = database(&source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        // The driver requests the last definition. When an implementation follows the overload,
        // completing that implementation must first infer the overload we record here.
        let overload = prepared.parsed_module().syntax().body[1]
            .as_function_def_stmt()
            .unwrap();
        let overload_definition = prepared.semantic_index().expect_single_definition(overload);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(overload_definition, false);
        let cold = capture(&db, || {
            controlled(&prepared, Request::Definition, &funded())
        })
        .unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Definition(inference))) = cold.value else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        let journal = snapshot();
        assert_eq!(
            journal
                .statements
                .iter()
                .map(|statement| statement.range)
                .collect::<Vec<_>>(),
            overload.body.iter().map(Ranged::range).collect::<Vec<_>>()
        );
        assert!(
            journal
                .statements
                .iter()
                .all(|statement| statement.live > 0)
        );
        assert!(
            journal
                .statements
                .windows(2)
                .all(|pair| pair[0].remaining > pair[1].remaining)
        );
        assert_eq!(journal.candidates.len(), 1);
        assert_eq!(
            journal.candidates[0].decorators,
            FunctionDecorators::OVERLOAD
        );
        assert!(!journal.retired.is_empty());
        assert_cleanup();
        if !implementation.is_empty() {
            let previous_key = definition_inference_ingredient(&db)
                .database_key_index(overload_definition.as_id());
            let parent_key =
                definition_inference_ingredient(&db).database_key_index(definition.as_id());
            assert!(
                cold.reads
                    .iter()
                    .any(|read| read.key == previous_key && read.parent == Some(parent_key))
            );
            assert_memo(&db, overload_definition, true);
            assert_child_memo(&db, overload_definition);
        }
        assert_ordinary_overload_and_memo_reuse(&source, &db, &prepared, inference);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn transforming_decorators_preserve_classification_and_candidate_order_before_refusal() {
    for (decorators, unknown, flags, refusal) in [
        (
            "@property\n",
            vec![false],
            FunctionDecorators::empty(),
            OperationId::CallableConversion(CallableConversionOperation::RuntimeUnion),
        ),
        (
            "@True\n",
            vec![true],
            FunctionDecorators::empty(),
            OperationId::CallBindings,
        ),
        (
            "@classmethod\n@True\n@staticmethod\n",
            vec![false, true, false],
            FunctionDecorators::CLASSMETHOD | FunctionDecorators::STATICMETHOD,
            OperationId::CallBindings,
        ),
        (
            "@staticmethod\n@classmethod\n@True\n",
            vec![false, false, true],
            FunctionDecorators::CLASSMETHOD | FunctionDecorators::STATICMETHOD,
            OperationId::CallBindings,
        ),
        (
            "@True\n@staticmethod\n@classmethod\n",
            vec![true, false, false],
            FunctionDecorators::CLASSMETHOD | FunctionDecorators::STATICMETHOD,
            OperationId::CheckerKnownFunction,
        ),
    ] {
        let source = format!(
            "from builtins import property, classmethod, staticmethod\n{decorators}def target(): ...\n"
        );
        let db = database(&source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        for attempt in 0..2 {
            let mut events_db = db.clone();
            events_db.take_salsa_events();
            observations::reset(None);
            let recording = Recording::start(definition, false);
            let cold = capture(&db, || {
                controlled(&prepared, Request::Definition, &funded())
            })
            .unwrap();
            drop(recording);
            assert_eq!(cold.value, Ok(unavailable(refusal)), "{source}; attempt {attempt}");
            // A refused definition need not return a final root read, even when its children finish.
            assert!(matches!(
                cold.check_root_reads(),
                Ok(()) | Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
            ));
            let journal = snapshot();
            assert_eq!(journal.merged.len(), 1);
            assert!(journal.merged[0].storage.expressions.0 > 0);
            assert_eq!(
                journal
                    .classified
                    .iter()
                    .map(|entry| entry.unknown)
                    .collect::<Vec<_>>(),
                unknown
            );
            assert_eq!(journal.candidates.len(), 1);
            let candidates = &journal.candidates[0];
            assert!(candidates.transforming);
            assert_eq!(candidates.decorators, flags);
            let child = function_known_decorators(&db, definition);
            assert_eq!(
                child.has_unknown_decorators(),
                unknown.iter().any(|value| *value)
            );
            let expected = selected_function(&prepared)
                .decorator_list
                .iter()
                .map(|decorator| {
                    (
                        type_fingerprint(child.expression_type(&decorator.expression).unwrap()),
                        decorator.expression.range(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(candidates.entries, expected);
            assert_eq!(
                journal
                    .classified
                    .iter()
                    .map(|entry| &entry.ty)
                    .collect::<Vec<_>>(),
                expected.iter().map(|(ty, _)| ty).collect::<Vec<_>>()
            );
            assert!(!journal.retired.is_empty());
            assert_memo(&db, definition, false);
            assert_child_memo(&db, definition);
            assert_cleanup();
            if attempt > 0 {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "function_known_decorators",
                    Some(definition.as_id()),
                    &events_db.take_salsa_events(),
                );
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

#[test]
fn final_and_overload_decorators_reach_explicit_metadata_boundaries() {
    for (decorator, expected, body, reported_statement) in [
        ("final", FunctionDecorators::FINAL, " ...\n", None),
        (
            "overload",
            FunctionDecorators::OVERLOAD,
            "\n    pass\n    'not a docstring'\n    ...\n    1\n    2\n",
            Some(3),
        ),
        (
            "overload",
            FunctionDecorators::OVERLOAD,
            "\n    pass\n    'not a docstring'\n    ...\n    1  # ty: ignore[useless-overload-body]\n    2\n",
            Some(4),
        ),
    ] {
        let source = format!("from typing import {decorator}\n@{decorator}\ndef target():{body}");
        let db = database(&source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..2 {
            observations::reset(None);
            let recording = Recording::start(definition, false);
            assert_eq!(
                controlled(&prepared, Request::Definition, &funded()),
                Ok(unavailable(OperationId::FunctionMetadata)),
                "{source}"
            );
            drop(recording);
            let journal = snapshot();
            assert_eq!(journal.merged.len(), 1);
            assert_eq!(journal.candidates.len(), 1);
            assert_eq!(journal.candidates[0].decorators, expected);
            assert!(!journal.candidates[0].transforming);
            assert!(journal.candidates[0].entries.is_empty());
            if decorator == "overload" {
                // Controlled reporting refuses before consulting suppressions, so even an ignored
                // first invalid statement stops here without recording a used suppression.
                let body = &selected_function(&prepared).body;
                assert_eq!(
                    journal.statements.iter().map(|statement| statement.range).collect::<Vec<_>>(),
                    body[..4].iter().map(Ranged::range).collect::<Vec<_>>()
                );
                assert_eq!(journal.refused_statements.len(), 1);
                assert_eq!(journal.refused_statements[0].0, body[3].range());
                let before = &journal.merged[0].storage;
                let refused = &journal.refused_statements[0].1;
                assert_eq!(before.diagnostics.0, 0);
                assert_eq!(before.used_suppressions.0, 0);
                assert_eq!(refused.diagnostics, before.diagnostics);
                assert_eq!(refused.used_suppressions, before.used_suppressions);
            } else {
                assert!(journal.statements.is_empty());
                assert!(journal.refused_statements.is_empty());
            }
            assert_memo(&db, definition, false);
            assert_child_memo(&db, definition);
            assert!(!journal.retired.is_empty());
            assert_cleanup();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        let ordinary_db = database(&source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        let ordinary = infer_definition_types(&ordinary_db, self::definition(&ordinary_prepared));
        let expected_id = if decorator == "final" {
            "final-on-non-method"
        } else {
            "useless-overload-body"
        };
        let diagnostics = definition_diagnostics(ordinary)
            .unwrap()
            .into_iter()
            .filter(|diagnostic| diagnostic.id().as_str() == expected_id)
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1);
        if let Some(statement) = reported_statement {
            assert_eq!(
                diagnostics[0].primary_span().unwrap().range(),
                Some(selected_function(&ordinary_prepared).body[statement].range())
            );
        }
    }
}

#[test]
fn ordinary_overload_body_reporting_continues_after_suppression_and_stops_after_emission() {
    let source = "from typing import overload\n@overload\ndef target():\n    1  # ty: ignore[useless-overload-body]\n    2\n    3\n";
    let db = database(source);
    let prepared = prepared(&db);
    let inference = infer_definition_types(&db, definition(&prepared));
    let diagnostics = definition_diagnostics(inference)
        .unwrap()
        .into_iter()
        .filter(|diagnostic| diagnostic.id().as_str() == "useless-overload-body")
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics[0];
    assert_eq!(
        diagnostic.primary_span().unwrap().range(),
        Some(selected_function(&prepared).body[1].range())
    );
    assert_eq!(
        diagnostic.headline_message(),
        "Useless body for `@overload`-decorated function `target`"
    );
    assert_eq!(
        diagnostic.primary_annotation().unwrap().get_message(),
        Some("This statement will never be executed")
    );
    assert_eq!(
        diagnostic
            .sub_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.headline_message())
            .collect::<Vec<_>>(),
        [
            "`@overload`-decorated functions are solely for type checkers and must be overwritten at runtime by a non-`@overload`-decorated implementation",
            "Consider replacing this function body with `...` or `pass`",
        ]
    );
}

#[test]
fn work_refusal_before_and_after_ingestion_discards_the_definition_but_reuses_its_child() {
    let measured = database(MARKERS);
    let measured_prepared = prepared(&measured);
    observations::reset(None);
    let recording = Recording::start(definition(&measured_prepared), false);
    assert!(matches!(
        controlled(&measured_prepared, Request::Definition, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Definition(_)))
    ));
    drop(recording);
    let journal = snapshot();
    let before_merge = funded().semantic_work_limit - journal.before[0].remaining;
    let after_merge = funded().semantic_work_limit - journal.merged[0].remaining;
    assert!(before_merge < after_merge);
    assert_cleanup();

    for (limit, merged_count) in [(before_merge, 0), (after_merge, 1)] {
        let db = database(MARKERS);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(definition, false);
        let result = controlled(
            &prepared,
            Request::Definition,
            &AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
        );
        drop(recording);
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            })
        );
        let journal = snapshot();
        assert_eq!(journal.before.len(), 1);
        assert_eq!(journal.merged.len(), merged_count);
        assert!(journal.candidates.is_empty());
        assert!(!journal.retired.is_empty());
        assert_memo(&db, definition, false);
        assert_child_memo(&db, definition);
        assert_cleanup();

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert!(matches!(
            controlled(&prepared, Request::Definition, &funded()),
            Ok(AnalysisOutcome::Complete(Value::Definition(_)))
        ));
        assert_function_query_was_not_run_by_name(
            &db,
            "function_known_decorators",
            Some(definition.as_id()),
            &events_db.take_salsa_events(),
        );
        assert_memo(&db, definition, true);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn work_refusal_during_overload_body_scanning_discards_the_definition_but_reuses_its_child() {
    let measured = database(OVERLOAD_BODY);
    let measured_prepared = prepared(&measured);
    observations::reset(None);
    let recording = Recording::start(definition(&measured_prepared), false);
    assert!(matches!(
        controlled(&measured_prepared, Request::Definition, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Definition(_)))
    ));
    drop(recording);
    let measured_journal = snapshot();
    assert_eq!(measured_journal.statements.len(), 3);
    assert_cleanup();

    // Each statement is observed after its work admission. One unit less refuses that
    // admission, so only the preceding statements can appear in the journal.
    for (index, statement) in measured_journal.statements.iter().enumerate() {
        let db = database(OVERLOAD_BODY);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(definition, false);
        assert_eq!(
            controlled(
                &prepared,
                Request::Definition,
                &AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - statement.remaining - 1,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            })
        );
        drop(recording);
        let journal = snapshot();
        assert_eq!(
            journal
                .statements
                .iter()
                .map(|statement| statement.range)
                .collect::<Vec<_>>(),
            selected_function(&prepared).body[..index]
                .iter()
                .map(Ranged::range)
                .collect::<Vec<_>>()
        );
        assert!(journal.refused_statements.is_empty());
        assert!(!journal.retired.is_empty());
        assert_memo(&db, definition, false);
        assert_child_memo(&db, definition);
        assert_cleanup();

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let Ok(AnalysisOutcome::Complete(Value::Definition(inference))) =
            controlled(&prepared, Request::Definition, &funded())
        else {
            panic!("funded overload retry did not complete");
        };
        assert_function_query_was_not_run_by_name(
            &db,
            "function_known_decorators",
            Some(definition.as_id()),
            &events_db.take_salsa_events(),
        );
        assert_ordinary_overload_and_memo_reuse(OVERLOAD_BODY, &db, &prepared, inference);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn native_cancellation_after_ingestion_retires_the_owner_and_preserves_completed_children() {
    let db = database(MARKERS);
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(definition, true);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Definition, &funded())
    }));
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let journal = snapshot();
    assert!(journal.cancellation_requested);
    assert_eq!(journal.merged.len(), 1);
    assert!(!journal.retired.is_empty());
    assert_child_memo(&db, definition);
    assert_cleanup();
    // A fixpoint query may finish while Local cancellation is masked. Only a completed
    // definition is eligible for reuse; an interrupted definition must run again.
    let completed = FinalSourceMemo::certify(
        &db as &dyn Db,
        definition_inference_ingredient(&db),
        definition.as_id(),
    )
    .is_ok();
    assert!(!completed || journal.cancellation_check_returned);
    assert!(matches!(
        salsa::Cancelled::catch(AssertUnwindSafe(|| {
            salsa::prepared_source_probe::try_with_preparation(&db, || ())
        })),
        Ok(Ok(()))
    ));
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert!(matches!(
        controlled(&prepared, Request::Definition, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Definition(_)))
    ));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "function_known_decorators",
        Some(definition.as_id()),
        &events,
    );
    if completed {
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events,
        );
    } else {
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events
            )
            .is_some()
        );
    }
    assert_memo(&db, definition, true);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn native_cancellation_during_overload_body_scanning_retires_the_owner_and_preserves_completed_children()
 {
    let db = database(OVERLOAD_BODY);
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::cancel_at_statement(definition, 1);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Definition, &funded())
    }));
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let journal = snapshot();
    assert!(journal.cancellation_requested);
    assert!(journal.statements.len() >= 2);
    assert_eq!(
        journal.statements[1].range,
        selected_function(&prepared).body[1].range()
    );
    assert!(
        journal
            .statements
            .iter()
            .all(|statement| statement.live > 0)
    );
    assert!(journal.refused_statements.is_empty());
    assert!(!journal.retired.is_empty());
    assert_child_memo(&db, definition);
    assert_cleanup();
    // Cancellation can remain masked until a fixpoint query finishes, but an interrupted
    // definition must not publish a reusable memo for only a prefix of its body.
    let completed = FinalSourceMemo::certify(
        &db as &dyn Db,
        definition_inference_ingredient(&db),
        definition.as_id(),
    )
    .is_ok();
    assert!(!completed || journal.cancellation_check_returned);
    if completed {
        assert_eq!(
            journal.statements.len(),
            selected_function(&prepared).body.len()
        );
    }
    assert!(matches!(
        salsa::Cancelled::catch(AssertUnwindSafe(|| {
            salsa::prepared_source_probe::try_with_preparation(&db, || ())
        })),
        Ok(Ok(()))
    ));
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let Ok(AnalysisOutcome::Complete(Value::Definition(inference))) =
        controlled(&prepared, Request::Definition, &funded())
    else {
        panic!("funded overload retry did not complete");
    };
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "function_known_decorators",
        Some(definition.as_id()),
        &events,
    );
    if completed {
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events,
        );
    } else {
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events,
            )
            .is_some()
        );
    }
    assert_ordinary_overload_and_memo_reuse(OVERLOAD_BODY, &db, &prepared, inference);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn controlled_merge<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    builder: &mut TypeInferenceBuilder<'db, '_>,
    incoming: &FunctionDecoratorInference<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<()>, AnalysisFailure> {
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
            let effects = SourceEffects::new(&access, session.program());
            effects.merge_decorator_results(builder, incoming).await
        })
    })
}

fn populated_source() -> String {
    let mut source = String::from("from typing import Any, cast\n");
    for (name, count) in [("seed", 1), ("target", 8)] {
        for index in 0..count {
            source.push_str(&format!("{name}_Alias{index} = int | list[\"{name}_Alias{index}\"]\ndef {name}_call{index}(value: Any) -> Any: ...\n"));
        }
        for index in 0..count {
            source.push_str(&format!("@cast({name}_Alias{index}, {name}_call{index}({name}_bound{index} := True))\n@{name}_missing{index}\n@{name}_suppressed{index}  # ty: ignore[unresolved-reference]\n"));
        }
        source.push_str(&format!("def {name}(): ...\n"));
    }
    source
}

fn storage_builder<'db, 'ast>(
    db: &'db TestDb,
    prepared: &'ast PreparedAnalysisFile<'db>,
    env: &'ast ProgramEnvironment<'db>,
    seed: &FunctionDecoratorInference<'db>,
) -> TypeInferenceBuilder<'db, 'ast> {
    let file = prepared.program_file();
    let mut builder = TypeInferenceBuilder::new(
        db,
        env,
        InferenceRegion::Definition(definition(prepared)),
        file.file(db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    );
    builder.function_decorator_test_allow_discard();
    builder.extend_function_decorator_inference(seed);
    builder
}

#[test]
fn populated_merge_preserves_each_collection_and_refuses_before_destination_mutation() {
    // Ordinary inference supplies these decorator results because the controlled path does not
    // yet support their walrus bindings, recursive aliases, and unresolved-name diagnostics.
    // These payloads test merge storage only; cold definition completion is above.
    let db = database(&populated_source());
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let seed_function = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_function_def_stmt)
        .find(|function| function.name.id == "seed")
        .unwrap();
    let seed = function_known_decorators(
        &db,
        prepared
            .semantic_index()
            .expect_single_definition(seed_function),
    );
    let incoming = function_known_decorators(&db, definition);
    for payload in [seed, incoming] {
        assert!(payload.expression_types().len() > 0);
        assert!(payload.bindings().len() > 0);
        assert!(!payload.called_functions().is_empty());
        assert!(!payload.implicit_aliases().is_empty());
        let (diagnostics, _, suppressions, _) = payload.diagnostics().storage();
        assert!(diagnostics > 0);
        assert!(suppressions > 0);
    }
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut ordinary = storage_builder(&db, &prepared, &env, seed);
    ordinary.extend_function_decorator_inference(incoming);
    let expected = ordinary.function_decorator_test_contents();

    let mut measured = storage_builder(&db, &prepared, &env, seed);
    let before = measured.function_decorator_test_state();
    observations::reset(None);
    let recording = Recording::start(definition, false);
    assert_eq!(
        controlled_merge(&prepared, &mut measured, incoming, &funded()),
        Ok(AnalysisOutcome::Complete(()))
    );
    drop(recording);
    assert_eq!(measured.function_decorator_test_contents(), expected);
    let after = measured.function_decorator_test_state();
    for (before, after) in [
        (before.expressions, after.expressions),
        (before.bindings, after.bindings),
        (before.called, after.called),
        (before.aliases, after.aliases),
        (before.diagnostics, after.diagnostics),
        (before.used_suppressions, after.used_suppressions),
    ] {
        assert!(before.0 > 0);
        assert!(after.0 > before.0);
        assert!(after.1 > before.1);
    }
    let before_merge = funded().semantic_work_limit - snapshot().before[0].remaining;
    assert_cleanup();

    // The only requested allocation after the observed merge boundary belongs to this
    // atomic merge, so the smallest completing byte allowance isolates its admission.
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let mut builder = storage_builder(&db, &prepared, &env, seed);
        let result = controlled_merge(
            &prepared,
            &mut builder,
            incoming,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        match result {
            Ok(AnalysisOutcome::Complete(())) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_cleanup();
    }
    assert!(upper > 0);
    let revision = salsa::plumbing::current_revision(&db);
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: before_merge,
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
        let mut builder = storage_builder(&db, &prepared, &env, seed);
        let before = builder.function_decorator_test_state();
        let contents = builder.function_decorator_test_contents();
        observations::reset(None);
        let recording = Recording::start(definition, false);
        let result = controlled_merge(&prepared, &mut builder, incoming, &policy);
        drop(recording);
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert_eq!(snapshot().before.len(), 1);
        assert!(snapshot().merged.is_empty());
        assert_eq!(builder.function_decorator_test_state(), before);
        assert_eq!(builder.function_decorator_test_contents(), contents);
        assert_cleanup();
        assert_eq!(
            controlled_merge(&prepared, &mut builder, incoming, &funded()),
            Ok(AnalysisOutcome::Complete(()))
        );
        assert_eq!(builder.function_decorator_test_contents(), expected);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}
