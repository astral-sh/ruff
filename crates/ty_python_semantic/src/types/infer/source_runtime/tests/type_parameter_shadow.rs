//! Controls the admitted scan for function type parameters that shadow enclosing variables.
//!
//! Definition and constructor controls start with cold canonical inference. Separate Lexical and
//! alias controls call binding effects directly to check their remaining refusals. The observations
//! distinguish scan progress, real child suspension, and resource admission, which mdtests cannot
//! expose.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::ownership::observations as owner_observations;
use crate::types::call::invocation::{InvocationContext, InvocationEffects};
use crate::types::callable::CallableTypeKind;
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::SourceDefinitionEffect;
use crate::types::signatures::ReturnCallableTypeVarScope;

const TOP_LEVEL: &str = "def target[T](): ...\n";
const IN_CLASS: &str = "class Product:\n    def target[T](self): ...\n";
const IN_FUNCTION: &str = "def outer():\n    def target[T](): ...\n";
const DEFERRED_CONSTRUCTOR: &str = "class Annotation: pass\ndef outer(value: Annotation):\n    def decorate[T](cls: T) -> T: ...\n    @decorate\n    class Target: pass\n    class Product:\n        def __new__(cls): ...\n        def __init__(self, value: Target): ...\n";

/// Identifies a production admission boundary; observations do not change its allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    Parameter,
    Ancestor,
    Variable,
    NameComparison,
    Report,
    Future,
}

impl Stage {
    /// Selects the fixed counter slot for this boundary.
    const fn index(self) -> usize {
        match self {
            Self::Parameter => 0,
            Self::Ancestor => 1,
            Self::Variable => 2,
            Self::NameComparison => 3,
            Self::Report => 4,
            Self::Future => 5,
        }
    }
}

/// Records progress and work debited by one production operation, including fixed transfers.
#[derive(Clone, Copy, Debug, Default)]
struct Boundary {
    before: usize,
    after: usize,
    remaining_before: Option<usize>,
    admitted_work: usize,
}

/// Records actual release of a prepared `__new__` binding tree's backing storage.
#[derive(Clone, Copy, Debug)]
struct OwnerRetirement {
    id: usize,
    begin: usize,
    complete: Option<usize>,
}

/// Collects a request's scan progress alongside its canonical child and owner lifetimes.
#[derive(Clone, Copy, Debug, Default)]
struct Journal {
    boundaries: [Boundary; 6],
    live_scans: usize,
    entered_scans: usize,
    pending_with_scan: usize,
    child_key: Option<salsa::DatabaseKeyIndex>,
    scans_at_child: usize,
    builders_at_child: usize,
    child_entry: Option<usize>,
    retirement: Option<OwnerRetirement>,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records arrival before the selected production operation requests admission.
pub(in crate::types::infer) fn observe_before(db: &dyn Db, stage: Stage) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            let boundary = &mut journal.boundaries[stage.index()];
            boundary.before += 1;
            boundary.remaining_before =
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
    });
}

/// Records successful admission and the operation's completed state change.
pub(in crate::types::infer) fn observe_after(db: &dyn Db, stage: Stage) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            let boundary = &mut journal.boundaries[stage.index()];
            boundary.after += 1;
            if let Some(before) = boundary.remaining_before
                && let Some(after) = salsa::attempt_probe::remaining_allowance_for_diagnostics(db)
            {
                boundary.admitted_work += before - after;
            }
        }
    });
}

/// Observes the lifetime of an executing scan, without claiming that its future was deallocated.
#[derive(Debug)]
pub(in crate::types::infer) struct ScanLifetime;

/// Marks entry after the scan future is admitted and before its parameter cursor advances.
pub(in crate::types::infer) fn scan_enter(_db: &dyn Db) -> ScanLifetime {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.live_scans += 1;
            journal.entered_scans += 1;
        }
    });
    ScanLifetime
}

impl Drop for ScanLifetime {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| {
            if let Some(journal) = journal {
                journal.live_scans -= 1;
            }
        });
    }
}

/// Observes a real `Pending` result while the production scan is still executing.
fn observe_pending() {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.live_scans > 0
        {
            journal.pending_with_scan += 1;
        }
    });
}

/// Polls the scan unchanged and records each actual `Pending` result while it is executing.
/// Canonical children can run between polls of the outer request, so observing only that request
/// does not capture suspension of the scan inside its independently polled definition provider.
pub(in crate::types::infer) async fn observe_scan_polling<F: Future>(scan: F) -> F::Output {
    let mut scan = std::pin::pin!(scan);
    poll_fn(|context| {
        let result = scan.as_mut().poll(context);
        if result.is_pending() {
            observe_pending();
        }
        result
    })
    .await
}

/// Records canonical child entry while the enclosing scan and builders remain alive.
fn observe_query_entry(event: &salsa::EventKind) {
    let salsa::EventKind::WillExecute { database_key } = *event else {
        return;
    };
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.child_key == Some(database_key)
        {
            journal.scans_at_child = journal.live_scans;
            journal.builders_at_child = observations::counts().0;
            journal.child_entry = Some(lifetime_observations::snapshot().count);
        }
    });
}

/// Records both destructor entry and completed storage release for the constructor owner.
fn observe_owner_retirement(event: owner_observations::RetirementEvent<'_>) {
    if event.constructor_kind != Some(ConstructorCallableKind::New) {
        return;
    }
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            let count = lifetime_observations::snapshot().count;
            match event.phase {
                owner_observations::RetirementPhase::Begin => {
                    if journal.retirement.is_none() {
                        journal.retirement = Some(OwnerRetirement {
                            id: event.id,
                            begin: count,
                            complete: None,
                        });
                    }
                }
                owner_observations::RetirementPhase::Complete => {
                    if let Some(retirement) = &mut journal.retirement
                        && retirement.id == event.id
                    {
                        retirement.complete = Some(count);
                    }
                }
            }
        }
    });
}

/// Installs observations for one request, including its unwinding and owner cleanup.
#[derive(Debug)]
struct Recording {
    previous_owner: Option<for<'db> fn(owner_observations::RetirementEvent<'db>)>,
}

impl Recording {
    /// Starts with no semantic scan or owner observations from an earlier request.
    fn start(child_key: Option<salsa::DatabaseKeyIndex>) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal {
                child_key,
                ..Journal::default()
            });
        });
        lifetime_observations::reset();
        Self {
            previous_owner: owner_observations::set_retirement_observer(Some(
                observe_owner_retirement,
            )),
        }
    }

    /// Copies fixed observation counters without retaining production-owned values.
    fn snapshot(&self) -> Journal {
        JOURNAL.with_borrow(|journal| journal.unwrap())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        owner_observations::set_retirement_observer(self.previous_owner);
        lifetime_observations::stop();
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
}

/// Selects definition inference, constructor preparation, or a still-unavailable context entry.
#[derive(Clone, Copy, Debug)]
enum Request<'db> {
    Definition(Definition<'db>),
    Constructor(Definition<'db>),
    LexicalFunction(Definition<'db>),
    Alias(Definition<'db>),
}

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<()>
    where
        'db: 'run,
    {
        match self {
            Self::Definition(definition) => {
                access.definition(definition).await?;
            }
            Self::Constructor(definition) => {
                let inference = access.definition(definition).await?;
                let endpoint = access.endpoint();
                let class = endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        inference
                            .original_class_type(definition)
                            .map(Type::ClassLiteral)
                            .ok_or(RunError::Contract("constructor fixture is not a class"))
                    })
                    .await;
                let effects = SourceEffects::new(access, program);
                let env = ProgramEnvironment::from_program(program);
                let guard = InvocationEffects::new_guard(&effects).await?;
                let arguments = CallArguments::default();
                InvocationEffects::prepare(
                    &effects,
                    InvocationContext {
                        db: access.db(),
                        env: &env,
                        arguments: &arguments,
                    },
                    class,
                    &guard,
                )
                .await?;
            }
            Self::LexicalFunction(definition) => {
                TypeVarBindingEffects::function_context(
                    &SourceEffects::new(access, program),
                    definition,
                    ReturnCallableTypeVarScope::Lexical,
                )
                .await?;
            }
            Self::Alias(definition) => {
                TypeVarBindingEffects::alias_context(
                    &SourceEffects::new(access, program),
                    definition,
                )
                .await?;
            }
        }
        Ok(())
    }
}

/// Creates a fresh database and fixture file without requesting semantic inference.
fn database(source: &str) -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", source)
        .with_salsa_event_callback(observe_query_entry)
        .build()
        .unwrap()
}

/// Finds a named function or class through its enclosing syntax, without inferring definitions.
fn definition<'db>(prepared: &PreparedAnalysisFile<'db>, path: &[&str]) -> Definition<'db> {
    let mut suite = prepared.parsed_module().syntax().body.as_slice();
    let mut selected = None;
    for name in path {
        let statement = suite.iter().find(|statement| match statement {
            Stmt::FunctionDef(function) => function.name.as_str() == *name,
            Stmt::ClassDef(class) => class.name.as_str() == *name,
            _ => false,
        });
        match statement {
            Some(Stmt::FunctionDef(function)) => {
                selected = Some(prepared.semantic_index().expect_single_definition(function));
                suite = &function.body;
            }
            Some(Stmt::ClassDef(class)) => {
                selected = Some(prepared.semantic_index().expect_single_definition(class));
                suite = &class.body;
            }
            _ => panic!("fixture has no definition at {path:?}"),
        }
    }
    selected.expect("fixture definition path is empty")
}

/// Checks completed provider lifetimes and the absence of an active execution attempt.
fn assert_drained() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Runs one canonical definition request at the unchanged resource ceilings and checks cleanup.
fn funded_definition<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
) -> Result<AnalysisOutcome<()>, AnalysisFailure> {
    let result = controlled_member_operation(prepared, Request::Definition(definition), &funded());
    assert_drained();
    result
}

/// A no-match scan completes canonical function metadata without requesting its generic signature.
#[test_case::test_case(TOP_LEVEL, &["target"]; "top level")]
#[test_case::test_case(IN_CLASS, &["Product", "target"]; "nongeneric class")]
#[test_case::test_case(IN_FUNCTION, &["outer", "target"]; "nongeneric function")]
fn cold_no_match_metadata_completes(source: &str, path: &[&str]) {
    let db = database(source);
    let prepared = prepare(&db);
    let target = definition(&prepared, path);
    observations::reset(None);
    let recording = Recording::start(None);
    let captured = capture(&db, || funded_definition(&prepared, target)).unwrap();
    let journal = recording.snapshot();
    drop(recording);
    assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(())));
    captured.check_root_reads().unwrap();
    assert!(journal.entered_scans > 0, "{journal:?}");
    assert_eq!(journal.live_scans, 0);
    assert_eq!(journal.boundaries[Stage::Parameter.index()].after, 2);
    assert_eq!(journal.boundaries[Stage::Report.index()].before, 0);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            target.as_id()
        )
        .is_ok()
    );
    assert_drained();
}

/// A populated enclosing context completes metadata for a nonmatching parameter name.
/// A matching name stops precisely at the first shadow report.
#[test_case::test_case("from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Product(Generic[T]):\n    def target[U](self): ...\n", &["Product", "target"], None; "nonmatching legacy class name")]
#[test_case::test_case("from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Product(Generic[T]):\n    def target[T](self): ...\n", &["Product", "target"], Some(OperationId::FunctionTypeParameterShadowDiagnostic); "matching legacy class name")]
#[test_case::test_case("def outer[T]():\n    def target[U](): ...\n", &["outer", "target"], None; "nonmatching PEP 695 function name")]
fn cold_populated_context_preserves_name_matching(
    source: &str,
    path: &[&str],
    refusal: Option<OperationId>,
) {
    let db = database(source);
    let prepared = prepare(&db);
    let target = definition(&prepared, path);
    observations::reset(None);
    let recording = Recording::start(None);
    let captured = capture(&db, || funded_definition(&prepared, target)).unwrap();
    let journal = recording.snapshot();
    drop(recording);
    let expected = refusal
        .map(unavailable)
        .unwrap_or(AnalysisOutcome::Complete(()));
    assert_eq!(captured.value, Ok(expected));
    captured.check_root_reads().unwrap();
    assert!(
        journal.boundaries[Stage::Variable.index()].after > 0,
        "{journal:?}"
    );
    assert!(
        journal.boundaries[Stage::NameComparison.index()].after > 0,
        "{journal:?}"
    );
    assert_eq!(
        journal.boundaries[Stage::Report.index()].before > 0,
        refusal.is_some()
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            target.as_id()
        )
        .is_ok(),
        refusal.is_none()
    );
    assert_eq!(journal.live_scans, 0);
    assert_drained();
}

/// Public function contexts use canonical definition and effective last-signature queries.
/// The staticmethod case also checks its stored undecorated function and descriptor kind.
#[test_case::test_case(""; "ordinary function")]
#[test_case::test_case("@staticmethod\n"; "staticmethod declaration")]
fn public_function_context_uses_definition_and_last_signature(decorator: &str) {
    let db = database(&format!(
        "{decorator}def outer():\n    def target[T](): ...\n"
    ));
    let prepared = prepare(&db);
    let outer = definition(&prepared, &["outer"]);
    let target = definition(&prepared, &["outer", "target"]);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        funded_definition(&prepared, target),
        Ok(AnalysisOutcome::Complete(()))
    );
    let events = events_db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_definition_types",
            Some(outer.as_id()),
            &events
        )
        .is_some()
    );
    let inference = infer_definition_types(&db, outer);
    let function = inference.function_type(outer).unwrap();
    if !decorator.is_empty() {
        assert_eq!(
            inference
                .undecorated_type()
                .and_then(Type::as_function_literal),
            Some(function),
        );
        let declared = inference
            .inferred_declaration(outer)
            .declared()
            .unwrap()
            .inner_type()
            .as_function_literal()
            .unwrap();
        assert_eq!(declared, function);
        assert_eq!(function.descriptor_kind(&db), None);
        assert_eq!(
            function.callable_type_kind(&db),
            CallableTypeKind::StaticMethodLike,
        );
    }
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_last_definition_signature_ingredient(&db),
            function.as_id()
        )
        .is_ok()
    );
    let signature_key =
        function_last_definition_signature_ingredient(&db).database_key_index(function.as_id());
    assert!(events.iter().any(|event| matches!(
        event.kind,
        salsa::EventKind::WillExecute { database_key } if database_key == signature_key
    )));
    assert_drained();
}

/// Cold method metadata compares `U` with the enclosing class's `T` without reporting shadowing.
/// Completed definition and class-context queries retain their exact keys on same-revision reuse.
#[test]
fn pep695_class_context_completes_shadow_scan_and_reuses_canonical_memos() {
    let db = database("class Product[T]:\n    def target[U](self): ...\n");
    let prepared = prepare(&db);
    let class_definition = definition(&prepared, &["Product"]);
    let target = definition(&prepared, &["Product", "target"]);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    let captured = capture(&db, || funded_definition(&prepared, target)).unwrap();
    let journal = recording.snapshot();
    drop(recording);
    assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(())));
    captured.check_root_reads().unwrap();
    assert!(journal.entered_scans > 0, "{journal:?}");
    assert!(
        journal.boundaries[Stage::Variable.index()].after > 0,
        "{journal:?}"
    );
    assert!(
        journal.boundaries[Stage::NameComparison.index()].after > 0,
        "{journal:?}"
    );
    assert_eq!(journal.boundaries[Stage::Report.index()].before, 0);
    assert_eq!(journal.live_scans, 0);
    for definition in [target, class_definition] {
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
    }
    let class = infer_definition_types(&db, class_definition)
        .original_class_type(class_definition)
        .and_then(ClassLiteral::as_static)
        .expect("canonical Product class");
    let target_function = infer_definition_types(&db, target)
        .function_type(target)
        .expect("completed target function metadata");
    let inner = pep695_generic_context_ingredient(&db);
    let outer = static_class_generic_context_ingredient(&db);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, inner, class.as_id()).is_ok());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, outer, class.as_id()).is_ok());
    for key in [
        inner.database_key_index(class.as_id()),
        outer.database_key_index(class.as_id()),
    ] {
        assert!(captured.reads.iter().any(|read| read.key == key));
    }
    let context = class.pep695_generic_context(&db).expect("class context");
    let variables = context.variables(&db).collect::<Vec<_>>();
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name(&db).as_str(), "T");
    assert_eq!(variables[0].kind(&db), TypeVarKind::Pep695TypeVar);
    assert_eq!(
        variables[0].binding_context(&db),
        crate::types::BindingContext::Definition(class_definition)
    );
    assert_drained();

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(None);
    assert_eq!(
        funded_definition(&prepared, target),
        Ok(AnalysisOutcome::Complete(()))
    );
    let retry_journal = recording.snapshot();
    drop(recording);
    assert_eq!(retry_journal.entered_scans, 0);
    assert_eq!(
        infer_definition_types(&db, target).function_type(target),
        Some(target_function)
    );
    assert_eq!(class.pep695_generic_context(&db), Some(context));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(target.as_id()),
        &events,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(class_definition.as_id()),
        &events,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "static_class_generic_context",
        Some(class.as_id()),
        &events,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "pep695_generic_context_inner",
        Some(class.as_id()),
        &events,
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
}

/// Lexical function lookup retains its distinct refusal instead of using the Public query.
#[test]
fn lexical_function_context_remains_unavailable() {
    let db = database("def outer(): ...\n");
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request::LexicalFunction(definition(&prepared, &["outer"])),
            &funded()
        ),
        Ok(unavailable(OperationId::TypeVarBindingFunctionContext))
    );
    assert_drained();
}

/// Alias context construction remains unavailable at the binding effect used by ancestor lookup.
#[test]
fn alias_context_remains_unavailable() {
    let db = database("type Alias[T] = T\n");
    let prepared = prepare(&db);
    let Some(Stmt::TypeAlias(alias)) = prepared.parsed_module().syntax().body.first() else {
        panic!("fixture is not a type alias");
    };
    let definition = prepared.semantic_index().expect_single_definition(alias);
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(&prepared, Request::Alias(definition), &funded()),
        Ok(unavailable(OperationId::TypeVarBindingAliasContext))
    );
    assert_drained();
}

/// Selects the one resource allowance reduced by an admission control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    /// Returns a policy capping the selected resource at `limit` while leaving the other funded.
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

    /// Returns the unchanged ceiling used to bracket the real admission threshold.
    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    /// Returns the production refusal expected when this allowance is exhausted.
    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Measures whether a fresh cold request crosses one real production admission boundary.
fn reaches_boundary(stage: Stage, policy: &AnalysisPolicy) -> bool {
    let db = database(TOP_LEVEL);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    let _result = controlled_member_operation(
        &prepared,
        Request::Definition(definition(&prepared, &["target"])),
        policy,
    );
    let journal = recording.snapshot();
    drop(recording);
    assert_drained();
    journal.boundaries[stage.index()].after > 0
}

/// Admission refuses cursor progress or the actual boxed scan future before construction;
/// the unpublished target then completes on a funded request in the same revision.
#[test_case::test_case(Stage::Parameter, Resource::Work; "parameter work")]
#[test_case::test_case(Stage::Future, Resource::Bytes; "scan future bytes")]
fn resource_refusal_precedes_scan_operation(stage: Stage, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_boundary(stage, &resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_boundary(stage, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(TOP_LEVEL);
    let prepared = prepare(&db);
    let target = definition(&prepared, &["target"]);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    let result = controlled_member_operation(
        &prepared,
        Request::Definition(target),
        &resource.policy(low),
    );
    let journal = recording.snapshot();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    let boundary = journal.boundaries[stage.index()];
    assert!(boundary.before > 0, "{journal:?}");
    assert_eq!(boundary.after, 0, "{journal:?}");
    assert_eq!(journal.live_scans, 0);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            target.as_id()
        )
        .is_err()
    );
    assert_drained();
    assert_eq!(
        funded_definition(&prepared, target),
        Ok(AnalysisOutcome::Complete(()))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
}

/// Measures only the selected scan operations, excluding unrelated child-query work.
fn measured_scan(source: &str, path: &[&str], stage: Stage) -> Boundary {
    let db = database(source);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    assert_eq!(
        funded_definition(&prepared, definition(&prepared, path)),
        Ok(AnalysisOutcome::Complete(()))
    );
    let journal = recording.snapshot();
    drop(recording);
    assert_eq!(journal.live_scans, 0);
    assert_drained();
    journal.boundaries[stage.index()]
}

/// Every extra parameter and ancestor incurs additional admitted cursor work, including walks
/// restarted for later parameters.
#[test_case::test_case(TOP_LEVEL, &["target"], "def target[T, U, V](): ...\n", &["target"], Stage::Parameter; "parameter count")]
#[test_case::test_case(TOP_LEVEL, &["target"], "def target[T, U, V](): ...\n", &["target"], Stage::Ancestor; "ancestor walks per parameter")]
#[test_case::test_case(TOP_LEVEL, &["target"], "class Outer:\n    class Inner:\n        def target[T](self): ...\n", &["Outer", "Inner", "target"], Stage::Ancestor; "ancestor count")]
fn cursor_work_grows_with_scan_size(
    short: &str,
    short_path: &[&str],
    long: &str,
    long_path: &[&str],
    stage: Stage,
) {
    let short = measured_scan(short, short_path, stage);
    let long = measured_scan(long, long_path, stage);
    assert!(long.after > short.after, "short={short:?}; long={long:?}");
    assert!(
        long.admitted_work > short.admitted_work,
        "short={short:?}; long={long:?}"
    );
}

/// Searching a longer populated context charges additional variable advancement work.
#[test]
fn variable_work_grows_with_context_size() {
    let short = "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Product(Generic[T]):\n    def target[Z](self): ...\n";
    let long = "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nU = TypeVar(\"U\")\nV = TypeVar(\"V\")\nclass Product(Generic[T, U, V]):\n    def target[Z](self): ...\n";
    let short = measured_scan(short, &["Product", "target"], Stage::Variable);
    let long = measured_scan(long, &["Product", "target"], Stage::Variable);
    assert!(long.after > short.after, "short={short:?}; long={long:?}");
    assert!(
        long.admitted_work > short.admitted_work,
        "short={short:?}; long={long:?}"
    );
}

/// Same-length names that differ at the final byte charge more comparison work as they grow.
/// These names are borrowed from existing syntax and interned identities; the test adds no buffer
/// to the production scan to make its requested-byte accounting grow.
#[test]
fn name_comparison_work_grows_with_name_length() {
    let source = |prefix: &str| {
        format!(
            "from typing import Generic, TypeVar\n{prefix}A = TypeVar(\"{prefix}A\")\nclass Product(Generic[{prefix}A]):\n    def target[{prefix}B](self): ...\n"
        )
    };
    let short = measured_scan(&source("T"), &["Product", "target"], Stage::NameComparison);
    let long = measured_scan(
        &source("TypeParameterWithALongCommonPrefix"),
        &["Product", "target"],
        Stage::NameComparison,
    );
    assert_eq!(long.after, short.after, "short={short:?}; long={long:?}");
    assert!(
        long.admitted_work > short.admitted_work,
        "short={short:?}; long={long:?}"
    );
}

/// Checks that constructor scopes close and their guard storage retires with no outstanding work.
fn assert_guard_storage_released() {
    let snapshot = lifetime_observations::snapshot();
    assert!(!snapshot.overflowed, "{snapshot:?}");
    assert_eq!(snapshot.active_scopes, 0, "{snapshot:?}");
    let events = &snapshot.events[..snapshot.count];
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Some(lifetime_observations::Event::ScopeOpened(_))))
    );
    let released = events
        .iter()
        .filter_map(|event| match event {
            Some(lifetime_observations::Event::StorageDropped {
                outstanding_removal_weights,
                ..
            }) => Some(outstanding_removal_weights),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!released.is_empty(), "{snapshot:?}");
    assert!(
        released.iter().all(|weights| **weights == [0; 3]),
        "{snapshot:?}"
    );
    assert_drained();
}

/// A shadow scan suspends while the constructor retains a prepared `__new__` binding tree.
/// The initializer's `Target` annotation requests that decorated class, whose `decorate[T]`
/// metadata scans the enclosing `outer(value: Annotation)` signature and requests `Annotation`.
/// Constructor preparation builds the `__new__` tree before requesting initializer signatures,
/// so the tree remains alive until this child drains. The scan completes before the decorator's
/// dataclass-transform metadata lookup refuses; this does not establish constructor completion.
#[test]
fn real_child_suspension_keeps_scan_alive() {
    let db = database(DEFERRED_CONSTRUCTOR);
    let prepared = prepare(&db);
    let child = definition(&prepared, &["Annotation"]);
    observations::reset(None);
    let recording = Recording::start(Some(
        definition_inference_ingredient(&db).database_key_index(child.as_id()),
    ));
    let result = controlled_member_operation(
        &prepared,
        Request::Constructor(definition(&prepared, &["outer", "Product"])),
        &funded(),
    );
    let journal = recording.snapshot();
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::DefinitionBody(
            SourceDefinitionEffect::ClassMetadata,
        )))
    );
    assert!(journal.pending_with_scan > 0, "{journal:?}");
    assert!(journal.scans_at_child > 0, "{journal:?}");
    assert!(journal.builders_at_child > 0, "{journal:?}");
    assert_eq!(journal.live_scans, 0);
    let retirement = journal
        .retirement
        .expect("constructor owner did not retire");
    assert!(
        journal.child_entry.unwrap() <= retirement.begin,
        "{journal:?}"
    );
    assert!(retirement.complete.is_some(), "{journal:?}");
    assert_guard_storage_released();
}

/// Cancelling the `Annotation` child leaves no executing scan and drops that child's provider
/// before releasing the prepared `__new__` binding tree's backing storage. The initializer's
/// `Target` annotation reaches `decorate[T]`'s enclosing-function scan after the tree is built,
/// as in the preceding suspension control. A same-revision retry retains the decorator metadata
/// refusal. If `Annotation` has a completed canonical memo after cancellation, retry does not
/// execute that child again.
#[test]
fn interrupted_child_releases_constructor_storage_before_retry() {
    let db = database(DEFERRED_CONSTRUCTOR);
    let prepared = prepare(&db);
    let child = definition(&prepared, &["Annotation"]);
    let constructor = definition(&prepared, &["outer", "Product"]);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    observations::cancel_definition_creation(child.as_id());
    let recording = Recording::start(Some(
        definition_inference_ingredient(&db).database_key_index(child.as_id()),
    ));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, Request::Constructor(constructor), &funded())
    }));
    let journal = recording.snapshot();
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(journal.pending_with_scan > 0, "{journal:?}");
    assert!(journal.scans_at_child > 0, "{journal:?}");
    assert_eq!(journal.live_scans, 0);
    assert_guard_storage_released();
    let snapshot = lifetime_observations::snapshot();
    let events = &snapshot.events[..snapshot.count];
    let child_dropped = events.iter().rposition(|event| matches!(event, Some(lifetime_observations::Event::SourceChildDropped { definition: Some(actual) }) if *actual == child.as_id())).expect("annotation provider lifetime did not end");
    let retirement = journal
        .retirement
        .expect("prepared constructor owner did not retire");
    let complete = retirement
        .complete
        .expect("constructor backing storage was not released");
    assert!(
        child_dropped < retirement.begin,
        "{journal:?}; {snapshot:?}"
    );
    assert!(retirement.begin <= complete, "{journal:?}");
    let guard_released = events
        .iter()
        .rposition(|event| {
            matches!(
                event,
                Some(lifetime_observations::Event::StorageDropped { .. })
            )
        })
        .expect("constructor guard storage was not released");
    assert!(complete <= guard_released, "{journal:?}; {snapshot:?}");
    let child_published = FinalSourceMemo::certify(
        &db as &dyn Db,
        definition_inference_ingredient(&db),
        child.as_id(),
    )
    .is_ok();
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(None);
    let retry =
        controlled_member_operation(&prepared, Request::Constructor(constructor), &funded());
    drop(recording);
    assert_eq!(
        retry,
        Ok(unavailable(OperationId::DefinitionBody(
            SourceDefinitionEffect::ClassMetadata,
        )))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_guard_storage_released();
    if child_published {
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(child.as_id()),
            &events_db.take_salsa_events(),
        );
    }
}
