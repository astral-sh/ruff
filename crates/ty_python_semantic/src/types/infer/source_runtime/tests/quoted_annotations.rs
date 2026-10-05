//! Checks admission and retained syntax while quoted type expressions use canonical children.
//! Typing and diagnostic details remain specified by the string and deferred-annotation mdtests.

use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use test_case::test_case;
use ty_python_core::node_key::NodeKey;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::analysis::QuotedAnnotationOperation;
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::infer::builder::DeferredExpressionState;
use crate::types::infer::{InferenceFlags, infer_deferred_types};

const FORWARD: &str = "def choose(value: \"Annotation\"): ...\nclass Annotation: pass\n";
const CAPACITY: usize = 128;
const STORAGE_CAPACITY: usize = 32;

/// Identifies production steps whose normal admission can refuse the quoted continuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    ParsedRetained,
    FrameInstalled,
    OriginalKeyStored,
    FlagsTransferred,
}

/// Identifies the builder state immediately before a quoted child, after its finish, or on abort.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum StateStage {
    ParsedChild,
    QuoteFinished,
    Restored,
}

/// Retains the deferred lookup identity without borrowing a builder or parsed tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Deferred {
    None,
    Deferred,
    String(NodeKey),
}

/// Records flags and lookup keys independently of the lifetime of their parsed syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct State {
    stage: StateStage,
    original: ExpressionNodeKey,
    enclosing: NodeKey,
    flags: InferenceFlags,
    deferred: Deferred,
}

/// Records production transitions; none of these events requests suspension or changes admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Before(Stage),
    After(Stage),
    State(State),
    StorageCreated(usize),
    StorageBegin(usize),
    StorageComplete(usize),
    ParsedIn(usize),
    ChildStarted,
    ChildPending,
}

/// Relates local events to the existing journal of actual canonical child destruction.
#[derive(Clone, Copy, Debug)]
struct Entry {
    event: Event,
    child_events: usize,
}

/// Uses fixed storage so observation itself does not allocate during controlled inference.
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    entries: [Option<Entry>; CAPACITY],
    count: usize,
    active_storage: [bool; STORAGE_CAPACITY],
    parsed_storage: [bool; STORAGE_CAPACITY],
    next_storage: usize,
    watched_definition: Option<salsa::Id>,
    watched_key: Option<salsa::DatabaseKeyIndex>,
    overflowed: bool,
}

impl Snapshot {
    const fn new() -> Self {
        Self {
            entries: [None; CAPACITY],
            count: 0,
            active_storage: [false; STORAGE_CAPACITY],
            parsed_storage: [false; STORAGE_CAPACITY],
            next_storage: 0,
            watched_definition: None,
            watched_key: None,
            overflowed: false,
        }
    }

    fn record(&mut self, event: Event) {
        let children = lifetime_observations::snapshot();
        self.overflowed |= children.overflowed;
        if let Some(slot) = self.entries.get_mut(self.count) {
            *slot = Some(Entry {
                event,
                child_events: children.count,
            });
            self.count += 1;
        } else {
            self.overflowed = true;
        }
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries[..self.count].iter().flatten()
    }

    fn count(&self, event: Event) -> usize {
        self.entries().filter(|entry| entry.event == event).count()
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static SNAPSHOT: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
}

/// Restricts observations to one controlled request, including its cleanup.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start() -> Self {
        assert!(!ACTIVE.replace(true));
        SNAPSHOT.set(Snapshot::new());
        lifetime_observations::reset();
        Self
    }

    fn watch(&self, db: &dyn Db, definition: Definition<'_>) {
        let mut snapshot = SNAPSHOT.get();
        snapshot.watched_definition = Some(definition.as_id());
        snapshot.watched_key =
            Some(definition_inference_ingredient(db).database_key_index(definition.as_id()));
        SNAPSHOT.set(snapshot);
    }

    fn snapshot(&self) -> Snapshot {
        let snapshot = SNAPSHOT.get();
        assert!(!snapshot.overflowed, "{snapshot:?}");
        snapshot
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ACTIVE.set(false);
        lifetime_observations::stop();
    }
}

/// Records entry immediately before the selected production step's normal admission.
pub(in crate::types::infer) fn observe_before(stage: Stage) {
    if ACTIVE.get() {
        let mut snapshot = SNAPSHOT.get();
        snapshot.record(Event::Before(stage));
        SNAPSHOT.set(snapshot);
    }
}

/// Records completion after the admitted production step, including empty-flags bookkeeping.
pub(in crate::types::infer) fn observe_after(stage: Stage) {
    if ACTIVE.get() {
        let mut snapshot = SNAPSHOT.get();
        snapshot.record(Event::After(stage));
        if stage == Stage::ParsedRetained {
            if let Some(id) = snapshot.active_storage.iter().rposition(|active| *active) {
                snapshot.parsed_storage[id] = true;
                snapshot.record(Event::ParsedIn(id));
            } else {
                snapshot.overflowed = true;
            }
        }
        SNAPSHOT.set(snapshot);
    }
}

/// Captures state at child submission, quote completion, and completed checkpoint restoration.
pub(in crate::types::infer) fn observe_state(
    stage: StateStage,
    original: ExpressionNodeKey,
    enclosing: NodeKey,
    flags: InferenceFlags,
    deferred: DeferredExpressionState,
) {
    if ACTIVE.get() {
        let deferred = match deferred {
            DeferredExpressionState::None => Deferred::None,
            DeferredExpressionState::Deferred => Deferred::Deferred,
            DeferredExpressionState::InStringAnnotation(key) => Deferred::String(key),
        };
        let mut snapshot = SNAPSHOT.get();
        snapshot.record(Event::State(State {
            stage,
            original,
            enclosing,
            flags,
            deferred,
        }));
        SNAPSHOT.set(snapshot);
    }
}

/// Records completed checkpoint restoration using the most recent quote's observed keys.
pub(in crate::types::infer) fn observe_restored(
    flags: InferenceFlags,
    deferred: DeferredExpressionState,
) {
    if ACTIVE.get() {
        let snapshot = SNAPSHOT.get();
        let previous = snapshot.entries[..snapshot.count]
            .iter()
            .rev()
            .flatten()
            .find_map(|entry| match entry.event {
                Event::State(state) => Some(state),
                _ => None,
            });
        if let Some(previous) = previous {
            observe_state(
                StateStage::Restored,
                previous.original,
                previous.enclosing,
                flags,
                deferred,
            );
        }
    }
}

/// Declared before the syntax arena, so its destructor observes completed arena destruction.
#[derive(Debug)]
pub(in crate::types::infer) struct SyntaxDropComplete {
    id: Option<usize>,
}

impl SyntaxDropComplete {
    pub(in crate::types::infer) fn new() -> Self {
        let id = if ACTIVE.get() {
            let mut snapshot = SNAPSHOT.get();
            let id = snapshot.next_storage;
            snapshot.next_storage += 1;
            if let Some(active) = snapshot.active_storage.get_mut(id) {
                *active = true;
                snapshot.record(Event::StorageCreated(id));
                SNAPSHOT.set(snapshot);
                Some(id)
            } else {
                snapshot.overflowed = true;
                SNAPSHOT.set(snapshot);
                None
            }
        } else {
            None
        };
        Self { id }
    }

    pub(in crate::types::infer) const fn id(&self) -> Option<usize> {
        self.id
    }
}

impl Drop for SyntaxDropComplete {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            let mut snapshot = SNAPSHOT.get();
            snapshot.active_storage[id] = false;
            snapshot.record(Event::StorageComplete(id));
            SNAPSHOT.set(snapshot);
        }
    }
}

/// Declared after the syntax arena and before the driver, so destruction starts after driver drain.
#[derive(Debug)]
pub(in crate::types::infer) struct SyntaxDropBegin {
    id: Option<usize>,
}

impl SyntaxDropBegin {
    pub(in crate::types::infer) const fn new(id: Option<usize>) -> Self {
        Self { id }
    }
}

impl Drop for SyntaxDropBegin {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            let mut snapshot = SNAPSHOT.get();
            snapshot.record(Event::StorageBegin(id));
            SNAPSHOT.set(snapshot);
        }
    }
}

/// Observes the watched canonical definition request's actual pending polls with syntax retained.
///
/// The caller waits on a queued canonical reply even when the definition body completes inline.
/// Poll the reply demand unchanged so the observation cannot introduce suspension.
pub(in crate::types::infer) async fn observe_definition_request<F: Future>(
    definition: salsa::Id,
    demand: F,
) -> F::Output {
    let mut demand = std::pin::pin!(demand);
    poll_fn(|context| {
        let result = demand.as_mut().poll(context);
        if result.is_pending() && ACTIVE.get() {
            let mut snapshot = SNAPSHOT.get();
            if snapshot.watched_definition == Some(definition)
                && snapshot
                    .active_storage
                    .iter()
                    .zip(snapshot.parsed_storage)
                    .any(|(active, parsed)| *active && parsed)
            {
                snapshot.record(Event::ChildPending);
                SNAPSHOT.set(snapshot);
            }
        }
        result
    })
    .await
}

/// Records execution of the exact watched canonical definition key without inferring an input.
fn observe_query_entry(event: &salsa::EventKind) {
    if let salsa::EventKind::WillExecute { database_key } = *event
        && ACTIVE.get()
    {
        let mut snapshot = SNAPSHOT.get();
        if snapshot.watched_key == Some(database_key) {
            snapshot.record(Event::ChildStarted);
            SNAPSHOT.set(snapshot);
        }
    }
}

/// Requests the function's existing canonical deferred inference with no semantic prewarming.
#[derive(Clone, Copy, Debug)]
struct Request<'db>(Definition<'db>);

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = &'db DefinitionInference<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.deferred_definition(self.0).await
    }
}

/// Creates a fresh cold database with observation of canonical query execution enabled.
fn database(source: &str) -> TestDb {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .with_salsa_event_callback(observe_query_entry)
        .build()
        .unwrap()
}

/// Finds the fixture's function syntax without querying its inferred type.
fn function<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::StmtFunctionDef {
    prepared.parsed_module().syntax().body[0]
        .as_function_def_stmt()
        .expect("fixture function")
}

/// Creates the canonical deferred request from the fixture's prepared function syntax.
fn request<'db>(prepared: &PreparedAnalysisFile<'db>) -> Request<'db> {
    Request(
        prepared
            .semantic_index()
            .expect_single_definition(function(prepared)),
    )
}

fn annotation<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::Expr {
    function(prepared).parameters.args[0]
        .annotation()
        .expect("fixture annotation")
}

/// Selects the later class definition whose canonical inference the forward annotation demands.
fn annotation_definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    let class = prepared.parsed_module().syntax().body[1]
        .as_class_def_stmt()
        .expect("fixture annotation class");
    prepared.semantic_index().expect_single_definition(class)
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_unpublished(db: &dyn Db, definition: Definition<'_>) {
    assert!(
        FinalSourceMemo::certify(
            db,
            deferred_definition_inference_ingredient(db),
            definition.as_id()
        )
        .is_err()
    );
}

/// States whether a quote occurs inside another type-expression operation, such as a tuple element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Nesting {
    TopLevel,
    ContainsNested,
}

/// Nested and composed quoted expressions complete cold and store the ordinary result on the source string.
#[test_case("\"Annotation\"", 1, Nesting::TopLevel; "forward class")]
#[test_case("\"'Annotation'\"", 2, Nesting::TopLevel; "nested quote")]
#[test_case("\"Annotation | None\"", 1, Nesting::TopLevel; "union")]
#[test_case("\"tuple['Annotation', None]\"", 2, Nesting::ContainsNested; "tuple with nested quote")]
fn cold_quoted_forms_preserve_lookup_state_and_original_result(
    quoted: &str,
    parsed: usize,
    nested: Nesting,
) {
    let source = format!("def choose(value: {quoted}): ...\nclass Annotation: pass\n");
    let db = database(&source);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    recording.watch(&db, annotation_definition(&prepared));
    let result = controlled_member_operation(&prepared, request(&prepared), &funded());
    let snapshot = recording.snapshot();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(inference)) = result else {
        panic!("cold quoted annotation: {result:?}");
    };
    let original = annotation(&prepared);
    assert!(snapshot.count(Event::ChildStarted) > 0, "{snapshot:?}");
    assert!(snapshot.count(Event::ChildPending) > 0, "{snapshot:?}");
    assert_eq!(snapshot.count(Event::After(Stage::ParsedRetained)), parsed);
    let children: Vec<_> = snapshot
        .entries()
        .filter_map(|entry| match entry.event {
            Event::State(state) if state.stage == StateStage::ParsedChild => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(children.len(), parsed);
    assert!(
        children
            .iter()
            .all(|state| state.enclosing == NodeKey::from_node(original))
    );
    assert!(
        children
            .iter()
            .all(|state| !state.flags.contains(InferenceFlags::IN_TYPE_EXPRESSION))
    );
    assert_eq!(
        children.iter().any(|state| state
            .flags
            .contains(InferenceFlags::IN_NESTED_TYPE_EXPRESSION)),
        nested == Nesting::ContainsNested
    );
    assert_eq!(
        snapshot.count(Event::After(Stage::FlagsTransferred)),
        parsed
    );
    let finished: Vec<_> = snapshot
        .entries()
        .filter_map(|entry| match entry.event {
            Event::State(state) if state.stage == StateStage::QuoteFinished => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(finished.len(), parsed);
    assert!(children.iter().all(|child| finished.iter().any(|finish| {
        finish.original == child.original
            && finish.enclosing == child.enclosing
            && finish.deferred == child.deferred
            && finish.flags == child.flags | InferenceFlags::IN_TYPE_EXPRESSION
    })));
    let ordinary_db = database(&source);
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary = infer_deferred_types(&ordinary_db, request(&ordinary_prepared).0);
    let ordinary_annotation = annotation(&ordinary_prepared);
    assert_eq!(
        inference
            .expression_type(original)
            .display(&db, &ProgramEnvironment::from_file(prepared.program_file()))
            .to_string(),
        ordinary
            .expression_type(ordinary_annotation)
            .display(
                &ordinary_db,
                &ProgramEnvironment::from_file(ordinary_prepared.program_file()),
            )
            .to_string()
    );
    assert!(inference.try_expression_type(original).is_some());
    assert_eq!(
        inference.type_expression_flags(original),
        ordinary.type_expression_flags(ordinary_annotation)
    );
    assert_cleanup();
}

/// After cancellation is requested during canonical child creation, the child drains before
/// the retained syntax arena starts destruction.
/// Fixpoint masking lets the deferred query finish and publish before cancellation reaches its caller.
/// A funded same-revision retry reuses that final memo and matches the complete ordinary result.
#[test]
fn cancelled_child_drains_before_parsed_syntax_and_retries() {
    let db = database(FORWARD);
    let prepared = prepare(&db);
    let definition = annotation_definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    observations::cancel_definition_creation(definition.as_id());
    let recording = Recording::start();
    recording.watch(&db, definition);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request(&prepared), &funded())
    }));
    let snapshot = recording.snapshot();
    let children = lifetime_observations::snapshot();
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(snapshot.count(Event::ChildStarted) > 0, "{snapshot:?}");
    assert!(snapshot.count(Event::ChildPending) > 0, "{snapshot:?}");
    let storage = snapshot
        .entries()
        .find_map(|entry| match entry.event {
            Event::ParsedIn(id) => Some(id),
            _ => None,
        })
        .expect("parsed syntax was retained");
    let child = children.events[..children.count].iter().rposition(|event| {
        matches!(event, Some(lifetime_observations::Event::SourceChildDropped { definition: Some(actual) }) if *actual == definition.as_id())
    }).expect("watched canonical child was destroyed");
    let entries: Vec<_> = snapshot.entries().collect();
    let begin = entries
        .iter()
        .position(|entry| entry.event == Event::StorageBegin(storage))
        .expect("arena destruction began");
    let complete = entries
        .iter()
        .position(|entry| entry.event == Event::StorageComplete(storage))
        .expect("arena backing was released");
    assert!(
        child < entries[begin].child_events,
        "{snapshot:?}; {children:?}"
    );
    assert!(begin < complete, "{snapshot:?}");
    assert_eq!(snapshot.count(Event::After(Stage::OriginalKeyStored)), 1);
    assert_eq!(snapshot.count(Event::After(Stage::FlagsTransferred)), 1);
    assert_eq!(
        snapshot
            .entries()
            .filter(|entry| matches!(entry.event, Event::State(State {
                stage: StateStage::QuoteFinished,
                ..
            })))
            .count(),
        1,
        "{snapshot:?}"
    );
    let deferred = request(&prepared).0;
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            deferred.as_id(),
        )
        .is_ok()
    );
    assert_cleanup();

    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    let retry = controlled_member_operation(&prepared, Request(deferred), &funded());
    let Ok(AnalysisOutcome::Complete(inference)) = retry else {
        panic!("same-revision quoted annotation retry: {retry:?}");
    };
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(deferred.as_id()),
        &event_db.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();

    // Run the ordinary producer directly after the retry so this compares all result fields
    // without obtaining the same completed memo through the ordinary query wrapper.
    //
    let program_file = prepared.program_file();
    let env = ProgramEnvironment::from_file(program_file);
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Deferred(deferred),
        program_file.file(&db),
        program_file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_definition(deferred);
    assert_eq!(inference, &ordinary);
}

/// Selects one resource limit while leaving the other at the established funded allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Measures whether a fresh cold request completes the selected step under a real resource limit.
fn reaches(stage: Stage, policy: &AnalysisPolicy) -> bool {
    let db = database(FORWARD);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let _result = controlled_member_operation(&prepared, request(&prepared), policy);
    let after = recording.snapshot().count(Event::After(stage));
    drop(recording);
    assert_cleanup();
    after > 0
}

/// Exhaustion refuses the selected step, leaves the canonical parent unpublished, and permits retry.
/// The flags cases cover empty-flag bookkeeping; this fixture does not insert a nonempty flags entry.
#[test_case(Stage::ParsedRetained, Resource::Work; "parsed work")]
#[test_case(Stage::ParsedRetained, Resource::Bytes; "parsed bytes")]
#[test_case(Stage::FrameInstalled, Resource::Work; "frame work")]
#[test_case(Stage::FrameInstalled, Resource::Bytes; "frame bytes")]
#[test_case(Stage::OriginalKeyStored, Resource::Work; "key work")]
#[test_case(Stage::OriginalKeyStored, Resource::Bytes; "key bytes")]
#[test_case(Stage::FlagsTransferred, Resource::Work; "flags work")]
#[test_case(Stage::FlagsTransferred, Resource::Bytes; "flags bytes")]
fn admission_refusal_precedes_mutation_and_retries(stage: Stage, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches(stage, &resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches(stage, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(FORWARD);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request(&prepared), &resource.policy(low));
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert!(snapshot.count(Event::Before(stage)) > 0, "{snapshot:?}");
    assert_eq!(snapshot.count(Event::After(stage)), 0, "{snapshot:?}");
    assert_unpublished(&db, request(&prepared).0);
    assert_cleanup();
    observations::reset(None);
    assert!(matches!(
        controlled_member_operation(&prepared, request(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Rejected string syntax reports its precise unavailable diagnostic without publishing an unknown type.
#[test_case("r\"Annotation\"", QuotedAnnotationOperation::RawStringDiagnostic; "raw")]
#[test_case("\"Anno\" \"tation\"", QuotedAnnotationOperation::ConcatenatedStringDiagnostic; "concatenated")]
#[test_case("\"Annota\\x74ion\"", QuotedAnnotationOperation::EscapeDiagnostic; "escaped")]
#[test_case("\"Annotation [\"", QuotedAnnotationOperation::SyntaxDiagnostic; "syntax")]
fn rejected_quoted_syntax_preserves_diagnostic_category(
    quoted: &str,
    operation: QuotedAnnotationOperation,
) {
    let source = format!("def choose(value: {quoted}): ...\nclass Annotation: pass\n");
    let db = database(&source);
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(&prepared, request(&prepared), &funded()),
        Ok(unavailable(OperationId::QuotedAnnotation(operation)))
    );
    assert_unpublished(&db, request(&prepared).0);
    assert_cleanup();
}

/// Valid parsed syntax with an unsupported semantic descendant preserves that descendant's refusal.
#[test]
fn unsupported_parsed_expression_preserves_legacy_refusal_and_restores_state() {
    let db = database("def choose(value: \"lambda: None\"): ...\n");
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    assert_eq!(
        controlled_member_operation(&prepared, request(&prepared), &funded()),
        Ok(unavailable(OperationId::TypeExpressionLegacy))
    );
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(snapshot.count(Event::After(Stage::ParsedRetained)), 1);
    assert_eq!(snapshot.count(Event::After(Stage::FlagsTransferred)), 0);
    let restored = snapshot
        .entries()
        .find_map(|entry| match entry.event {
            Event::State(state) if state.stage == StateStage::Restored => Some(state),
            _ => None,
        })
        .expect("aborted local invocation restored its builder");
    assert!(!restored.flags.intersects(
        InferenceFlags::IN_TYPE_EXPRESSION | InferenceFlags::IN_NESTED_TYPE_EXPRESSION
    ));
    assert_eq!(restored.deferred, Deferred::None);
    assert_unpublished(&db, request(&prepared).0);
    assert_cleanup();
}
