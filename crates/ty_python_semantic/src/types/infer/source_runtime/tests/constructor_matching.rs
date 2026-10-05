//! Constructor argument matching stops at the complete binding tree before type checking.
//!
//! Real-source controls infer the callee canonically in a fresh database. Constructed-input
//! controls separately exercise runtime states that a Python mdtest cannot observe.

mod class_context;
mod inherited_context;
pub(in crate::types::infer) mod self_receivers;

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use ty_python_core::SemanticIndex;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::call::bind::constructor_matching::fixtures::{
    TreeShape, build_tree, constructor_snapshots, matching_entry_order, property_entry_ids,
};
use crate::types::call::bind::constructor_matching::observations::{
    self as matching_observations, Event,
};
use crate::types::call::bind::constructor_matching::{
    ConstructorMatchingEffects, ConstructorMatchingOperation, ConstructorMatchingPhase,
};
use crate::types::call::bind::ownership::clone_bindings_with;
use crate::types::call::bind::ownership::observations as owner_observations;
use crate::types::call::bind::{CallableBinding, ConstructorCallableKind};
use crate::types::call::invocation::{InvocationContext, InvocationEffects};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::cyclic::guard_storage::observations as guard_observations;
use crate::types::generics::enclosing_binding_contexts;
use crate::types::mapping::source::observations::{
    self as mapping_observations, OwnedMappingSnapshot,
};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::typevar::constructor_nonce::ConstructorNonceOperation;
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
    TypeVarNonce, TypeVarNonceGenerator, bound_typevar_default_ingredient,
};
use crate::types::{
    BindingContext, BoundTypeVarInstance, GenericContext, MappingOperation, MaterializationOperation,
    SubclassOfType, TypeFormType, TypeMapping, TypeVarBoundOrConstraints, TypeVarVariance,
};

const BOX: &str = "class Box[T]:\n    def __init__(self, value: T) -> None:\n        pass\n\n    def again(self, value: T):\n        Box[T](value)\n\nBox[bool](True)\n";
const INITIALIZER: &str = "class Product:\n    def __init__(self, value): ...\nProduct(True)\n";
const DOWNSTREAM: &str = "class Product:\n    def __new__(cls, value): ...\n    def __init__(self, value): ...\nProduct(True)\n";
const THREE_METHODS: &str = "class Meta(type):\n    def __call__(cls, value): ...\nclass Product(metaclass=Meta):\n    def __new__(cls, value): ...\n    def __init__(self, value): ...\nProduct(True)\n";
const CONSTRUCTED: &str = "class Anchor[T, U]: pass\n";

/// Selects either the module call or the constructor call enclosed by `Box.again`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallSite {
    Module,
    Again,
}

/// Selects the production local-call adapter or invocation adapter with a supplied guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Adapter {
    Local,
    SuppliedGuard,
}

/// Retains the matched tree and canonical callee for an ordinary comparison after cold execution.
#[derive(Debug)]
struct Matched<'db> {
    callable: Type<'db>,
    bindings: Bindings<'db>,
}

/// Retains syntax and index facts without requesting a semantic query before the controlled run.
#[derive(Clone, Copy, Debug)]
struct Request<'ast, 'db> {
    index: &'ast SemanticIndex<'db>,
    call: &'ast ast::ExprCall,
    callee: Expression<'db>,
    scope: FileScopeId,
    adapter: Adapter,
}

impl<'db> MemberOperation<'db> for Request<'_, 'db> {
    type Output = Matched<'db>;

    /// Infers the actual callee and runs preparation and matching without entering the checker.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        let inference = access
            .expression(self.callee, TypeContext::default())
            .await?;
        let callable = effects
            .local_with_fixed_transfers(2, 0, || inference.expression_type(&*self.call.func))
            .await?;
        let count = self.call.arguments.len();
        let bytes = CallArguments::capacity_bytes(count).ok_or(RunError::Contract(
            "constructor control argument quotation overflow",
        ))?;
        let arguments = effects
            .local_with_fixed_transfers(count.saturating_mul(4).saturating_add(4), bytes, || {
                CallArguments::from_arguments(&self.call.arguments, |_, _| Type::unknown())
            })
            .await?;
        let matching = async {
            match self.adapter {
                Adapter::Local => {
                    let (guard, bindings) = effects
                        .match_local_bindings(&env, self.index, self.scope, callable, &arguments)
                        .await?;
                    drop(guard);
                    Ok::<Bindings<'db>, RunError>(bindings)
                }
                Adapter::SuppliedGuard => {
                    let guard = InvocationEffects::new_guard(&effects).await?;
                    let context = InvocationContext {
                        db: access.db(),
                        env: &env,
                        arguments: &arguments,
                    };
                    let bindings =
                        InvocationEffects::prepare(&effects, context, callable, &guard).await?;
                    let bindings =
                        InvocationEffects::match_parameters(&effects, context, bindings).await?;
                    drop(guard);
                    Ok::<Bindings<'db>, RunError>(bindings)
                }
            }
        };
        let mut matching = std::pin::pin!(matching);
        let bindings = poll_fn(|context| {
            TRACE.with_borrow_mut(|trace| {
                if let Some(trace) = trace {
                    trace.mapping_pending = false;
                }
            });
            let result = matching.as_mut().poll(context);
            if result.is_pending() {
                observe_pending();
            }
            result
        })
        .await?;
        Ok(Matched { callable, bindings })
    }
}

/// Finds the specified statement-level call using parsed syntax and structural index data.
/// The index gives its callee a standalone query key; calls inside assignments or returns do not have that key.
fn request<'ast, 'db>(
    prepared: &'ast PreparedAnalysisFile<'db>,
    site: CallSite,
    adapter: Adapter,
) -> Request<'ast, 'db> {
    let body = &prepared.parsed_module().syntax().body;
    let call = match site {
        CallSite::Module => match body.last() {
            Some(Stmt::Expr(statement)) => statement.value.as_call_expr(),
            _ => None,
        },
        CallSite::Again => body
            .iter()
            .filter_map(Stmt::as_class_def_stmt)
            .find(|class| class.name.as_str() == "Box")
            .and_then(|class| {
                class
                    .body
                    .iter()
                    .filter_map(Stmt::as_function_def_stmt)
                    .find(|function| function.name.as_str() == "again")
            })
            .and_then(|function| function.body.first())
            .and_then(Stmt::as_expr_stmt)
            .map(|statement| &*statement.value)
            .and_then(ast::Expr::as_call_expr),
    }
    .expect("constructor fixture call");
    let index = prepared.semantic_index();
    Request {
        index,
        call,
        callee: index.expression(&*call.func),
        scope: index.expression_scope_id(&ast::ExprRef::from(call)),
        adapter,
    }
}

/// Creates an independent semantic database for every cold matching attempt.
fn database(source: &str) -> TestDb {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()
        .unwrap()
}

/// Stores a bounded event sequence without charging or changing the active runtime.
#[derive(Clone, Debug)]
struct Trace {
    events: [Option<Event>; 512],
    count: usize,
    overflowed: bool,
    pending_after_increment: usize,
    pending_with_mapping: usize,
    mapping_pending: bool,
    mapping_retirement_live_children: Option<usize>,
    cancel_on_mapping_child: Option<salsa::CancellationToken>,
    cancelled_at_pending: bool,
    cancelled_mapping_child: Option<usize>,
    watched_child: Option<salsa::DatabaseKeyIndex>,
    child_entry: Option<CanonicalChildEntry>,
    cancel_canonical_child: Option<salsa::CancellationToken>,
    owner_retirement: Option<OwnerRetirement>,
}

/// Records whether the mapping root was suspended and its child still live at canonical entry.
#[derive(Clone, Copy, Debug)]
struct CanonicalChildEntry {
    mapping_pending: bool,
    live_mapping_children: usize,
    preceding_events: usize,
}

/// Locates a constructor owner's real retirement relative to mapping children and guard storage.
#[derive(Clone, Copy, Debug)]
struct OwnerRetirement {
    id: usize,
    mapping_events: usize,
    guard_events: usize,
    live_children: usize,
    complete_guard_events: Option<usize>,
}

thread_local! {
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

/// Records an already-reached production boundary without affecting its admission.
/// An armed token requests caller cancellation when a freshening child starts while matching is
/// still suspended; the parent poll and child entry occur separately in the runtime queue.
fn observe(event: Event) {
    if event == Event::AfterNonce(ConstructorNonceOperation::Increment) {
        mapping_observations::reset(None);
    }
    let cancellation = TRACE.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            if matches!(event, Event::MappingRetired { .. }) {
                let live = live_mapping_children();
                trace.mapping_retirement_live_children = Some(
                    trace.mapping_retirement_live_children.unwrap_or(0).max(live),
                );
            }
            if matches!(
                event,
                Event::BeforeMatching(ConstructorMatchingOperation::GenericContextIntern)
                    | Event::AfterMatching(ConstructorMatchingOperation::GenericContextIntern)
            ) && !trace.events.contains(&Some(Event::AfterNonce(
                ConstructorNonceOperation::Increment,
            ))) {
                return None;
            }
            if let Some(slot) = trace.events.get_mut(trace.count) {
                *slot = Some(event);
            } else {
                trace.overflowed = true;
            }
            trace.count += 1;
            if let Event::MappingChild { visitor, .. } = event
                && trace.mapping_pending
                && let Some(cancellation) = trace.cancel_on_mapping_child.take()
            {
                trace.cancelled_at_pending = true;
                trace.cancelled_mapping_child = Some(visitor);
                return Some(cancellation);
            }
        }
        None
    });
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }
}

/// Records matching suspension, counting it separately after freshening starts and when a
/// freshening root has been observed.
/// The child-entry observer can use this state until matching is polled again.
fn observe_pending() {
    TRACE.with_borrow_mut(|trace| {
        let Some(trace) = trace else {
            return;
        };
        trace.mapping_pending = true;
        if trace.events.contains(&Some(Event::AfterNonce(
            ConstructorNonceOperation::Increment,
        ))) {
            trace.pending_after_increment += 1;
        }
        if mapping_observations::mapping_snapshot()
            .roots
            .iter()
            .flatten()
            .any(|root| {
                matches!(
                    root.mapping,
                    OwnedMappingSnapshot::FreshenBoundTypeVars { .. }
                )
            })
        {
            trace.pending_with_mapping += 1;
        }
    });
}

/// Counts retained mapping children whose actual futures have not yet retired.
fn live_mapping_children() -> usize {
    let snapshot = mapping_observations::set_cleanup_snapshot();
    snapshot.events[..snapshot.event_count.min(snapshot.events.len())]
        .iter()
        .flatten()
        .fold(0usize, |live, event| match event {
            mapping_observations::SetCleanupEvent::ChildEntered { .. } => live + 1,
            mapping_observations::SetCleanupEvent::ChildDropped { .. } => live - 1,
            mapping_observations::SetCleanupEvent::BuilderDropped { .. } => live,
        })
}

/// Observes one selected canonical query and optionally cancels at its real execution entry.
fn observe_query_entry(event: &salsa::EventKind) {
    let salsa::EventKind::WillExecute { database_key } = event else {
        return;
    };
    let cancellation = TRACE.with_borrow_mut(|trace| {
        let Some(trace) = trace else {
            return None;
        };
        if trace.watched_child != Some(*database_key) {
            return None;
        }
        trace.child_entry = Some(CanonicalChildEntry {
            mapping_pending: trace.mapping_pending,
            live_mapping_children: live_mapping_children(),
            preceding_events: trace.count,
        });
        trace.cancel_canonical_child.take()
    });
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }
}

/// Records constructor storage retirement only after matching has entered its freshening pass.
fn observe_owner_retirement(event: owner_observations::RetirementEvent<'_>) {
    TRACE.with_borrow_mut(|trace| {
        let Some(trace) = trace else {
            return;
        };
        if event.constructor_kind.is_none()
            || !trace.events.contains(&Some(Event::AfterNonce(
                ConstructorNonceOperation::Increment,
            )))
        {
            return;
        }
        match event.phase {
            owner_observations::RetirementPhase::Begin if trace.owner_retirement.is_none() => {
                trace.owner_retirement = Some(OwnerRetirement {
                    id: event.id,
                    mapping_events: mapping_observations::set_cleanup_snapshot().event_count,
                    guard_events: guard_observations::snapshot().count,
                    live_children: live_mapping_children(),
                    complete_guard_events: None,
                });
            }
            owner_observations::RetirementPhase::Complete => {
                if let Some(retirement) = &mut trace.owner_retirement
                    && retirement.id == event.id
                {
                    retirement.complete_guard_events = Some(guard_observations::snapshot().count);
                }
            }
            owner_observations::RetirementPhase::Begin => {}
        }
    });
}

/// Limits passive callback state to one attempt, including cleanup, on the executing test thread.
#[derive(Debug)]
struct Recording {
    previous: Option<fn(Event)>,
    previous_owner: Option<owner_observations::RetirementObserver>,
}

impl Recording {
    /// Starts empty matching, mapping, and guard observations before the controlled request.
    fn start() -> Self {
        TRACE.with_borrow_mut(|trace| {
            assert!(trace.is_none());
            *trace = Some(Trace {
                events: [None; 512],
                count: 0,
                overflowed: false,
                pending_after_increment: 0,
                pending_with_mapping: 0,
                mapping_pending: false,
                mapping_retirement_live_children: None,
                cancel_on_mapping_child: None,
                cancelled_at_pending: false,
                cancelled_mapping_child: None,
                watched_child: None,
                child_entry: None,
                cancel_canonical_child: None,
                owner_retirement: None,
            });
        });
        mapping_observations::reset(None);
        guard_observations::reset();
        Self {
            previous: matching_observations::set_observer(Some(observe)),
            previous_owner: owner_observations::set_retirement_observer(Some(
                observe_owner_retirement,
            )),
        }
    }

    /// Copies the recorded boundaries so assertions can run after the observer is removed.
    fn trace(&self) -> Trace {
        TRACE.with_borrow(|trace| trace.as_ref().expect("active matching recording").clone())
    }

    /// Arms caller cancellation at a retained freshening child's entry after matching returns
    /// a real `Pending`, provided matching has not been polled again before that entry.
    fn cancel_at_pending(&self, cancellation: salsa::CancellationToken) {
        TRACE.with_borrow_mut(|trace| {
            trace.as_mut().unwrap().cancel_on_mapping_child = Some(cancellation);
        });
    }

    /// Selects a canonical query and records the suspended mapping state when that query enters.
    /// An optional token requests cancellation at that same execution entry.
    fn watch_child(
        &self,
        key: salsa::DatabaseKeyIndex,
        cancellation: Option<salsa::CancellationToken>,
    ) {
        TRACE.with_borrow_mut(|trace| {
            let trace = trace.as_mut().unwrap();
            trace.watched_child = Some(key);
            trace.cancel_canonical_child = cancellation;
        });
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        matching_observations::set_observer(self.previous);
        owner_observations::set_retirement_observer(self.previous_owner);
        guard_observations::stop();
        TRACE.with_borrow_mut(|trace| *trace = None);
    }
}

/// Confirms every guard scope and queued runtime attempt has drained after the request returns.
fn assert_drained() {
    let guards = guard_observations::snapshot();
    assert!(!guards.overflowed, "{guards:?}");
    assert_eq!(guards.active_scopes, 0, "{guards:?}");
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Compares the complete matched state, including downstream trees and shape errors.
/// Ordinary preparation and matching happen only after the controlled attempt has completed.
fn assert_ordinary_parity<'db>(db: &'db TestDb, request: Request<'_, 'db>, result: &Matched<'db>) {
    let env = ProgramEnvironment::from_file(request.callee.program_file(db));
    let arguments = CallArguments::from_arguments(&request.call.arguments, |_, _| Type::unknown());
    let mut ordinary = result
        .callable
        .bindings_impl(db, &env, &CallableRecursionGuard::new());
    if request.adapter == Adapter::Local {
        ordinary.set_enclosing_binding_contexts(enclosing_binding_contexts(
            request.index,
            request.scope,
        ));
    }
    let ordinary = ordinary.match_parameters(db, &env, &arguments);
    assert_eq!(format!("{:#?}", result.bindings), format!("{ordinary:#?}"));
}

/// Requires one fully funded cold match, compares its complete result with ordinary matching,
/// and returns the passive trace after checking drainage.
fn assert_cold_parity(source: &str, site: CallSite, adapter: Adapter) -> Trace {
    let db = database(source);
    let prepared = prepare(&db);
    let request = request(&prepared, site, adapter);
    observations::reset(None);
    let recording = Recording::start();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let trace = recording.trace();
    drop(recording);
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = captured.value else {
        panic!("cold constructor matching: {:?}", captured.value);
    };
    assert!(!trace.overflowed, "{trace:?}");
    assert_drained();
    assert_ordinary_parity(&db, request, &result);
    trace
}

/// Explicit specialization completes generic matching on the first class-context occurrence.
#[test]
fn cold_explicit_bool_constructor_matches_without_freshening() {
    let trace = assert_cold_parity(BOX, CallSite::Module, Adapter::Local);
    assert!(trace.events.contains(&Some(Event::AfterNonce(
        ConstructorNonceOperation::SeenInsert
    ))));
    assert!(!trace.events.contains(&Some(Event::AfterNonce(
        ConstructorNonceOperation::Increment
    ))));
}

/// The class context enclosing `again` freshens its first constructor occurrence before matching.
/// Its mapped receiver and parameter share the same new variable while ordinary parity checks
/// the complete tree, including preservation of the source-level constructor instance.
#[test]
fn cold_enclosing_class_constructor_freshens_before_matching() {
    let db = database(BOX);
    let prepared = prepare(&db);
    let request = request(&prepared, CallSite::Again, Adapter::Local);
    observations::reset(None);
    let recording = Recording::start();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let trace = recording.trace();
    drop(recording);
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = captured.value else {
        panic!("cold enclosing constructor: {:?}", captured.value);
    };
    assert!(!trace.overflowed, "{trace:?}");
    assert_eq!(trace.events.iter().filter(|event| **event == Some(Event::AfterNonce(ConstructorNonceOperation::Increment))).count(), 1);
    assert!(!trace.events.contains(&Some(Event::AfterNonce(
        ConstructorNonceOperation::SeenInsert
    ))));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let binding = result
        .bindings
        .single_element()
        .expect("single initializer callable");
    let receiver = binding.bound_type.expect("bound initializer receiver");
    let (_, specialization) = receiver
        .class_specialization(&db, &env)
        .expect("fresh receiver specialization");
    let Type::TypeVar(variable) = specialization.types(&db)[0] else {
        panic!("fresh receiver argument is not a type variable");
    };
    assert_ne!(variable.freshness(&db).value(), 0);
    assert_eq!(
        binding.overloads()[0]
            .signature
            .parameters()
            .iter()
            .last()
            .unwrap()
            .annotated_type(),
        Type::TypeVar(variable)
    );
    let first_match = trace
        .events
        .iter()
        .position(|event| {
            matches!(
                event,
                Some(Event::Entry(ConstructorMatchingPhase::Matching, _))
            )
        })
        .expect("matching phase");
    let install = trace
        .events
        .iter()
        .rposition(|event| {
            *event
                == Some(Event::AfterMatching(
                    ConstructorMatchingOperation::SignatureInstall,
                ))
        })
        .expect("fresh signature installation");
    assert!(install < first_match, "{trace:?}");
    assert_drained();
    assert_ordinary_parity(&db, request, &result);
}

/// The supplied-guard invocation adapter reaches the same matched constructor result.
#[test]
fn supplied_guard_adapter_matches_explicit_generic_constructor() {
    assert_cold_parity(BOX, CallSite::Module, Adapter::SuppliedGuard);
}

/// Each constructor method receives the original argument shape before downstream checking.
#[test_case::test_case(INITIALIZER; "entry only")]
#[test_case::test_case(DOWNSTREAM; "entry and downstream")]
#[test_case::test_case(THREE_METHODS; "all three constructor methods")]
fn cold_constructor_tree_matches_ordinary_state(source: &str) {
    assert_cold_parity(source, CallSite::Module, Adapter::Local);
}

/// Matching preserves missing, extra and duplicate argument errors and accepts a valid explicit keyword.
#[test_case::test_case("Product()"; "missing argument")]
#[test_case::test_case("Product(True, False)"; "extra argument")]
#[test_case::test_case("Product(True, value=False)"; "duplicate argument")]
#[test_case::test_case("Product(value=True)"; "explicit keyword")]
fn constructor_argument_shapes_match_ordinary_state(call: &str) {
    let source = format!("class Product:\n    def __init__(self, value): ...\n{call}\n");
    assert_cold_parity(&source, CallSite::Module, Adapter::Local);
}

/// Every overload keeps its own matches and errors, in the ordinary declaration order.
#[test]
fn constructor_overloads_match_ordinary_state() {
    assert_cold_parity(
        "from typing import overload\nclass Product:\n    @overload\n    def __init__(self, first): ...\n    @overload\n    def __init__(self, first, second): ...\n    def __init__(self, *args): ...\nProduct(True)\n",
        CallSite::Module,
        Adapter::Local,
    );
}

/// Matching exposes actual canonical suspension after the freshening nonce has advanced.
#[test]
fn freshening_observes_real_canonical_child_pending() {
    let trace = assert_cold_parity(BOX, CallSite::Again, Adapter::Local);
    assert!(trace.pending_after_increment > 0, "{trace:?}");
    assert!(trace.pending_with_mapping > 0, "{trace:?}");
}

/// Selects the independent execution ceiling reduced by a refusal control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    /// Reduces one ceiling while retaining the other funded ceiling.
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

    /// Returns the unchanged maximum allowance for the selected resource.
    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    /// Identifies the incomplete result produced by the selected execution ceiling.
    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Pairs the passive events immediately before and after one admitted mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    Nonce(ConstructorNonceOperation),
    Matching(ConstructorMatchingOperation),
}

impl Boundary {
    /// Returns the event emitted before the selected mutation requests admission.
    const fn before(self) -> Event {
        match self {
            Self::Nonce(operation) => Event::BeforeNonce(operation),
            Self::Matching(operation) => Event::BeforeMatching(operation),
        }
    }

    /// Returns the event emitted only after the selected admitted mutation completes.
    const fn after(self) -> Event {
        match self {
            Self::Nonce(operation) => Event::AfterNonce(operation),
            Self::Matching(operation) => Event::AfterMatching(operation),
        }
    }
}

/// Tests whether a fresh database reaches a mutation under the supplied numeric policy.
fn reaches_mutation(site: CallSite, boundary: Boundary, policy: &AnalysisPolicy) -> bool {
    let db = database(BOX);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let _result =
        controlled_member_operation(&prepared, request(&prepared, site, Adapter::Local), policy);
    let trace = recording.trace();
    drop(recording);
    assert_drained();
    trace.events.contains(&Some(boundary.after()))
}

/// Finds an actual numeric refusal before a mutation and retries that database in the same revision.
/// Search attempts use separate cold databases; only the explicit retry reuses completed children.
fn assert_refusal_before_mutation(site: CallSite, boundary: Boundary, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(
        reaches_mutation(site, boundary, &resource.policy(high)),
        "{boundary:?}"
    );
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_mutation(site, boundary, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(BOX);
    let prepared = prepare(&db);
    let request = request(&prepared, site, Adapter::Local);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected {resource:?} refusal before {boundary:?}: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(trace.events.contains(&Some(boundary.before())), "{trace:?}");
    assert!(!trace.events.contains(&Some(boundary.after())), "{trace:?}");
    assert_drained();
    let recording = Recording::start();
    let retry = controlled_member_operation(&prepared, request, &funded());
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("funded matching retry: {retry:?}");
    };
    assert!(trace.events.contains(&Some(boundary.after())), "{trace:?}");
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
    assert_ordinary_parity(&db, request, &result);
}

/// Work and byte ceilings independently reject the selected mutation before it changes state.
#[test_case::test_case(CallSite::Module, Boundary::Nonce(ConstructorNonceOperation::Create), Resource::Work; "nonce creation work")]
#[test_case::test_case(CallSite::Module, Boundary::Nonce(ConstructorNonceOperation::Create), Resource::Bytes; "nonce creation bytes")]
#[test_case::test_case(CallSite::Module, Boundary::Nonce(ConstructorNonceOperation::SeenInsert), Resource::Work; "seen insertion work")]
#[test_case::test_case(CallSite::Module, Boundary::Nonce(ConstructorNonceOperation::SeenInsert), Resource::Bytes; "seen insertion bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Nonce(ConstructorNonceOperation::EnclosingInsert), Resource::Work; "enclosing insertion work")]
#[test_case::test_case(CallSite::Again, Boundary::Nonce(ConstructorNonceOperation::EnclosingInsert), Resource::Bytes; "enclosing insertion bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Nonce(ConstructorNonceOperation::Increment), Resource::Work; "nonce increment work")]
#[test_case::test_case(CallSite::Again, Boundary::Nonce(ConstructorNonceOperation::Increment), Resource::Bytes; "nonce increment bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::FrameGrowth), Resource::Work; "frame growth work")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::FrameGrowth), Resource::Bytes; "frame growth bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::ContextBufferGrowth), Resource::Work; "context buffer growth work")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::ContextBufferGrowth), Resource::Bytes; "context buffer growth bytes")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::BoundArguments), Resource::Work; "bound arguments work")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::BoundArguments), Resource::Bytes; "bound arguments bytes")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::MatcherAllocation), Resource::Work; "matcher allocation work")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::MatcherAllocation), Resource::Bytes; "matcher allocation bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::SignatureInstall), Resource::Work; "signature installation work")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::SignatureInstall), Resource::Bytes; "signature installation bytes")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::GenericContextIntern), Resource::Work; "fresh generic context work")]
#[test_case::test_case(CallSite::Again, Boundary::Matching(ConstructorMatchingOperation::GenericContextIntern), Resource::Bytes; "fresh generic context bytes")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::ResultTransfer), Resource::Work; "result transfer work")]
#[test_case::test_case(CallSite::Module, Boundary::Matching(ConstructorMatchingOperation::ResultTransfer), Resource::Bytes; "result transfer bytes")]
fn numeric_refusal_precedes_constructor_mutation(
    site: CallSite,
    boundary: Boundary,
    resource: Resource,
) {
    assert_refusal_before_mutation(site, boundary, resource);
}

/// Unsupported splats retain their existing operation instead of becoming a constructor fallback.
#[test_case::test_case("Product(*values)", OperationId::CallVariadicMatching; "positional splat")]
#[test_case::test_case("Product(**values)", OperationId::CallKeywordMatching; "keyword splat")]
fn constructor_splats_preserve_exact_refusal(call: &str, operation: OperationId) {
    let db = database(&format!(
        "class Product:\n    def __init__(self, value): ...\n{call}\n"
    ));
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        request(&prepared, CallSite::Module, Adapter::Local),
        &funded(),
    );
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected splat refusal: {result:?}");
    };
    assert_eq!(reason, AnalysisIncomplete::UnavailableOperation(operation));
    assert_drained();
}

/// Selects eager metadata constructed directly rather than inferred from a lazy source declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EagerMetadata {
    Absent,
    UpperBound,
    Constraints,
    Default,
}

/// Holds canonical input handles for exercising retained freshening independently of source parsing.
#[derive(Clone, Copy, Debug)]
struct ConstructedRequest<'db> {
    variable: BoundTypeVarInstance<'db>,
    input: Type<'db>,
    context: GenericContext<'db>,
}

impl<'db> MemberOperation<'db> for ConstructedRequest<'db> {
    type Output = Type<'db>;

    /// Maps the supplied bound occurrence through the production retained constructor mapper.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let env = ProgramEnvironment::from_program(program);
        let effects = SourceEffects::new(access, program);
        let mapping = ConstructorMatchingEffects::freshen_type(
            &effects,
            access.db(),
            &env,
            self.input,
            self.context,
            7,
        );
        let mut mapping = std::pin::pin!(mapping);
        poll_fn(|context| {
            TRACE.with_borrow_mut(|trace| {
                if let Some(trace) = trace {
                    trace.mapping_pending = false;
                }
            });
            let result = mapping.as_mut().poll(context);
            if result.is_pending() {
                observe_pending();
            }
            result
        })
        .await
    }
}

/// Constructs eager canonical metadata; no source query is used to turn a lazy bound into an eager one.
fn constructed_request<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    metadata: EagerMetadata,
) -> ConstructedRequest<'db> {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let program = prepared.program_file().program(db);
    let binding = BindingContext::Synthetic(program);
    let class = prepared.parsed_module().syntax().body[0]
        .as_class_def_stmt()
        .unwrap();
    let parameters = &class.type_params.as_ref().unwrap().type_params;
    let definition = |parameter: &ast::TypeParam| match parameter {
        ast::TypeParam::TypeVar(node) => prepared.semantic_index().expect_single_definition(node),
        ast::TypeParam::ParamSpec(node) => prepared.semantic_index().expect_single_definition(node),
        ast::TypeParam::TypeVarTuple(node) => {
            prepared.semantic_index().expect_single_definition(node)
        }
    };
    let dependency = BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static("T"),
                Some(definition(&parameters[0])),
                TypeVarKind::Pep695TypeVar,
            ),
            None,
            None,
            None,
        ),
        binding,
        None,
        TypeVarNonce::NONE,
    );
    let bounds = match metadata {
        EagerMetadata::Absent | EagerMetadata::Default => None,
        EagerMetadata::UpperBound => Some(TypeVarBoundOrConstraintsEvaluation::from(
            TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(dependency)),
        )),
        EagerMetadata::Constraints => Some(TypeVarBoundOrConstraintsEvaluation::from(
            TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                db,
                vec![
                    Type::TypeVar(dependency),
                    Type::bool_literal(true),
                    Type::TypeVar(dependency),
                ]
                .into_boxed_slice(),
            )),
        )),
    };
    let default = match metadata {
        EagerMetadata::Default => Some(TypeVarDefaultEvaluation::from(Type::bool_literal(false))),
        EagerMetadata::Absent | EagerMetadata::UpperBound | EagerMetadata::Constraints => None,
    };
    let variable = BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static("U"),
                Some(definition(&parameters[1])),
                TypeVarKind::Pep695TypeVar,
            ),
            bounds,
            Some(TypeVarVariance::Covariant),
            default,
        ),
        binding,
        None,
        TypeVarNonce::NONE,
    );
    ConstructedRequest {
        variable,
        input: Type::TypeVar(variable),
        context: GenericContext::from_typevar_instances(db, &env, [dependency, variable]),
    }
}

/// Eager bounds, ordered duplicate constraints, and bound defaults retain their ordinary metadata.
/// These constructed-input cases do not establish support for lazy source-bound producers.
#[test_case::test_case(EagerMetadata::Absent; "absent bounds and default")]
#[test_case::test_case(EagerMetadata::UpperBound; "eager upper bound")]
#[test_case::test_case(EagerMetadata::Constraints; "eager ordered constraints")]
#[test_case::test_case(EagerMetadata::Default; "eager bound default")]
fn constructed_eager_freshening_matches_ordinary_metadata(metadata: EagerMetadata) {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let request = constructed_request(&db, &prepared, metadata);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    observations::reset(None);
    let recording = Recording::start();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    drop(recording);
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(mapped @ Type::TypeVar(variable))) = captured.value else {
        panic!("constructed eager freshening: {:?}", captured.value);
    };
    assert_drained();
    assert_eq!(variable.freshness(&db).value(), 7);
    assert_eq!(
        variable.binding_context(&db),
        request.variable.binding_context(&db)
    );
    assert_eq!(
        variable.typevar(&db).identity(&db),
        request.variable.typevar(&db).identity(&db)
    );
    assert_eq!(
        variable.typevar(&db).explicit_variance(&db),
        Some(TypeVarVariance::Covariant)
    );
    match metadata {
        EagerMetadata::Absent => assert_eq!(variable.typevar(&db), request.variable.typevar(&db)),
        EagerMetadata::UpperBound => {
            let Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(bound))) =
                variable.typevar(&db).bound_or_constraints(&db, &env)
            else {
                panic!("mapped eager upper bound");
            };
            assert_eq!(bound.freshness(&db).value(), 7);
        }
        EagerMetadata::Constraints => {
            let constraints = variable
                .typevar(&db)
                .constraints(&db, &env)
                .expect("mapped constraints");
            assert_eq!(constraints.len(), 3);
            assert_eq!(constraints[0], constraints[2]);
            assert_eq!(constraints[1], Type::bool_literal(true));
            let Type::TypeVar(bound) = constraints[0] else {
                panic!("mapped constraint variable");
            };
            assert_eq!(bound.freshness(&db).value(), 7);
        }
        EagerMetadata::Default => assert_eq!(
            variable.default_type(&db),
            Some(Type::bool_literal(false))
        ),
    }
    assert_eq!(
        mapped,
        Type::TypeVar(request.variable).apply_type_mapping(
            &db,
            &env,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: request.context,
                delta: 7
            },
            TypeContext::default()
        ),
    );
    let key = bound_typevar_default_ingredient(&db).database_key_index(request.variable.as_id());
    assert!(captured.reads.iter().any(|read| read.key == key));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            bound_typevar_default_ingredient(&db),
            request.variable.as_id()
        )
        .is_ok()
    );
}

/// Enclosing-context freshening refuses at the existing lazy upper-bound or constraints operation when demanded.
#[test_case::test_case("int", OperationId::ExplicitSpecializationLazyUpperBound; "lazy upper bound")]
#[test_case::test_case("(int, str)", OperationId::ExplicitSpecializationLazyConstraints; "lazy constraints")]
fn lazy_source_metadata_preserves_exact_refusal(annotation: &str, operation: OperationId) {
    let source = format!(
        "class Box[T: {annotation}]:\n    def __init__(self, value: T): ...\n    def again(self, value: T):\n        Box[T](value)\n"
    );
    let db = database(&source);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        request(&prepared, CallSite::Again, Adapter::Local),
        &funded(),
    );
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected lazy metadata refusal: {result:?}");
    };
    assert_eq!(reason, AnalysisIncomplete::UnavailableOperation(operation));
    assert_drained();
}

/// Caller cancellation while matching is suspended drains its newly entered freshening child before
/// the root constructor owner and guard storage retire. The fixture observes a real parent `Pending`
/// before cancelling at child entry; a funded attempt then matches in the same revision.
#[test]
fn cancelled_freshening_child_drains_before_constructor_owner_and_retries() {
    let db = database(BOX);
    let prepared = prepare(&db);
    let request = request(&prepared, CallSite::Again, Adapter::Local);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    recording.cancel_at_pending(db.cancellation_token());
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    let trace = recording.trace();
    drop(recording);
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}; {trace:?}"
    );
    assert!(trace.cancelled_at_pending, "{trace:?}");
    assert!(trace.pending_after_increment > 0, "{trace:?}");
    assert!(trace.pending_with_mapping > 0, "{trace:?}");
    let cancelled_visitor = trace
        .cancelled_mapping_child
        .expect("freshening child entered");
    assert!(
        trace.events.contains(&Some(Event::MappingEntered {
            visitor: cancelled_visitor,
        })),
        "{trace:?}",
    );
    assert_mapping_scopes_retired(&trace);
    let retirement = trace
        .owner_retirement
        .expect("freshened constructor owner retired");
    assert_eq!(retirement.live_children, 0, "{retirement:?}");
    let mapping = mapping_observations::set_cleanup_snapshot();
    assert!(mapping.event_count <= mapping.events.len(), "{mapping:?}");
    let last_child = mapping.events[..mapping.event_count]
        .iter()
        .rposition(|event| {
            matches!(
                event,
                Some(mapping_observations::SetCleanupEvent::ChildDropped { .. })
            )
        })
        .expect("retained mapping child retired");
    assert!(
        last_child < retirement.mapping_events,
        "{retirement:?}; {mapping:?}"
    );
    let guard = guard_observations::snapshot();
    assert!(!guard.overflowed, "{guard:?}");
    let storage_drop = guard.events[..guard.count]
        .iter()
        .rposition(|event| {
            matches!(
                event,
                Some(guard_observations::Event::StorageDropped { .. })
            )
        })
        .expect("invocation guard storage retired");
    let complete = retirement
        .complete_guard_events
        .expect("constructor retirement completed");
    assert!(retirement.guard_events <= complete);
    assert!(complete <= storage_drop, "{retirement:?}; {guard:?}");
    assert_drained();

    observations::reset(None);
    let recording = Recording::start();
    let retry = controlled_member_operation(&prepared, request, &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("same-revision constructor matching retry: {retry:?}");
    };
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
    assert_ordinary_parity(&db, request, &result);
}

/// Checks that every observed freshening root has retired with no active transformation scopes.
/// Its pooled visitor can remain allocated after this assertion; only the mapping entry has ended.
fn assert_mapping_scopes_retired(trace: &Trace) {
    let entered = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::MappingEntered { visitor }) => Some(*visitor),
            _ => None,
        })
        .collect::<Vec<_>>();
    let retired = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::MappingRetired {
                visitor,
                active: Some(0),
            }) => Some(*visitor),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!entered.is_empty(), "{trace:?}");
    assert_eq!(retired, entered, "{trace:?}");
}

/// Selects observation or cancellation when the chosen canonical query actually enters execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalChildAction {
    Observe,
    Cancel,
}

/// A TypeForm mapping remains suspended with its transformation scope retained while its canonical
/// bound-default child runs. The scheduler can complete that child before polling the root again.
/// On cancellation the child drains before the scope retires; a completed memo is reused and an
/// unfinished child executes again on the same-revision retry.
#[test_case::test_case(CanonicalChildAction::Observe; "canonical completion")]
#[test_case::test_case(CanonicalChildAction::Cancel; "canonical cancellation and retry")]
fn constructed_typeform_scope_retains_canonical_child(action: CanonicalChildAction) {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", CONSTRUCTED)
        .with_salsa_event_callback(observe_query_entry)
        .build()
        .unwrap();
    let prepared = prepare(&db);
    let mut request = constructed_request(&db, &prepared, EagerMetadata::Default);
    request.input = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(request.variable)));
    let revision = salsa::plumbing::current_revision(&db);
    let ingredient = bound_typevar_default_ingredient(&db);
    let key = ingredient.database_key_index(request.variable.as_id());
    observations::reset(None);
    let recording = Recording::start();
    recording.watch_child(
        key,
        (action == CanonicalChildAction::Cancel).then(|| db.cancellation_token()),
    );
    let first = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    let trace = recording.trace();
    drop(recording);
    assert!(!trace.overflowed, "{trace:?}");
    let child_entry = trace.child_entry.expect("watched canonical child execution");
    assert!(trace.pending_with_mapping > 0, "{trace:?}");
    assert!(child_entry.mapping_pending, "{trace:?}");
    assert!(child_entry.live_mapping_children > 0, "{trace:?}");
    let before_child = &trace.events[..child_entry.preceding_events];
    let visitor = before_child
        .iter()
        .rev()
        .find_map(|event| match event {
            Some(Event::MappingChild {
                visitor,
                active: Some(depth),
            }) if *depth > 0 => Some(*visitor),
            _ => None,
        })
        .expect("TypeForm scope sampled active when the retained child started");
    assert!(
        before_child.contains(&Some(Event::MappingEntered { visitor })),
        "{trace:?}"
    );
    assert!(
        !before_child.iter().any(|event| {
            matches!(event, Some(Event::MappingRetired { visitor: retired, .. }) if *retired == visitor)
        }),
        "{trace:?}"
    );
    assert_eq!(trace.mapping_retirement_live_children, Some(0), "{trace:?}");
    assert_mapping_scopes_retired(&trace);
    assert_drained();
    let first_published =
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, request.variable.as_id()).is_ok();
    if action == CanonicalChildAction::Cancel {
        assert!(matches!(first, Err(salsa::Cancelled::Local)), "{first:?}");
    } else {
        assert!(
            matches!(first, Ok(Ok(AnalysisOutcome::Complete(Type::TypeForm(_))))),
            "{first:?}"
        );
        assert!(first_published);
    }
    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start();
    let retry = controlled_member_operation(&prepared, request, &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("TypeForm freshening retry: {retry:?}");
    };
    let events = event_db.take_salsa_events();
    let executed = events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    });
    assert_eq!(executed, !first_published);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, request.variable.as_id()).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        result,
        request.input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: request.context,
                delta: 7,
            },
            TypeContext::default()
        )
    );
}

/// Tests whether freshening has interned reconstructed constraints under one independent ceiling.
fn reaches_constraint_interner(policy: &AnalysisPolicy) -> bool {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let request = constructed_request(&db, &prepared, EagerMetadata::Constraints);
    observations::reset(None);
    let recording = Recording::start();
    let _result = controlled_member_operation(&prepared, request, policy);
    let trace = recording.trace();
    drop(recording);
    assert_drained();
    trace.events.contains(&Some(Event::AfterMatching(
        ConstructorMatchingOperation::ConstraintIntern,
    )))
}

/// Canonical constraint interning refuses independently under work and byte exhaustion, then retries.
/// Input constraints are constructed before the attempt; the observed mutation is reconstruction.
#[test_case::test_case(Resource::Work; "work")]
#[test_case::test_case(Resource::Bytes; "bytes")]
fn constructed_constraint_interning_refuses_before_mutation(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_constraint_interner(&resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_constraint_interner(&resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let request = constructed_request(&db, &prepared, EagerMetadata::Constraints);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("constraint interner refusal: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(
        trace.events.contains(&Some(Event::BeforeMatching(
            ConstructorMatchingOperation::ConstraintIntern
        ))),
        "{trace:?}"
    );
    assert!(
        !trace.events.contains(&Some(Event::AfterMatching(
            ConstructorMatchingOperation::ConstraintIntern
        ))),
        "{trace:?}"
    );
    assert_drained();
    let recording = Recording::start();
    let retry = controlled_member_operation(&prepared, request, &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("constraint interner retry: {retry:?}");
    };
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        result,
        request.input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: request.context,
                delta: 7
            },
            TypeContext::default()
        )
    );
    assert_drained();
}

/// Selects whether the context is empty, first-seen, entirely enclosing, or mixed across owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NonceCase {
    Empty,
    Repeated,
    Enclosing,
    Mixed,
}

/// Holds one canonical context and its already-known enclosing identities for nonce decisions.
#[derive(Clone, Copy, Debug)]
struct NonceRequest<'a, 'db> {
    context: GenericContext<'db>,
    enclosing: &'a [BindingContext<'db>],
}

/// Records eligibility and consumed deltas without hiding nonce continuation behind matching output.
#[derive(Debug, Eq, PartialEq)]
struct NonceResult {
    first: Option<u32>,
    repeated: Option<u32>,
    continuation: u32,
}

impl<'db> MemberOperation<'db> for NonceRequest<'_, 'db> {
    type Output = NonceResult;

    /// Uses one shared generator for two occurrences and a following nonce request.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let generator =
            TypeVarNonceGenerator::new_for_matching_with(self.enclosing, &effects).await?;
        let first = if generator
            .should_freshen_with(access.db(), self.context, &effects)
            .await?
        {
            Some(generator.next_with(&effects).await?.value())
        } else {
            None
        };
        let repeated = if generator
            .should_freshen_with(access.db(), self.context, &effects)
            .await?
        {
            Some(generator.next_with(&effects).await?.value())
        } else {
            None
        };
        let continuation = generator.next_with(&effects).await?.value();
        Ok(NonceResult {
            first,
            repeated,
            continuation,
        })
    }
}

/// Only a context entirely owned by one enclosing binding freshens immediately without touching seen.
/// Empty and mixed contexts preserve first-versus-repeated eligibility and nonce continuation.
#[test_case::test_case(NonceCase::Empty; "empty context")]
#[test_case::test_case(NonceCase::Repeated; "first and repeated context")]
#[test_case::test_case(NonceCase::Enclosing; "enclosing context")]
#[test_case::test_case(NonceCase::Mixed; "mixed binding contexts")]
fn constructed_nonce_decisions_preserve_order(case: NonceCase) {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let input = constructed_request(&db, &prepared, EagerMetadata::Absent);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let enclosing = match case {
        NonceCase::Repeated => Vec::new(),
        NonceCase::Empty | NonceCase::Enclosing | NonceCase::Mixed => {
            vec![input.variable.binding_context(&db)]
        }
    };
    let context = match case {
        NonceCase::Empty => GenericContext::from_typevar_instances(&db, &env, []),
        NonceCase::Repeated | NonceCase::Enclosing => input.context,
        NonceCase::Mixed => {
            let class = prepared.parsed_module().syntax().body[0]
                .as_class_def_stmt()
                .unwrap();
            let definition = prepared.semantic_index().expect_single_definition(class);
            let other = BoundTypeVarInstance::new(
                &db,
                input.variable.typevar(&db),
                BindingContext::Definition(definition),
                None,
                TypeVarNonce::NONE,
            );
            GenericContext::from_typevar_instances(&db, &env, [input.variable, other])
        }
    };
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        NonceRequest {
            context,
            enclosing: &enclosing,
        },
        &funded(),
    );
    let trace = recording.trace();
    drop(recording);
    let expected = match case {
        NonceCase::Enclosing => NonceResult {
            first: Some(1),
            repeated: Some(2),
            continuation: 3,
        },
        NonceCase::Empty | NonceCase::Repeated | NonceCase::Mixed => NonceResult {
            first: None,
            repeated: Some(1),
            continuation: 2,
        },
    };
    assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
    assert_eq!(
        trace.events.contains(&Some(Event::AfterNonce(
            ConstructorNonceOperation::SeenInsert
        ))),
        case != NonceCase::Enclosing
    );
    assert_drained();
}

/// Selects an occurrence that must return unchanged before its lazy metadata is demanded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkippedVariable {
    OutsideContext,
    ParamSpec,
    ParamSpecArgs,
    ParamSpecKwargs,
}

/// Context exclusion and ParamSpec eligibility preserve the exact occurrence without reading bounds or defaults.
/// These constructed inputs need only admitted interned fields, so no canonical query read or default memo is produced.
#[test_case::test_case(SkippedVariable::OutsideContext; "outside context")]
#[test_case::test_case(SkippedVariable::ParamSpec; "ParamSpec")]
#[test_case::test_case(SkippedVariable::ParamSpecArgs; "ParamSpec args attribute")]
#[test_case::test_case(SkippedVariable::ParamSpecKwargs; "ParamSpec kwargs attribute")]
fn constructed_ineligible_variable_skips_lazy_metadata(case: SkippedVariable) {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let binding = BindingContext::Synthetic(prepared.program_file().program(&db));
    let kind = match case {
        SkippedVariable::OutsideContext => TypeVarKind::Pep695TypeVar,
        SkippedVariable::ParamSpec
        | SkippedVariable::ParamSpecArgs
        | SkippedVariable::ParamSpecKwargs => TypeVarKind::Pep695ParamSpec,
    };
    let variable = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(&db, Name::new_static("P"), None, kind),
            Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            None,
            Some(TypeVarDefaultEvaluation::Lazy),
        ),
        binding,
        None,
        TypeVarNonce::NONE,
    );
    let context = match case {
        SkippedVariable::OutsideContext => GenericContext::from_typevar_instances(&db, &env, []),
        SkippedVariable::ParamSpec
        | SkippedVariable::ParamSpecArgs
        | SkippedVariable::ParamSpecKwargs => {
            GenericContext::from_typevar_instances(&db, &env, [variable])
        }
    };
    let variable = match case {
        SkippedVariable::OutsideContext | SkippedVariable::ParamSpec => variable,
        SkippedVariable::ParamSpecArgs => {
            variable.with_paramspec_attr(&db, crate::types::ParamSpecAttrKind::Args)
        }
        SkippedVariable::ParamSpecKwargs => {
            variable.with_paramspec_attr(&db, crate::types::ParamSpecAttrKind::Kwargs)
        }
    };
    let request = ConstructedRequest {
        variable,
        input: Type::TypeVar(variable),
        context,
    };
    observations::reset(None);
    let recording = Recording::start();
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    drop(recording);
    assert_eq!(
        captured.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    );
    assert!(captured.reads.is_empty());
    assert_eq!(
        captured.value,
        Ok(AnalysisOutcome::Complete(Type::TypeVar(variable)))
    );
    let ingredient = bound_typevar_default_ingredient(&db);
    assert!(
        !captured
            .reads
            .iter()
            .any(|read| read.key == ingredient.database_key_index(variable.as_id()))
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, variable.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_drained();
}

/// Top-level regular generic callables retain the separate `CallGenericFreshening` refusal.
#[test]
fn regular_generic_callable_preserves_exact_freshening_refusal() {
    let db = database("def identity[T](value: T) -> T: ...\nidentity(True)\n");
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        request(&prepared, CallSite::Module, Adapter::Local),
        &funded(),
    );
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("regular generic refusal: {result:?}");
    };
    assert_eq!(
        reason,
        AnalysisIncomplete::UnavailableOperation(OperationId::CallGenericFreshening)
    );
    assert_drained();
}

/// Type-variable class-object mapping retains the separately unavailable conversion child.
#[test]
fn constructed_subclass_mapping_preserves_exact_refusal() {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut request = constructed_request(&db, &prepared, EagerMetadata::Absent);
    request.input = SubclassOfType::from(&db, &env, request.variable);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &funded());
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::FreshenBoundTypeVars(
            MaterializationOperation::LegacyContinuation
        )))
    );
    assert_drained();
}

/// Supplies a complete signature to the same retained root used by constructor overload freshening.
#[derive(Clone, Copy, Debug)]
struct SignatureRequest<'a, 'db> {
    signature: &'a Signature<'db>,
    context: GenericContext<'db>,
}

impl<'db> MemberOperation<'db> for SignatureRequest<'_, 'db> {
    type Output = Signature<'db>;

    /// Freshens the signature through the production single-signature retained entry point.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        ConstructorMatchingEffects::freshen_signature(
            &effects,
            access.db(),
            &env,
            self.signature,
            self.context,
            7,
        )
        .await
    }
}

/// Distinguishes absent constraints, source-free terminals, and an actual conditional graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiverConstraints {
    Absent,
    True,
    False,
    Conditional,
}

/// Signature freshening preserves terminal constraints and refuses conditional reconstruction precisely.
#[test_case::test_case(ReceiverConstraints::Absent; "absent receiver constraints")]
#[test_case::test_case(ReceiverConstraints::True; "true receiver constraints")]
#[test_case::test_case(ReceiverConstraints::False; "false receiver constraints")]
#[test_case::test_case(ReceiverConstraints::Conditional; "conditional receiver constraints")]
fn constructed_signature_receiver_constraints_keep_their_boundary(case: ReceiverConstraints) {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = constructed_request(&db, &prepared, EagerMetadata::Absent);
    let signature = Signature::new(Parameters::empty(), Type::TypeVar(input.variable));
    let signature = match case {
        ReceiverConstraints::Absent => signature,
        ReceiverConstraints::True => {
            signature.with_probe_receiver_constraints(OwnedConstraintSet::always())
        }
        ReceiverConstraints::False => signature.with_probe_receiver_constraints(
            ConstraintSetBuilder::new()
                .into_owned(|builder| ConstraintSet::from_bool(builder, false)),
        ),
        ReceiverConstraints::Conditional => {
            let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
                ConstraintSet::constrain_typevar_equivalence_bound(
                    &db,
                    &env,
                    builder,
                    input.variable,
                    TypeFormType::from_type_expression(&db, Type::int_literal(1)),
                )
            });
            constraints.query(|_, set| assert!(set.to_owned_terminal().is_none()));
            signature.with_probe_receiver_constraints(constraints)
        }
    };
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        SignatureRequest {
            signature: &signature,
            context: input.context,
        },
        &funded(),
    );
    drop(recording);
    assert_drained();
    match case {
        ReceiverConstraints::Conditional => {
            assert_eq!(
                result,
                Ok(unavailable(OperationId::FreshenBoundTypeVars(
                    MaterializationOperation::Leaf(MappingOperation::SignatureReceiverConstraints)
                )))
            );
        }
        ReceiverConstraints::Absent | ReceiverConstraints::True | ReceiverConstraints::False => {
            let Ok(AnalysisOutcome::Complete(mapped)) = result else {
                panic!("terminal receiver mapping: {result:?}");
            };
            let expected = signature.apply_type_mapping_impl(
                &db,
                &TypeMapping::FreshenBoundTypeVars {
                    generic_context: input.context,
                    delta: 7,
                },
                TypeContext::default(),
                &crate::types::ApplyTypeMappingVisitor::new(&env),
            );
            assert_eq!(mapped, expected);
            assert_eq!(
                mapped.receiver_constraints().is_some(),
                case == ReceiverConstraints::False
            );
        }
    }
}

/// Borrows a constructed tree and the quotation for inspecting its entry identities.
#[derive(Clone, Copy, Debug)]
struct TreeRequest<'tree, 'db> {
    source: &'tree Bindings<'db>,
    entries: usize,
}

/// Retains the matched tree and its entry addresses before matching changed the entries.
#[derive(Debug)]
struct MatchedTree<'db> {
    bindings: Bindings<'db>,
    order: Vec<usize>,
    property_entries: Vec<usize>,
}

impl<'db> MemberOperation<'db> for TreeRequest<'_, 'db> {
    type Output = MatchedTree<'db>;

    /// Clones a tree through admitted ownership operations, then matches it using production effects.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        let mut bindings = clone_bindings_with(self.source, &effects).await?;
        let (order, property_entries) = effects
            .local_with_fixed_transfers(
                self.entries.saturating_mul(32).saturating_add(32),
                self.entries.saturating_mul(4 * size_of::<usize>()),
                || {
                    (
                        matching_entry_order(&bindings),
                        property_entry_ids(&bindings),
                    )
                },
            )
            .await?;
        bindings
            .match_parameters_with(access.db(), &env, &CallArguments::default(), &effects)
            .await?;
        effects
            .local_with_fixed_transfers(4, size_of::<MatchedTree<'db>>(), || ())
            .await?;
        Ok(MatchedTree {
            bindings,
            order,
            property_entries,
        })
    }
}

/// Deep, wide, union and intersection trees visit each entry before its downstream tree and siblings.
/// Matching leaves property-error-owned trees as cleanup-only children.
#[test_case::test_case(TreeShape::Deep(12); "deep downstream tree")]
#[test_case::test_case(TreeShape::Wide(8); "wide downstream tree")]
#[test_case::test_case(TreeShape::Union; "union entries")]
#[test_case::test_case(TreeShape::Intersection; "intersection entries")]
#[test_case::test_case(TreeShape::PropertyErrors; "property error ownership")]
fn constructed_tree_matching_preserves_order_and_state(shape: TreeShape) -> anyhow::Result<()> {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let regular = Bindings::from(CallableBinding::from_overloads(
        Type::unknown(),
        [Signature::new(Parameters::empty(), Type::unknown())],
    ));
    let constructor = regular
        .clone()
        .into_constructor_bindings(Type::any(), ConstructorCallableKind::Init);
    let source = build_tree(&constructor, &regular, shape)?;
    let entries = matching_entry_order(&source).len() + property_entry_ids(&source).len();
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        TreeRequest {
            source: &source,
            entries,
        },
        &funded(),
    );
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        anyhow::bail!("constructed tree matching: {result:?}");
    };
    assert!(!trace.overflowed, "{trace:?}");
    let actual = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::Entry(ConstructorMatchingPhase::Matching, entry)) => Some(*entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, result.order);
    for entry in &result.property_entries {
        assert!(
            !trace
                .events
                .iter()
                .any(|event| matches!(event, Some(Event::Entry(_, actual)) if actual == entry))
        );
    }
    if shape == TreeShape::PropertyErrors {
        assert!(!result.property_entries.is_empty());
    }
    assert_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = source.match_parameters(&db, &env, &CallArguments::default());
    assert_eq!(format!("{:#?}", result.bindings), format!("{ordinary:#?}"));
    Ok(())
}

/// Determines whether a cold canonical expression reaches the matched-tree transfer.
fn expression_reaches_transfer(policy: &AnalysisPolicy) -> bool {
    let db = database("class Product: pass\nProduct()\n");
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start();
    let _result = expression_type_with_policy(&prepared, expression_key(&prepared), policy);
    let trace = recording.trace();
    drop(recording);
    assert_drained();
    trace.events.contains(&Some(Event::AfterMatching(
        ConstructorMatchingOperation::ResultTransfer,
    )))
}

/// A refusal at matched-tree transfer leaves the enclosing expression unpublished while completed
/// constructor-member children remain reusable. A same-revision retry reaches the separate checker.
#[test_case::test_case(Resource::Work; "expression work refusal")]
#[test_case::test_case(Resource::Bytes; "expression byte refusal")]
fn refused_matching_does_not_publish_its_canonical_parent(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(expression_reaches_transfer(&resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if expression_reaches_transfer(&resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database("class Product: pass\nProduct()\n");
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start();
    let result =
        expression_type_with_policy(&prepared, expression_key(&prepared), &resource.policy(low));
    let trace = recording.trace();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert!(trace.events.contains(&Some(Event::BeforeMatching(
        ConstructorMatchingOperation::ResultTransfer
    ))));
    assert!(!trace.events.contains(&Some(Event::AfterMatching(
        ConstructorMatchingOperation::ResultTransfer
    ))));
    assert_drained();
    let events = event_db.take_salsa_events();
    let parent = find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events)
        .expect("the enclosing expression did not execute");
    let salsa::EventKind::WillExecute {
        database_key: parent,
    } = parent.kind
    else {
        panic!("expected enclosing expression execution");
    };
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            parent.key_index()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let child = find_will_execute_event_by_name(&db, "lookup_dunder_new_inner", None, &events)
        .expect("the constructor member did not execute");
    let salsa::EventKind::WillExecute {
        database_key: child,
    } = child.kind
    else {
        panic!("expected constructor-member execution");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            crate::types::LookupDunderNewQuery::ingredient(&db),
            child.key_index()
        )
        .is_ok()
    );
    let recording = Recording::start();
    let retry = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let trace = recording.trace();
    drop(recording);
    assert_eq!(retry, Ok(unavailable(OperationId::CheckerConstructor)));
    assert!(trace.events.contains(&Some(Event::AfterMatching(
        ConstructorMatchingOperation::ResultTransfer
    ))));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            parent.key_index()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "lookup_dunder_new_inner",
        Some(child.key_index()),
        &event_db.take_salsa_events(),
    );
    assert_drained();
}

/// Constructor ParamSpec eligibility is checked before the seen set or nonce changes.
/// Class inference is explicit constructed-input setup; this does not claim a cold source result.
/// The chained assignment gives its RHS a standalone expression key for that setup inference.
#[test]
fn constructed_paramspec_constructor_skips_nonce_mutation() {
    let db = database("class Box[**P]: pass\nvalue = alias = Box\n");
    let prepared = prepare(&db);
    let expression = expression(&db);
    let inference = infer_expression_types(&db, expression, TypeContext::default());
    let Type::ClassLiteral(class) = inference.expression_type(expression.node_ref(&db)) else {
        panic!("the constructed fixture must name its generic class");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let instance = Type::instance(&db, &env, class.identity_specialization(&db));
    let source = Bindings::from(CallableBinding::from_overloads(
        Type::unknown(),
        [Signature::new(Parameters::empty(), Type::unknown())],
    ))
    .into_constructor_bindings(instance, ConstructorCallableKind::Init);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        TreeRequest {
            source: &source,
            entries: 1,
        },
        &funded(),
    );
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        panic!("constructed ParamSpec matching: {result:?}");
    };
    assert!(!trace.events.contains(&Some(Event::BeforeNonce(
        ConstructorNonceOperation::SeenInsert
    ))));
    assert!(!trace.events.contains(&Some(Event::BeforeNonce(
        ConstructorNonceOperation::Increment
    ))));
    assert!(!trace.events.iter().any(|event| matches!(
        event,
        Some(Event::Entry(ConstructorMatchingPhase::Freshening, _))
    )));
    assert_drained();
    let ordinary = source.match_parameters(&db, &env, &CallArguments::default());
    assert_eq!(format!("{:#?}", result.bindings), format!("{ordinary:#?}"));
}

/// Signature-owned `Self` and the class variable in its bound freshen together with the receiver.
#[test]
fn cold_signature_self_shares_the_fresh_class_receiver() {
    let db = database(
        "from typing import Self\nclass Box[T]:\n    def __init__(self, value: Self) -> None: ...\n    def again(self, value: T):\n        Box[T](self)\n",
    );
    let prepared = prepare(&db);
    let request = request(&prepared, CallSite::Again, Adapter::Local);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        panic!("cold signature Self freshening: {result:?}");
    };
    assert_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let binding = result.bindings.single_element().unwrap();
    let receiver = binding.bound_type.unwrap();
    let Type::TypeVar(signature_self) = binding.overloads()[0]
        .signature
        .parameters()
        .iter()
        .last()
        .unwrap()
        .annotated_type()
    else {
        panic!("the initializer parameter must retain signature-owned Self");
    };
    assert!(signature_self.typevar(&db).is_self(&db));
    assert_ne!(signature_self.freshness(&db).value(), 0);
    assert_eq!(
        signature_self.typevar(&db).upper_bound(&db, &env),
        Some(receiver)
    );
    assert_ordinary_parity(&db, request, &result);
}

/// A caller's `Self` used as an explicit class argument keeps its original identity and bound.
/// The eligible constructor consumes its nonce, then skips installation when mapping leaves the instance unchanged.
#[test]
fn cold_caller_self_keeps_its_original_bound() {
    let db = database(
        "from typing import Self\nclass Box[T]:\n    def __init__(self, value: T) -> None: ...\n    def again(self, value: Self):\n        Box[Self](value)\n",
    );
    let prepared = prepare(&db);
    let request = request(&prepared, CallSite::Again, Adapter::Local);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &funded());
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        panic!("cold caller Self matching: {result:?}");
    };
    assert_drained();
    assert!(trace.events.contains(&Some(Event::AfterNonce(
        ConstructorNonceOperation::Increment
    ))));
    assert!(
        trace
            .events
            .iter()
            .any(|event| matches!(event, Some(Event::MappingEntered { .. })))
    );
    assert!(!trace.events.contains(&Some(Event::BeforeMatching(
        ConstructorMatchingOperation::SignatureInstall
    ))));
    let binding = result.bindings.single_element().unwrap();
    let Type::TypeVar(caller_self) = binding.overloads()[0]
        .signature
        .parameters()
        .iter()
        .last()
        .unwrap()
        .annotated_type()
    else {
        panic!("the explicit class argument must retain caller-owned Self");
    };
    assert!(caller_self.typevar(&db).is_self(&db));
    assert_eq!(caller_self.freshness(&db), TypeVarNonce::NONE);
    assert_ordinary_parity(&db, request, &result);
}

/// A starred variadic parameter keeps the precise unpacked-matching refusal within a constructor.
#[test]
fn constructed_unpacked_parameter_preserves_exact_refusal() {
    let db = database(CONSTRUCTED);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let parameter = Parameter::variadic(Name::new_static("args"))
        .with_annotated_type(Type::heterogeneous_tuple(
            &db,
            &env,
            [Type::bool_literal(true)],
        ))
        .with_starred_annotation();
    let source = Bindings::from(CallableBinding::from_overloads(
        Type::unknown(),
        [Signature::new(
            Parameters::standard([parameter]),
            Type::unknown(),
        )],
    ))
    .into_constructor_bindings(Type::any(), ConstructorCallableKind::Init);
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        TreeRequest {
            source: &source,
            entries: 1,
        },
        &funded(),
    );
    drop(recording);
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::CallUnpackedMatching),
                completed: (),
            })
        ),
        "{result:?}"
    );
    assert_drained();
}

/// Retains a constructor seed, its regular sibling seed, and the unchanged constructed instance.
#[derive(Debug)]
struct GenericSeed<'db> {
    constructor: Bindings<'db>,
    regular: Bindings<'db>,
    instance: Type<'db>,
}

/// Builds a generic constructor seed from an ordinary identity-specialized class fixture.
/// Only the subsequent constructed tree is the controlled input; class inference is setup.
/// The fixture's chained assignment gives its RHS a standalone expression key.
fn generic_constructor_seed<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> GenericSeed<'db> {
    let expression = expression(db);
    let inference = infer_expression_types(db, expression, TypeContext::default());
    let Type::ClassLiteral(class) = inference.expression_type(expression.node_ref(db)) else {
        panic!("the constructed fixture must name its generic class");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let context = class.generic_context(db).unwrap();
    let variable = context.variables(db).next().unwrap();
    let instance = Type::instance(db, &env, class.identity_specialization(db));
    let mut signature = Signature::new(
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_annotated_type(instance),
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::TypeVar(variable)),
        ]),
        Type::unknown(),
    );
    signature.generic_context = Some(context);
    let constructor = Bindings::from(
        CallableBinding::from_overloads(Type::unknown(), [signature]).with_bound_type(instance),
    )
    .into_constructor_bindings(instance, ConstructorCallableKind::Init);
    let regular = Bindings::from(CallableBinding::from_overloads(
        Type::unknown(),
        [Signature::new(Parameters::empty(), Type::unknown())],
    ));
    GenericSeed {
        constructor,
        regular,
        instance,
    }
}

/// Generic root siblings share one nonce generator, while each root shares its delta with all
/// downstream constructors. Regular siblings do not consume nonces, and stored source instances stay unchanged.
#[test]
fn constructed_generic_downstream_trees_share_deltas_and_continue_nonces() -> anyhow::Result<()> {
    let db = database("class Box[T]: pass\nvalue = alias = Box\n");
    let prepared = prepare(&db);
    let seed = generic_constructor_seed(&db, &prepared);
    let source = build_tree(&seed.constructor, &seed.regular, TreeShape::Union)?;
    let entries = matching_entry_order(&source).len();
    let original = constructor_snapshots(&source);
    assert_eq!(
        original.iter().map(|entry| entry.root).collect::<Vec<_>>(),
        vec![0, 0, 0, 1, 1, 1, 2, 2, 2]
    );
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        TreeRequest {
            source: &source,
            entries,
        },
        &funded(),
    );
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        anyhow::bail!("constructed generic downstream matching: {result:?}");
    };
    assert!(!trace.overflowed, "{trace:?}");
    assert_eq!(trace.events.iter().filter(|event| **event == Some(Event::AfterNonce(ConstructorNonceOperation::Increment))).count(), 2);
    let actual_order = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::Entry(ConstructorMatchingPhase::Matching, entry)) => Some(*entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_order, result.order);
    let freshened = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::Entry(ConstructorMatchingPhase::Freshening, entry)) => Some(*entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(freshened.len(), 6);
    assert_eq!(
        freshened,
        result
            .order
            .iter()
            .copied()
            .filter(|entry| freshened.contains(entry))
            .collect::<Vec<_>>()
    );
    let last_freshening = trace
        .events
        .iter()
        .rposition(|event| {
            matches!(
                event,
                Some(Event::Entry(ConstructorMatchingPhase::Freshening, _))
            )
        })
        .unwrap();
    let first_matching = trace
        .events
        .iter()
        .position(|event| {
            matches!(
                event,
                Some(Event::Entry(ConstructorMatchingPhase::Matching, _))
            )
        })
        .unwrap();
    assert!(last_freshening < first_matching);
    let mapped = constructor_snapshots(&result.bindings);
    assert_eq!(mapped.len(), original.len());
    let env = ProgramEnvironment::from_file(prepared.program_file());
    for (mapped, original) in mapped.iter().zip(&original) {
        assert_eq!((mapped.root, mapped.depth), (original.root, original.depth));
        assert_eq!(mapped.stored_instance, seed.instance);
        assert_eq!(mapped.stored_instance, original.stored_instance);
        let receiver = mapped.receiver.unwrap();
        let (_, specialization) = receiver.class_specialization(&db, &env).unwrap();
        let Type::TypeVar(variable) = specialization.types(&db)[0] else {
            anyhow::bail!("the receiver must retain the class type variable");
        };
        assert_eq!(
            usize::try_from(variable.freshness(&db).value())?,
            mapped.root
        );
        for (signature, instance) in mapped.signatures.iter().zip(&mapped.overload_instances) {
            assert_eq!(
                signature
                    .parameters()
                    .iter()
                    .last()
                    .unwrap()
                    .annotated_type(),
                Type::TypeVar(variable)
            );
            if mapped.root == 0 {
                assert_eq!(*instance, None);
            } else {
                assert_eq!(*instance, Some(receiver));
            }
        }
    }
    assert_drained();
    let ordinary = source.match_parameters(&db, &env, &CallArguments::default());
    assert_eq!(format!("{:#?}", result.bindings), format!("{ordinary:#?}"));
    Ok(())
}

/// Generic freshening and matching skip property-error-owned trees while retaining their cleanup owners.
/// An enclosing class context forces a real freshening pass through every main-tree constructor.
#[test]
fn constructed_generic_property_errors_remain_cleanup_only() -> anyhow::Result<()> {
    let db = database("class Box[T]: pass\nvalue = alias = Box\n");
    let prepared = prepare(&db);
    let seed = generic_constructor_seed(&db, &prepared);
    let mut source = build_tree(&seed.constructor, &seed.regular, TreeShape::PropertyErrors)?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (_, specialization) = seed.instance.class_specialization(&db, &env).unwrap();
    let Type::TypeVar(variable) = specialization.types(&db)[0] else {
        anyhow::bail!("the constructor seed must retain its class variable");
    };
    source.set_enclosing_binding_contexts([variable.binding_context(&db)]);
    let constructors = constructor_snapshots(&source).len();
    let entries = matching_entry_order(&source).len() + property_entry_ids(&source).len();
    observations::reset(None);
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        TreeRequest {
            source: &source,
            entries,
        },
        &funded(),
    );
    let trace = recording.trace();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = result else {
        anyhow::bail!("generic property-error tree matching: {result:?}");
    };
    assert!(!trace.overflowed, "{trace:?}");
    assert_eq!(trace.events.iter().filter(|event| **event == Some(Event::AfterNonce(ConstructorNonceOperation::Increment))).count(), 1);
    let freshened = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::Entry(ConstructorMatchingPhase::Freshening, entry)) => Some(*entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(freshened.len(), constructors);
    assert_eq!(
        freshened,
        result
            .order
            .iter()
            .copied()
            .filter(|entry| freshened.contains(entry))
            .collect::<Vec<_>>()
    );
    let matched = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Some(Event::Entry(ConstructorMatchingPhase::Matching, entry)) => Some(*entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(matched, result.order);
    assert!(!result.property_entries.is_empty());
    for property_entry in &result.property_entries {
        assert!(
            !trace.events.iter().any(
                |event| matches!(event, Some(Event::Entry(_, entry)) if entry == property_entry)
            )
        );
    }
    assert_drained();
    let ordinary = source.match_parameters(&db, &env, &CallArguments::default());
    assert_eq!(format!("{:#?}", result.bindings), format!("{ordinary:#?}"));
    Ok(())
}
