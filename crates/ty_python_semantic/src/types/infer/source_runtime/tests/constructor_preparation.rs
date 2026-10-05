//! Checks direct constructor preparation without matching arguments or checking a constructor call.
//! Fixtures enter through controlled definition inference and retain the supplied callable guard.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use ruff_db::system::SystemPathBuf;
use salsa::plumbing::ZalsaDatabase;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::analysis::BoundMethodPreparationOperation;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::ownership::observations as owner_observations;
use crate::types::call::invocation::{InvocationContext, InvocationEffects};
use crate::types::cyclic::entry::{
    CallableEntryDecision, CallableEntryFacts, CallableGuardEntryEffects,
    callable_enter_in_place_with,
};
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::cyclic::{CallableExpansion, CallableRecursionGuard, CallableVisitScope};
use crate::types::{BoundMethodType, LookupDunderNewQuery, Signature};

const EMPTY: &str = "class Product: pass\n";
const DECLARED_INIT: &str = "class Product:\n    def __init__(self, value): ...\n";
const INHERITED_INIT: &str =
    "class Base:\n    def __init__(self, value): ...\nclass Product(Base): pass\n";
const NEW_AND_INIT: &str =
    "class Product:\n    def __new__(cls, value): ...\n    def __init__(self, value): ...\n";
const METACLASS_AND_DOWNSTREAM: &str = "class Meta(type):\n    def __call__(cls, value): ...\nclass Product(metaclass=Meta):\n    def __new__(cls, value): ...\n    def __init__(self, value): ...\n";
const OVERLOADED_INIT: &str = "from typing import overload\nclass Product:\n    @overload\n    def __init__(self, value: int): ...\n    @overload\n    def __init__(self, value: str): ...\n    def __init__(self, value): ...\n";
const ALIASED_INIT: &str =
    "def initialize(self, value): ...\nclass Product:\n    __init__ = initialize\n";
const STORED_CALLABLE_INIT: &str =
    "from typing import Callable\nclass Product:\n    __init__: Callable[[int], None]\n";
const POSSIBLY_UNBOUND_INIT: &str =
    "from settings import condition\nclass Product:\n    if condition:\n        def __init__(self, value): ...\n";
const POSSIBLY_UNBOUND_NEW: &str = "from settings import condition\nclass Product:\n    if condition:\n        def __new__(cls, value): ...\n    def __init__(self, value): ...\n";
const DEFERRED_INIT: &str = "class Product:\n    def __new__(cls, value): ...\n    def __init__(self, value: \"Annotation\"): ...\nclass Annotation: pass\n";

/// Identifies an admitted constructor-preparation mutation observed by the controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    NativeBinding,
    InitialSignature,
    ConstructorWrap,
    DownstreamClone,
    DownstreamInstall,
    Transfer,
}

/// Counts visits to one mutation boundary without changing the admission ledger.
#[derive(Clone, Copy, Debug)]
struct Boundary {
    stage: Stage,
    before: usize,
    after: usize,
    pending_after_mutation: usize,
    child_query: Option<salsa::DatabaseKeyIndex>,
    child_scope: Option<usize>,
    constructor_scope: Option<usize>,
    mutations_before_child: usize,
    owner_retirement: Option<OwnerRetirement>,
    owner_retired_before_child: bool,
}

/// Locates one actual `__new__` bindings retirement in the guard and child event sequence.
#[derive(Clone, Copy, Debug)]
struct OwnerRetirement {
    id: usize,
    /// Number of guard and child events already recorded at destructor entry.
    begin: usize,
    /// Number of guard and child events already recorded after all owned storage is released.
    complete: Option<usize>,
}

thread_local! {
    static BOUNDARY: RefCell<Option<Boundary>> = const { RefCell::new(None) };
}

/// Records the first observed retirement of wrapped `__new__` bindings and its completion.
/// The deferred fixture creates one such root and clones only its initializer downstream;
/// its cancellation assertions also reject any retirement observed before the annotation begins.
fn observe_owner_retirement(event: owner_observations::RetirementEvent<'_>) {
    if event.constructor_kind != Some(ConstructorCallableKind::New) {
        return;
    }
    BOUNDARY.with_borrow_mut(|recording| {
        if let Some(recording) = recording {
            let snapshot = lifetime_observations::snapshot();
            assert!(!snapshot.overflowed, "{snapshot:?}");
            match event.phase {
                owner_observations::RetirementPhase::Begin => {
                    if recording.owner_retirement.is_none() {
                        recording.owner_retirement = Some(OwnerRetirement {
                            id: event.id,
                            begin: snapshot.count,
                            complete: None,
                        });
                    }
                }
                owner_observations::RetirementPhase::Complete => {
                    if let Some(retirement) = &mut recording.owner_retirement
                        && retirement.id == event.id
                    {
                        retirement.complete = Some(snapshot.count);
                    }
                }
            }
        }
    });
}

/// Records arrival immediately before the mutation's normal admission.
pub(in crate::types::infer) fn observe_before(stage: Stage) {
    BOUNDARY.with_borrow_mut(|recording| {
        if let Some(recording) = recording
            && recording.stage == stage
        {
            recording.before += 1;
        }
    });
}

/// Records a mutation only after its normal admission succeeds and storage has changed.
pub(in crate::types::infer) fn observe_after(stage: Stage) {
    BOUNDARY.with_borrow_mut(|recording| {
        if let Some(recording) = recording
            && recording.stage == stage
        {
            recording.after += 1;
            if stage == Stage::ConstructorWrap && recording.constructor_scope.is_none() {
                recording.constructor_scope = innermost_scope();
            }
        }
    });
}

/// Observes a real suspension after the selected constructor mutation has completed.
fn observe_pending() {
    BOUNDARY.with_borrow_mut(|recording| {
        if let Some(recording) = recording
            && recording.after > 0
        {
            recording.pending_after_mutation += 1;
        }
    });
}

/// Limits one boundary recording to the controlled request, including its cleanup.
#[derive(Debug)]
struct Recording {
    previous_observer: Option<for<'db> fn(owner_observations::RetirementEvent<'db>)>,
}

impl Recording {
    fn start(stage: Stage) -> Self {
        BOUNDARY.with_borrow_mut(|recording| {
            assert!(recording.is_none());
            *recording = Some(Boundary {
                stage,
                before: 0,
                after: 0,
                pending_after_mutation: 0,
                child_query: None,
                child_scope: None,
                constructor_scope: None,
                mutations_before_child: 0,
                owner_retirement: None,
                owner_retired_before_child: false,
            });
        });
        lifetime_observations::reset();
        Self {
            previous_observer: owner_observations::set_retirement_observer(Some(
                observe_owner_retirement,
            )),
        }
    }

    fn boundary(&self) -> Boundary {
        BOUNDARY.with_borrow(|recording| recording.unwrap())
    }

    /// Selects the canonical child whose currently active expansion scope will be recorded.
    fn watch_child(&self, key: salsa::DatabaseKeyIndex) {
        BOUNDARY.with_borrow_mut(|recording| recording.as_mut().unwrap().child_query = Some(key));
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        owner_observations::set_retirement_observer(self.previous_observer);
        lifetime_observations::stop();
        BOUNDARY.with_borrow_mut(|recording| *recording = None);
    }
}

/// Selects whether the fixture class is already active in the supplied guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardEntry {
    Fresh,
    Exact,
}

/// Requests preparation for a source class whose definition has not yet been inferred.
#[derive(Clone, Copy, Debug)]
struct Request<'db> {
    definition: Definition<'db>,
    guard_entry: GuardEntry,
}

/// Keeps the class identity alongside the prepared bindings for ordinary-path comparison.
#[derive(Debug)]
struct PreparedConstructor<'db> {
    class: Type<'db>,
    bindings: Bindings<'db>,
}

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = PreparedConstructor<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access.definition(self.definition).await?;
        let endpoint = access.endpoint();
        let class = endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                inference
                    .original_class_type(self.definition)
                    .map(Type::ClassLiteral)
                    .ok_or(RunError::Contract("fixture definition is not a class"))
            })
            .await;
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        let guard = InvocationEffects::new_guard(&effects).await?;
        let before = lifetime_observations::guard_state(&guard);
        let mut active = None;
        if self.guard_entry == GuardEntry::Exact {
            active = Some(
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(size_of::<CallableVisitScope<'_, 'db>>() * 2 + 1)?;
                        endpoint.check_completion()?;
                        Ok(guard.begin_scope())
                    })
                    .await,
            );
            if let Some(scope) = &mut active {
                assert_eq!(
                    callable_enter_in_place_with(
                        (CallableExpansion::Bindings, class),
                        scope,
                        CallableEntryFacts,
                        &effects,
                    )
                    .await?,
                    CallableEntryDecision::Entered,
                );
            }
        }
        let arguments = CallArguments::default();
        let bindings = {
            let preparation = InvocationEffects::prepare(
                &effects,
                InvocationContext {
                    db: access.db(),
                    env: &env,
                    arguments: &arguments,
                },
                class,
                &guard,
            );
            let mut preparation = std::pin::pin!(preparation);
            poll_fn(|context| {
                let result = preparation.as_mut().poll(context);
                if result.is_pending() {
                    observe_pending();
                }
                result
            })
            .await?
        };
        if let Some(scope) = &active {
            assert!(
                CallableGuardEntryEffects::contains_exact(
                    &effects,
                    scope,
                    (CallableExpansion::Bindings, class),
                )
                .await?,
            );
        }
        drop(active);
        assert_eq!(lifetime_observations::guard_state(&guard), before);
        Ok(PreparedConstructor { class, bindings })
    }
}

/// Creates a fresh database so each controlled preparation starts without semantic query memos.
fn database(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db
}

/// Creates a cold fixture with an imported boolean that is bound but may be true or false.
fn conditional_database(source: &str) -> TestDb {
    let mut db = database(source);
    db.write_file("src/settings.pyi", "condition: bool\n").unwrap();
    db
}

/// Records the innermost active expansion scope when the selected canonical child begins.
fn observe_query_entry(event: &salsa::EventKind) {
    let salsa::EventKind::WillExecute { database_key } = *event else {
        return;
    };
    BOUNDARY.with_borrow_mut(|recording| {
        if let Some(recording) = recording
            && recording.child_query == Some(database_key)
        {
            recording.child_scope = innermost_scope();
            recording.mutations_before_child = recording.after;
            recording.owner_retired_before_child = recording.owner_retirement.is_some();
        }
    });
}

/// Finds the opening event for the innermost expansion scope that is currently active.
fn innermost_scope() -> Option<usize> {
    let snapshot = lifetime_observations::snapshot();
    assert!(!snapshot.overflowed, "{snapshot:?}");
    let mut active = Vec::new();
    for (index, event) in snapshot.events[..snapshot.count].iter().enumerate() {
        match event {
            Some(lifetime_observations::Event::ScopeOpened(_)) => active.push(index),
            Some(lifetime_observations::Event::ScopeDropAfter(_)) => {
                active.pop().expect("a scope closed without opening");
            }
            Some(
                lifetime_observations::Event::ScopeDropBefore(_)
                | lifetime_observations::Event::StorageDropped { .. }
                | lifetime_observations::Event::RelationConversion { .. }
                | lifetime_observations::Event::RelationDropBefore { .. }
                | lifetime_observations::Event::RelationDropAfter { .. }
                | lifetime_observations::Event::SourceChildDropped { .. },
            )
            | None => {}
        }
    }
    active.last().copied()
}

/// Creates the deferred-annotation fixture with canonical-query entry observation enabled.
fn deferred_database() -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", DEFERRED_INIT)
        .with_salsa_event_callback(observe_query_entry)
        .build()
        .unwrap()
}

/// Finds the fixture's class definition without requesting its inferred type.
fn request<'db>(prepared: &PreparedAnalysisFile<'db>, guard_entry: GuardEntry) -> Request<'db> {
    Request {
        definition: class_definition(prepared, "Product"),
        guard_entry,
    }
}

/// Finds a named top-level class definition from the prepared syntax without inferring its type.
fn class_definition<'db>(prepared: &PreparedAnalysisFile<'db>, name: &str) -> Definition<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap();
    prepared.semantic_index().expect_single_definition(class)
}

/// Checks that all observed expansion scopes close before their admitted guard storage retires.
fn assert_guard_drained() {
    let snapshot = lifetime_observations::snapshot();
    assert!(!snapshot.overflowed, "{snapshot:?}");
    assert_eq!(snapshot.active_scopes, 0, "{snapshot:?}");
    let events = &snapshot.events[..snapshot.count];
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, Some(lifetime_observations::Event::ScopeOpened(_))) })
    );
    let Some(Some(lifetime_observations::Event::StorageDropped {
        id,
        outstanding_removal_weights,
    })) = events.last()
    else {
        panic!("guard storage did not retire after its scopes: {snapshot:?}");
    };
    assert_eq!(*outstanding_removal_weights, [0; 3]);
    let restored = events
        .iter()
        .rev()
        .find_map(|event| match event {
            Some(lifetime_observations::Event::ScopeDropAfter(state)) if state.id == *id => {
                Some(*state)
            }
            _ => None,
        })
        .expect("the retired guard did not report scope restoration");
    assert_eq!(restored.exact, 0, "{snapshot:?}");
    assert_eq!(restored.identities, 0, "{snapshot:?}");
    assert_eq!(restored.definitions, 0, "{snapshot:?}");
    assert_eq!(restored.anchors, 0, "{snapshot:?}");
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Compares every prepared field, including downstream constructors, with ordinary preparation.
/// The ordinary read happens after the cold controlled result; debug output exposes the private
/// binding tree, so this comparison includes constructor contexts and source-parameter offsets.
fn assert_parity(source: &str, overloads: usize) -> PreparedFacts {
    let db = database(source);
    assert_database_parity(&db, overloads)
}

/// Summarizes whether constructor preparation retained either missing-implicit-call flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreparedFlags {
    new: bool,
    init: bool,
}

/// Retains fixture assertions about implicit-call flags, receiver binding, and normalized returns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreparedFacts {
    flags: PreparedFlags,
    receiver_is_constructed_instance: bool,
    return_is_constructed_instance: bool,
}

/// Checks cold and ordinary preparation in the supplied database, including custom typeshed fixtures.
fn assert_database_parity(db: &TestDb, overloads: usize) -> PreparedFacts {
    let prepared = prepare(db);
    observations::reset(None);
    lifetime_observations::reset();
    let captured = capture(db, || {
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded())
    })
    .unwrap();
    lifetime_observations::stop();
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, bindings })) = captured.value
    else {
        panic!("cold constructor preparation: {:?}", captured.value);
    };
    assert_guard_drained();
    assert!(bindings.has_only_constructor_items());
    assert_eq!(
        bindings
            .iter_flat()
            .flat_map(IntoIterator::into_iter)
            .count(),
        overloads,
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let Type::ClassLiteral(class_literal) = class else {
        panic!("fixture constructor is not a class literal");
    };
    let instance = Type::instance(db, &env, ClassType::NonGeneric(class_literal));
    let ordinary = class.bindings_impl(db, &env, &CallableRecursionGuard::new());
    assert_eq!(format!("{bindings:#?}"), format!("{ordinary:#?}"));
    PreparedFacts {
        flags: PreparedFlags {
            new: bindings.has_implicit_dunder_new_is_possibly_unbound(),
            init: bindings.has_implicit_dunder_init_is_possibly_unbound(),
        },
        receiver_is_constructed_instance: bindings
            .iter_flat()
            .all(|binding| binding.bound_type == Some(instance)),
        return_is_constructed_instance: bindings
            .iter_flat()
            .flat_map(IntoIterator::into_iter)
            .all(|binding| binding.return_ty == instance),
    }
}

/// An empty class prepares the inherited object initializer without entering call checking.
#[test]
fn empty_class_matches_ordinary_preparation() {
    assert_parity(EMPTY, 1);
}

/// A declared initializer retains its bound receiver and normalized constructor return.
#[test]
fn declared_initializer_matches_ordinary_preparation() {
    let facts = assert_parity(DECLARED_INIT, 1);
    assert!(facts.receiver_is_constructed_instance);
    assert!(facts.return_is_constructed_instance);
}

/// An inherited initializer binds the instance of the constructed subclass.
#[test]
fn inherited_initializer_matches_ordinary_preparation() {
    let facts = assert_parity(INHERITED_INIT, 1);
    assert!(facts.receiver_is_constructed_instance);
    assert!(facts.return_is_constructed_instance);
}

/// New-method preparation retains the initializer as a downstream constructor.
#[test]
fn new_and_initializer_match_ordinary_preparation() {
    assert_parity(NEW_AND_INIT, 1);
}

/// A metaclass call retains both the new method and initializer downstream.
#[test]
fn metaclass_and_downstream_match_ordinary_preparation() {
    assert_parity(METACLASS_AND_DOWNSTREAM, 1);
}

/// Every initializer overload survives preparation because argument matching has not begun.
#[test]
fn initializer_overloads_match_ordinary_preparation() {
    assert_parity(OVERLOADED_INIT, 2);
}

/// An aliased function used as the initializer binds through the native function descriptor.
#[test]
fn aliased_initializer_matches_ordinary_preparation() {
    assert_parity(ALIASED_INIT, 1);
}

/// A stored regular callable used as the initializer keeps its already-bound parameter list.
#[test]
fn stored_callable_initializer_matches_ordinary_preparation() {
    assert_parity(STORED_CALLABLE_INIT, 1);
}

/// A conditional initializer remains marked as possibly missing during implicit construction.
#[test]
fn conditional_initializer_preserves_possible_unboundness() {
    let db = conditional_database(POSSIBLY_UNBOUND_INIT);
    let flags = assert_database_parity(&db, 1).flags;
    assert_eq!(
        flags,
        PreparedFlags {
            new: false,
            init: true
        }
    );
}

/// A conditional new method retains its missing-implicit-call flag alongside its initializer.
#[test]
fn conditional_new_method_preserves_possible_unboundness() {
    let db = conditional_database(POSSIBLY_UNBOUND_NEW);
    let flags = assert_database_parity(&db, 1).flags;
    assert_eq!(
        flags,
        PreparedFlags {
            new: true,
            init: false
        }
    );
}

/// If custom typeshed omits object initialization, preparation keeps gradual arguments and a missing-init flag.
#[test]
fn missing_initializer_uses_the_custom_typeshed_recovery() {
    let db = TestDbBuilder::new()
        .with_custom_typeshed(SystemPathBuf::from("/typeshed"))
        .with_file("src/main.py", EMPTY)
        .with_file("/typeshed/stdlib/VERSIONS", "builtins: 3.0-\nenum: 3.0-\n")
        .with_file(
            "/typeshed/stdlib/builtins.pyi",
            "class object: ...\nclass type(object): ...\n",
        )
        .with_file(
            "/typeshed/stdlib/enum.pyi",
            "class Enum: ...\nclass EnumType(type): ...\n",
        )
        .build()
        .unwrap();
    let flags = assert_database_parity(&db, 1).flags;
    assert_eq!(
        flags,
        PreparedFlags {
            new: false,
            init: true
        }
    );
}

/// Checks a precise unsupported child and confirms that its enclosing callable guard drains.
fn assert_child_refusal(source: &str, expected: OperationId) {
    let db = database(source);
    let prepared = prepare(&db);
    observations::reset(None);
    lifetime_observations::reset();
    let result =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded());
    lifetime_observations::stop();
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected constructor child refusal: {result:?}");
    };
    assert_eq!(reason, AnalysisIncomplete::UnavailableOperation(expected));
    assert_guard_drained();
}

/// An unspecialized generic class prepares the same complete constructor bindings as ordinary inference.
/// Preparation uses Product[T], retaining its own T for later argument inference.
#[test]
fn generic_class_identity_specialization_matches_ordinary_preparation() {
    assert_parity(
        "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Product(Generic[T]): pass\n",
        1,
    );
}

/// A generic initializer receiver refuses preparation at its unavailable type-variable search.
#[test]
fn generic_receiver_preserves_typevar_search_refusal() {
    assert_child_refusal(
        "class Product:\n    def __init__[T](self: T): ...\n",
        OperationId::BoundMethodPreparation(BoundMethodPreparationOperation::ReceiverTypevarSearch),
    );
}

/// An exact active class entry selects cycle recovery and remains active until its owner closes.
/// A fresh guard would prepare the initializer, so the unknown signature checks the supplied guard.
#[test]
fn active_supplied_guard_recovers_without_preparing_the_constructor_again() {
    let db = database(DECLARED_INIT);
    let prepared = prepare(&db);
    observations::reset(None);
    lifetime_observations::reset();
    let result =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Exact), &funded());
    lifetime_observations::stop();
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { bindings, .. })) = result else {
        panic!("exact constructor recovery: {result:?}");
    };
    assert_eq!(
        bindings
            .iter_flat()
            .flat_map(IntoIterator::into_iter)
            .map(|binding| &binding.signature)
            .collect::<Vec<_>>(),
        [&Signature::unknown()],
    );
    assert!(!bindings.has_only_constructor_items());
    assert_guard_drained();
}

/// Selects the single resource allowance reduced by a refusal control.
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

/// Reports whether a fresh cold run commits the selected mutation under the supplied policy.
fn reaches_mutation(source: &str, stage: Stage, policy: &AnalysisPolicy) -> bool {
    let db = database(source);
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(stage);
    let _result =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), policy);
    let boundary = recording.boundary();
    drop(recording);
    assert_no_active_attempt();
    boundary.after > 0
}

/// Finds a real admission refusal immediately before a selected storage mutation.
/// Each search run has a fresh database. The refused database itself is then retried at the same
/// revision with the ordinary funding policy; hooks only observe and never manufacture refusal.
fn assert_resource_refusal(source: &str, stage: Stage, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_mutation(source, stage, &resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_mutation(source, stage, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(source);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(stage);
    let result = controlled_member_operation(
        &prepared,
        request(&prepared, GuardEntry::Fresh),
        &resource.policy(low),
    );
    let boundary = recording.boundary();
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected {resource:?} refusal at {stage:?}: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(boundary.before > 0, "{boundary:?}");
    assert_eq!(boundary.after, 0, "{boundary:?}");
    assert_guard_drained();

    let recording = Recording::start(stage);
    let retry =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded());
    let boundary = recording.boundary();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, bindings })) = retry else {
        panic!("funded constructor retry: {retry:?}");
    };
    assert!(boundary.after > 0, "{boundary:?}");
    assert_guard_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = class.bindings_impl(&db, &env, &CallableRecursionGuard::new());
    assert_eq!(format!("{bindings:#?}"), format!("{ordinary:#?}"));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Work exhaustion refuses native receiver binding before its storage is constructed.
#[test]
fn native_binding_work_refusal_precedes_mutation() {
    assert_native_binding_refusal(Resource::Work);
}

/// Byte exhaustion refuses native receiver binding before its storage is constructed.
#[test]
fn native_binding_byte_refusal_precedes_mutation() {
    assert_native_binding_refusal(Resource::Bytes);
}

/// Counts the database's canonical bound-method identities.
fn native_bindings(db: &TestDb) -> usize {
    BoundMethodType::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count()
}

/// Reports whether a fresh controlled request allocates a bound method under the supplied policy.
/// Counting identities observes allocation independently of continuation work after interning.
fn allocates_native_binding(policy: &AnalysisPolicy) -> bool {
    let db = database(DECLARED_INIT);
    let prepared = prepare(&db);
    let before = native_bindings(&db);
    observations::reset(None);
    let _result =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), policy);
    assert_no_active_attempt();
    native_bindings(&db) > before
}

/// Refuses before the first actual bound-method allocation and retries the same database.
fn assert_native_binding_refusal(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(allocates_native_binding(&resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if allocates_native_binding(&resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(DECLARED_INIT);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let before = native_bindings(&db);
    observations::reset(None);
    let recording = Recording::start(Stage::NativeBinding);
    let result = controlled_member_operation(
        &prepared,
        request(&prepared, GuardEntry::Fresh),
        &resource.policy(low),
    );
    let boundary = recording.boundary();
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("expected {resource:?} refusal before native allocation: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(boundary.before > 0, "{boundary:?}");
    assert_eq!(native_bindings(&db), before);
    assert_guard_drained();
    lifetime_observations::reset();
    let retry =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded());
    lifetime_observations::stop();
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, bindings })) = retry else {
        panic!("funded native-binding retry: {retry:?}");
    };
    assert!(native_bindings(&db) > before);
    assert_guard_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = class.bindings_impl(&db, &env, &CallableRecursionGuard::new());
    assert_eq!(format!("{bindings:#?}"), format!("{ordinary:#?}"));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Work exhaustion refuses initial signature cloning before a binding receives the clone.
#[test]
fn initial_signature_work_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::InitialSignature, Resource::Work);
}

/// Byte exhaustion refuses initial signature cloning before a binding receives the clone.
#[test]
fn initial_signature_byte_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::InitialSignature, Resource::Bytes);
}

/// Work exhaustion refuses constructor wrapping before replacing the original callable items.
#[test]
fn constructor_wrap_work_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::ConstructorWrap, Resource::Work);
}

/// Byte exhaustion refuses constructor wrapping before replacing the original callable items.
#[test]
fn constructor_wrap_byte_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::ConstructorWrap, Resource::Bytes);
}

/// Work exhaustion refuses the downstream clone before its first owned storage is allocated.
#[test]
fn downstream_clone_work_refusal_precedes_mutation() {
    assert_resource_refusal(NEW_AND_INIT, Stage::DownstreamClone, Resource::Work);
}

/// Byte exhaustion refuses the downstream clone before its first owned storage is allocated.
#[test]
fn downstream_clone_byte_refusal_precedes_mutation() {
    assert_resource_refusal(NEW_AND_INIT, Stage::DownstreamClone, Resource::Bytes);
}

/// Work exhaustion refuses downstream installation before the cloned initializer is attached.
#[test]
fn downstream_install_work_refusal_precedes_mutation() {
    assert_resource_refusal(NEW_AND_INIT, Stage::DownstreamInstall, Resource::Work);
}

/// Byte exhaustion refuses downstream installation before the cloned initializer is attached.
#[test]
fn downstream_install_byte_refusal_precedes_mutation() {
    assert_resource_refusal(NEW_AND_INIT, Stage::DownstreamInstall, Resource::Bytes);
}

/// Work exhaustion refuses the final prepared-result transfer before the caller receives it.
#[test]
fn transfer_work_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::Transfer, Resource::Work);
}

/// Byte exhaustion refuses the final prepared-result transfer before the caller receives it.
#[test]
fn transfer_byte_refusal_precedes_mutation() {
    assert_resource_refusal(DECLARED_INIT, Stage::Transfer, Resource::Bytes);
}

/// Reads the same canonical new-member query used by constructor preparation.
#[derive(Clone, Copy, Debug)]
struct NewMemberRequest<'db>(Type<'db>);

impl<'db> MemberOperation<'db> for NewMemberRequest<'db> {
    type Output = Option<PlaceAndQualifiers<'db>>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.constructor_new_member(program, self.0).await
    }
}

/// Cold preparation publishes the existing new-member memo, which direct and ordinary reads reuse.
#[test]
fn new_member_lookup_reuses_the_canonical_key_and_memo() {
    let db = database(EMPTY);
    let prepared = prepare(&db);
    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded())
    })
    .unwrap();
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, .. })) = cold.value else {
        panic!("cold constructor preparation: {:?}", cold.value);
    };
    let events = event_db.take_salsa_events();
    let event = find_will_execute_event_by_name(&db, "lookup_dunder_new_inner", None, &events)
        .expect("cold preparation did not execute the new-member query");
    let salsa::EventKind::WillExecute { database_key } = event.kind else {
        panic!("expected the new-member execution event");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            LookupDunderNewQuery::ingredient(&db),
            database_key.key_index(),
        )
        .is_ok(),
    );
    let first = cold
        .reads
        .iter()
        .find(|read| read.key == database_key)
        .unwrap();
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, NewMemberRequest(class), &funded())
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(
        warm.value,
        Ok(AnalysisOutcome::Complete(Some(PlaceAndQualifiers::unbound())))
    );
    assert!(
        warm.reads
            .iter()
            .any(|read| { read.key == database_key && read.memo_address == first.memo_address })
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = capture(&db, || class.lookup_dunder_new(&db, &env)).unwrap();
    assert_eq!(ordinary.value, Some(PlaceAndQualifiers::unbound()));
    assert!(
        ordinary
            .reads
            .iter()
            .any(|read| { read.key == database_key && read.memo_address == first.memo_address })
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "lookup_dunder_new_inner",
        Some(database_key.key_index()),
        &event_db.take_salsa_events(),
    );
    assert_no_active_attempt();
}

/// Completed preparation and matching still refuse constructor checking and leave their caller unpublished.
/// The independently completed new-member query remains reusable after the enclosing expression stops.
#[test]
fn invocation_refusal_does_not_publish_a_partial_expression() {
    let db = database("class Product: pass\nProduct()\n");
    let prepared = prepare(&db);
    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(Stage::Transfer);
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let boundary = recording.boundary();
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::CheckerConstructor)),
    );
    assert!(boundary.after > 0, "{boundary:?}");
    let events = event_db.take_salsa_events();
    let expression_event =
        find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events)
            .expect("the enclosing expression did not execute");
    let salsa::EventKind::WillExecute { database_key } = expression_event.kind else {
        panic!("expected the enclosing expression execution event");
    };
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            database_key.key_index(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let new_event = find_will_execute_event_by_name(&db, "lookup_dunder_new_inner", None, &events)
        .expect("constructor preparation did not execute the new-member query");
    let salsa::EventKind::WillExecute { database_key } = new_event.kind else {
        panic!("expected the new-member execution event");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            LookupDunderNewQuery::ingredient(&db),
            database_key.key_index(),
        )
        .is_ok(),
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        result,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "lookup_dunder_new_inner",
        Some(database_key.key_index()),
        &event_db.take_salsa_events(),
    );
    assert_no_active_attempt();
}

/// A deferred initializer annotation suspends preparation after the new-method wrapper exists,
/// and resumption preserves the complete ordinary constructor tree. The polling wrapper observes
/// the production future's actual poll result without introducing suspension.
#[test]
fn real_child_suspension_preserves_prepared_constructor_state() {
    let db = deferred_database();
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(Stage::ConstructorWrap);
    recording.watch_child(
        definition_inference_ingredient(&db)
            .database_key_index(class_definition(&prepared, "Annotation").as_id()),
    );
    let result =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded());
    let boundary = recording.boundary();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, bindings })) = result else {
        panic!("suspended constructor preparation: {result:?}");
    };
    assert!(boundary.pending_after_mutation > 0, "{boundary:?}");
    assert!(boundary.mutations_before_child > 0, "{boundary:?}");
    assert!(boundary.child_scope.is_some(), "{boundary:?}");
    assert!(boundary.constructor_scope.is_some(), "{boundary:?}");
    assert!(!boundary.owner_retired_before_child, "{boundary:?}");
    assert_guard_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = class.bindings_impl(&db, &env, &CallableRecursionGuard::new());
    assert_eq!(format!("{bindings:#?}"), format!("{ordinary:#?}"));
}

/// Cancelling the initializer's real annotation child drains it before the prepared `__new__`
/// bindings release their backing storage, then closes the constructor's guard scope. A funded
/// request in the same revision prepares the complete ordinary constructor tree.
#[test]
fn cancelled_annotation_child_drains_before_guard_and_retries() {
    let db = deferred_database();
    let prepared = prepare(&db);
    let definition = class_definition(&prepared, "Annotation");
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    observations::cancel_definition_creation(definition.as_id());
    let recording = Recording::start(Stage::ConstructorWrap);
    recording
        .watch_child(definition_inference_ingredient(&db).database_key_index(definition.as_id()));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded())
    }));
    let boundary = recording.boundary();
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(boundary.after > 0, "{boundary:?}");
    assert!(boundary.pending_after_mutation > 0, "{boundary:?}");
    assert!(boundary.mutations_before_child > 0, "{boundary:?}");
    assert!(!boundary.owner_retired_before_child, "{boundary:?}");
    assert_guard_drained();
    let snapshot = lifetime_observations::snapshot();
    let events = &snapshot.events[..snapshot.count];
    let child = events
        .iter()
        .rposition(|event| {
            matches!(event, Some(lifetime_observations::Event::SourceChildDropped {
                definition: Some(actual),
            }) if *actual == definition.as_id())
        })
        .expect("the annotation child did not retire");
    let opening = boundary
        .child_scope
        .expect("the annotation child had no enclosing expansion scope");
    let scope = matching_scope_close(events, opening);
    assert!(child < scope, "{snapshot:?}");
    let constructor_opening = boundary
        .constructor_scope
        .expect("the new-method wrapper had no enclosing constructor scope");
    let constructor_scope = matching_scope_close(events, constructor_opening);
    let retirement = boundary
        .owner_retirement
        .expect("the prepared new-method bindings did not retire");
    let complete = retirement
        .complete
        .expect("the prepared new-method bindings did not finish releasing storage");
    assert!(child < retirement.begin, "{boundary:?}; {snapshot:?}");
    assert!(retirement.begin <= complete, "{boundary:?}");
    // Owner observations record how many native events preceded them. Equality means that
    // storage finished retiring immediately before the recorded scope-close event.
    //
    assert!(complete <= constructor_scope, "{boundary:?}; {snapshot:?}");

    let child_was_published = FinalSourceMemo::certify(
        &db as &dyn Db,
        definition_inference_ingredient(&db),
        definition.as_id(),
    )
    .is_ok();
    let mut event_db = db.clone();
    event_db.take_salsa_events();

    observations::reset(None);
    let recording = Recording::start(Stage::ConstructorWrap);
    let retry =
        controlled_member_operation(&prepared, request(&prepared, GuardEntry::Fresh), &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(PreparedConstructor { class, bindings })) = retry else {
        panic!("same-revision constructor retry: {retry:?}");
    };
    assert_guard_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = class.bindings_impl(&db, &env, &CallableRecursionGuard::new());
    assert_eq!(format!("{bindings:#?}"), format!("{ordinary:#?}"));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    // Salsa can defer cancellation until a complete fixpoint child has published.
    //
    // Such a child remains reusable; cancellation does not require removing its final memo.
    if child_was_published {
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &event_db.take_salsa_events(),
        );
    }
}

/// Finds one recorded scope's `ScopeDropBefore` event, accounting for nested expansion scopes.
fn matching_scope_close(events: &[Option<lifetime_observations::Event>], opening: usize) -> usize {
    let mut depth = 1;
    for (index, event) in events.iter().enumerate().skip(opening + 1) {
        match event {
            Some(lifetime_observations::Event::ScopeOpened(_)) => depth += 1,
            Some(lifetime_observations::Event::ScopeDropBefore(_)) if depth == 1 => return index,
            Some(lifetime_observations::Event::ScopeDropAfter(_)) => depth -= 1,
            Some(
                lifetime_observations::Event::ScopeDropBefore(_)
                | lifetime_observations::Event::StorageDropped { .. }
                | lifetime_observations::Event::RelationConversion { .. }
                | lifetime_observations::Event::RelationDropBefore { .. }
                | lifetime_observations::Event::RelationDropAfter { .. }
                | lifetime_observations::Event::SourceChildDropped { .. },
            )
            | None => {}
        }
    }
    panic!("the selected expansion scope did not retire: {events:?}");
}
