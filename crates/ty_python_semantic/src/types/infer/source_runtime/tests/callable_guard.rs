use std::cell::RefCell;
use std::convert::Infallible;
use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, RunResult, TaskEndpoint};

use super::*;
use crate::analysis::CallableGuardOperation;
use crate::types::call::invocation::{InvocationContext, InvocationEffects};
use crate::types::callable::{
    CallableConversionOperation, CallableType, CallableTypes, UpcastPolicy,
};
use crate::types::class::KnownClassInstanceEffects;
use crate::types::cyclic::entry::{
    CallableEntryDecision, CallableEntryFacts, CallableGuardEntryEffects,
    ExactCallableEntryDecision, callable_enter_exact_in_place_with, callable_enter_in_place_with,
};
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::cyclic::guard_storage::{CallableGuardStorageControl, CallableGuardStorageWork};
use crate::types::cyclic::{CallableExpansion, CallableRecursionGuard, CallableVisitScope};
use crate::types::subclass_of::{SubclassConstructionFacts, subclass_from_with};
use crate::types::{GenericAlias, KnownClass, Signature, SubclassOfInner};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    Dependency,
    Definition,
    Exact,
}

#[derive(Clone, Copy)]
enum Stop {
    None,
    Work(Stage),
    Allocation(Stage),
    Completion(Stage),
    Cancel(Stage),
    ChildCancellation(Stage),
    DefinitionChildCancellation(Stage, salsa::Id),
}

#[derive(Clone, Copy, Debug)]
struct State {
    guard: Option<lifetime_observations::GuardId>,
    definitions: Option<usize>,
    exact: Option<bool>,
    live_builders: usize,
}

#[derive(Clone, Debug, Default)]
struct Journal {
    commits: Vec<(Stage, State)>,
    retired: Vec<State>,
    child_cancellation_armed: bool,
    completion_marked: bool,
}

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static STOP: Cell<Stop> = const { Cell::new(Stop::None) };
    static JOURNAL: RefCell<Journal> = RefCell::new(Journal::default());
}

struct Recording;

impl Recording {
    fn start(stop: Stop) -> Self {
        assert!(!RECORDING.replace(true));
        lifetime_observations::reset();
        STOP.set(stop);
        JOURNAL.with_borrow_mut(|journal| *journal = Journal::default());
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
        lifetime_observations::stop();
        STOP.set(Stop::None);
    }
}

struct Inspection;

// Observations inspect existing state without performing entry or changing its admission ledger.
impl CallableGuardStorageControl for Inspection {
    type Error = Infallible;

    fn admit(&self, _work: CallableGuardStorageWork) -> Result<(), Infallible> {
        Ok(())
    }
}

fn state<'db>(
    guard: &CallableRecursionGuard<'db>,
    key: Option<(CallableExpansion, Type<'db>)>,
) -> State {
    State {
        guard: lifetime_observations::guard_state(guard).map(|state| state.id),
        definitions: guard
            .active_definition_uses_with(&Inspection)
            .ok()
            .map(|uses| uses.len()),
        exact: key.and_then(|key| guard.contains_exact_with(key, &Inspection).ok()),
        live_builders: observations::counts().0,
    }
}

pub(in crate::types::infer) fn committed<'db>(
    db: &'db dyn Db,
    endpoint: &TaskEndpoint<'_, 'db>,
    guard: &CallableRecursionGuard<'db>,
    stage: Stage,
    key: Option<(CallableExpansion, Type<'db>)>,
) -> RunResult<()> {
    if !RECORDING.get() {
        return Ok(());
    }
    JOURNAL.with_borrow_mut(|journal| journal.commits.push((stage, state(guard, key))));
    match STOP.get() {
        Stop::Work(target) if target == stage => {
            endpoint.admit_work(funded().semantic_work_limit)?
        }
        Stop::Allocation(target) if target == stage => {
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: funded().requested_bytes_limit,
            })?;
        }
        Stop::Cancel(target) if target == stage => {
            STOP.set(Stop::None);
            db.cancellation_token().cancel();
            db.unwind_if_revision_cancelled();
        }
        Stop::Completion(target) if target == stage => {
            report_incomplete(db, Incomplete::Allowance);
            JOURNAL.with_borrow_mut(|journal| journal.completion_marked = true);
            return Ok(());
        }
        Stop::ChildCancellation(target) if stage == target => {
            STOP.set(Stop::None);
            observations::reset(Some(observations::Event::Created));
            JOURNAL.with_borrow_mut(|journal| journal.child_cancellation_armed = true);
        }
        Stop::DefinitionChildCancellation(target, definition) if stage == target => {
            STOP.set(Stop::None);
            observations::reset(None);
            observations::cancel_definition_creation(definition);
            JOURNAL.with_borrow_mut(|journal| journal.child_cancellation_armed = true);
        }
        _ => {}
    }
    endpoint.check_completion()
}

struct GuardOwner<'db> {
    guard: CallableRecursionGuard<'db>,
    key: (CallableExpansion, Type<'db>),
}

impl Drop for GuardOwner<'_> {
    fn drop(&mut self) {
        if RECORDING.get() {
            JOURNAL.with_borrow_mut(|journal| {
                journal.retired.push(state(&self.guard, Some(self.key)));
            });
        }
    }
}

#[derive(Clone, Copy)]
enum Input<'db> {
    Known(KnownClass),
    KnownInstance(KnownClass),
    Definition(Definition<'db>),
    InstanceDefinition(Definition<'db>),
    SubclassDefinition(Definition<'db>),
    Type(Type<'db>),
}

#[derive(Clone, Copy)]
enum ConversionGuard {
    Omitted,
    Supplied,
    Exact,
    Untracked,
}

#[derive(Clone, Copy)]
enum Request<'db> {
    Prepare(Input<'db>),
    Enter(Input<'db>),
    LegacyEnter(Input<'db>),
    Exact(Input<'db>),
    Repeated(Type<'db>, Type<'db>),
    Untracked(Input<'db>),
    PropertyCallback(Definition<'db>),
    Convert(Input<'db>, UpcastPolicy, ConversionGuard),
    Relate(Input<'db>, CallableType<'db>),
}

#[derive(Debug)]
enum Output<'db> {
    Bindings(Bindings<'db>),
    Callables(Option<CallableTypes<'db>>),
    Relation(bool),
    Decision(ExactCallableEntryDecision),
    LegacyDecision(CallableEntryDecision),
    Property {
        getter: Type<'db>,
        result: Type<'db>,
    },
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    request: Request<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Output<'db>>, AnalysisFailure> {
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
        let descriptor_dispatches =
            register_descriptor_dispatches_values(session.db(), &mut registry)?;
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
            let input = match request {
                Request::Prepare(input)
                | Request::Enter(input)
                | Request::LegacyEnter(input)
                | Request::Exact(input)
                | Request::Untracked(input)
                | Request::Convert(input, _, _)
                | Request::Relate(input, _) => input,
                Request::Repeated(first, _) => Input::Type(first),
                Request::PropertyCallback(_) => Input::Known(KnownClass::Property),
            };
            let ty = match input {
                Input::Known(known) => {
                    KnownClassInstanceEffects::class_literal(&effects, known).await?
                }
                Input::KnownInstance(known) => {
                    access
                        .known_class_instance(session.program(), known)
                        .await?
                }
                Input::Type(ty) => ty,
                Input::Definition(definition)
                | Input::InstanceDefinition(definition)
                | Input::SubclassDefinition(definition) => {
                    let inference = access.definition(definition).await?;
                    access
                        .endpoint
                        .local_call(|| {
                            access.endpoint.admit_work(2)?;
                            access.endpoint.check_completion()?;
                            inference
                                .original_class_type(definition)
                                .map(Type::ClassLiteral)
                                .ok_or(RunError::Contract("fixture definition is not a class"))
                        })
                        .await
                }
            };
            let ty = if matches!(input, Input::InstanceDefinition(_)) {
                let class = KnownClassInstanceEffects::to_class_type(&effects, ty)
                    .await?
                    .ok_or(RunError::Contract("fixture class has no class type"))?;
                KnownClassInstanceEffects::instance(&effects, class).await?
            } else {
                ty
            };
            let ty = if matches!(input, Input::SubclassDefinition(_)) {
                let class = KnownClassInstanceEffects::to_class_type(&effects, ty)
                    .await?
                    .ok_or(RunError::Contract("fixture class has no class type"))?;
                subclass_from_with(
                    SubclassOfInner::Class(class),
                    SubclassConstructionFacts,
                    &effects,
                )
                .await?
            } else {
                ty
            };
            let env = ProgramEnvironment::from_file(prepared.program_file());
            if let Request::Relate(_, target) = request {
                return access
                    .is_redundant_with(ty, Type::Callable(target))
                    .await
                    .map(Output::Relation);
            }
            if let Request::Convert(_, policy, ConversionGuard::Omitted) = request {
                return effects
                    .callables_with_policy(&env, ty, policy, None)
                    .await
                    .map(Output::Callables);
            }
            let guard = if matches!(
                request,
                Request::Untracked(_) | Request::Convert(_, _, ConversionGuard::Untracked)
            ) {
                CallableRecursionGuard::new()
            } else {
                InvocationEffects::new_guard(&effects).await?
            };
            let owner = GuardOwner {
                guard,
                key: (
                    if matches!(request, Request::Convert(..)) {
                        CallableExpansion::Upcast
                    } else {
                        CallableExpansion::Bindings
                    },
                    ty,
                ),
            };
            let arguments = CallArguments::default();
            let context = InvocationContext {
                db: session.db(),
                env: &env,
                arguments: &arguments,
            };
            if let Request::Convert(_, policy, guard) = request
                && !matches!(guard, ConversionGuard::Exact)
            {
                return effects
                    .callables_with_policy(&env, ty, policy, Some(&owner.guard))
                    .await
                    .map(Output::Callables);
            }
            if matches!(request, Request::Untracked(_)) {
                let lifetime_count = lifetime_observations::snapshot().count;
                let result = effects
                    .synthetic_call(&env, ty, &arguments, Some(&owner.guard))
                    .await;
                assert_eq!(lifetime_events().len(), lifetime_count);
                let _result = result?;
                return Ok(Output::Decision(ExactCallableEntryDecision::Entered));
            }
            if let Request::PropertyCallback(definition) = request {
                let inference = access.definition(definition).await?;
                let getter = access
                    .endpoint
                    .local_call(|| {
                        access
                            .endpoint
                            .admit_work(1 + inference.binding_scan_len(definition))?;
                        access.endpoint.check_completion()?;
                        inference
                            .completed_binding(definition)
                            .ok_or(RunError::Contract(
                                "fixture function has no completed binding",
                            ))
                    })
                    .await;
                let bytes = CallArguments::capacity_bytes(1)
                    .ok_or(RunError::Contract("fixture argument storage overflow"))?;
                let mut arguments = None;
                access
                    .endpoint
                    .local_call(|| {
                        access.endpoint.admit_work(8)?;
                        access.endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: bytes,
                        })?;
                        access.endpoint.check_completion()?;
                        arguments = Some(CallArguments::positional([getter]));
                        Ok(())
                    })
                    .await;
                let arguments = arguments.ok_or(RunError::Contract("fixture arguments missing"))?;
                let called = effects
                    .synthetic_call(&env, ty, &arguments, Some(&owner.guard))
                    .await?;
                let bindings =
                    called.map_err(|_| RunError::Contract("property rejected fixture getter"))?;
                let result = bindings
                    .return_type_with(session.db(), &env, &effects)
                    .await?;
                return Ok(Output::Property { getter, result });
            }
            if matches!(request, Request::Prepare(_)) {
                return InvocationEffects::prepare(&effects, context, ty, &owner.guard)
                    .await
                    .map(Output::Bindings);
            }
            let mut scope = access
                .endpoint
                .local_call(|| {
                    access
                        .endpoint
                        .admit_work(size_of::<CallableVisitScope<'_, 'db>>() * 2 + 1)?;
                    access.endpoint.check_completion()?;
                    Ok(owner.guard.begin_scope())
                })
                .await;
            if let Request::LegacyEnter(_) = request {
                return callable_enter_in_place_with(
                    owner.key,
                    &mut scope,
                    CallableEntryFacts,
                    &effects,
                )
                .await
                .map(Output::LegacyDecision);
            }
            let first = callable_enter_exact_in_place_with(owner.key, &mut scope, &effects).await?;
            assert_eq!(first, ExactCallableEntryDecision::Entered);
            match request {
                Request::Enter(_) => Ok(Output::Decision(first)),
                Request::Exact(_) => {
                    let mut nested = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<CallableVisitScope<'_, 'db>>() * 2 + 1)?;
                            access.endpoint.check_completion()?;
                            Ok(owner.guard.begin_scope())
                        })
                        .await;
                    let repeated = callable_enter_exact_in_place_with(
                        owner.key,
                        &mut nested,
                        &effects,
                    )
                    .await?;
                    assert_eq!(repeated, ExactCallableEntryDecision::ExactCycle);
                    drop(nested);
                    assert!(
                        CallableGuardEntryEffects::contains_exact(&effects, &scope, owner.key)
                            .await?
                    );
                    InvocationEffects::prepare(&effects, context, ty, &owner.guard)
                        .await
                        .map(Output::Bindings)
                }
                Request::Repeated(_, second) => {
                    let mut nested = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<CallableVisitScope<'_, 'db>>() * 2 + 1)?;
                            access.endpoint.check_completion()?;
                            Ok(owner.guard.begin_scope())
                        })
                        .await;
                    callable_enter_exact_in_place_with(
                        (CallableExpansion::Bindings, second),
                        &mut nested,
                        &effects,
                    )
                    .await
                    .map(Output::Decision)
                }
                Request::Convert(_, policy, ConversionGuard::Exact) => effects
                    .callables_with_policy(&env, ty, policy, Some(&owner.guard))
                    .await
                    .map(Output::Callables),
                Request::Prepare(_)
                | Request::LegacyEnter(_)
                | Request::Untracked(_)
                | Request::PropertyCallback(_)
                | Request::Relate(..)
                | Request::Convert(..) => Err(RunError::Contract(
                    "preparation request reached entry driver",
                )),
            }
        })
    })
}

fn journal() -> Journal {
    JOURNAL.with_borrow(Clone::clone)
}

fn assert_cleanup() {
    let journal = journal();
    assert_eq!(journal.retired.len(), 1, "{journal:?}");
    let retired = journal.retired[0];
    assert_eq!(retired.definitions, Some(0), "{journal:?}");
    assert_eq!(retired.exact, Some(false), "{journal:?}");
    assert_eq!(retired.live_builders, 0, "{journal:?}");
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_conversion_finished(guard: ConversionGuard) {
    if matches!(guard, ConversionGuard::Omitted) {
        assert!(journal().retired.is_empty());
        assert_native_guard_retired();
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    } else {
        assert_cleanup();
    }
}

fn lifetime_events() -> Vec<lifetime_observations::Event> {
    let snapshot = lifetime_observations::snapshot();
    assert!(!snapshot.overflowed, "{snapshot:?}");
    assert_eq!(snapshot.active_scopes, 0, "{snapshot:?}");
    snapshot.events.into_iter().flatten().collect()
}

fn assert_native_guard_retired() -> (usize, usize) {
    let Some(id) = journal().commits.first().and_then(|(_, state)| state.guard) else {
        panic!("conversion did not enter an admitted guard");
    };
    let events = lifetime_events();
    let Some(opened) = events.iter().position(|event| {
        matches!(event,
            lifetime_observations::Event::ScopeOpened(state) if state.id == id
        )
    }) else {
        panic!("guard scope was not observed: {events:?}");
    };
    let Some(retired) = events.iter().position(|event| {
        matches!(event,
            lifetime_observations::Event::StorageDropped { id: actual, outstanding_removal_weights }
                if *actual == id && *outstanding_removal_weights == [0; 3]
        )
    }) else {
        panic!("guard storage did not retire with discharged cleanup receipts: {events:?}");
    };
    let Some(dropping) = events[..retired].iter().rposition(|event| {
        matches!(event,
            lifetime_observations::Event::ScopeDropBefore(state) if state.id == id
        )
    }) else {
        panic!("guard scope did not reach cleanup: {events:?}");
    };
    let Some(cleared) = events[..retired].iter().rposition(|event| {
        matches!(event,
            lifetime_observations::Event::ScopeDropAfter(state)
                if state.id == id && state.exact == 0 && state.identities == 0
                    && state.definitions == 0 && state.anchors == 0
        )
    }) else {
        panic!("guard scope retained active entries: {events:?}");
    };
    assert!(
        opened < dropping && dropping < cleared && cleared < retired,
        "{events:?}"
    );
    (opened, dropping)
}

fn assert_relation_retired(completed: bool) {
    let events = lifetime_events();
    let Some((conversion, visitor)) = events.iter().enumerate().find_map(|(index, event)| {
        if let lifetime_observations::Event::RelationConversion { visitor, counts } = event {
            assert_eq!(*counts, (1, 0), "{events:?}");
            Some((index, *visitor))
        } else {
            None
        }
    }) else {
        panic!("callable-source relation did not enter conversion: {events:?}");
    };
    let Some(relation_end) = events.iter().position(|event| {
        matches!(event,
            lifetime_observations::Event::RelationDropAfter { visitor: actual, counts, had_item }
                if *actual == visitor && *counts == (0, usize::from(completed))
                    && *had_item != completed
        )
    }) else {
        panic!("relation scope retained an active or partial result: {events:?}");
    };
    let Some(relation_dropping) = events.iter().position(|event| matches!(event,
        lifetime_observations::Event::RelationDropBefore { visitor: actual, counts, had_item }
            if *actual == visitor && *counts == (usize::from(!completed), usize::from(completed))
                && *had_item != completed
    )) else {
        panic!("relation scope did not retain its entry until completion or cleanup: {events:?}");
    };
    let Some(storage_end) = events
        .iter()
        .rposition(|event| matches!(event, lifetime_observations::Event::StorageDropped { .. }))
    else {
        panic!("callable guard storage did not retire: {events:?}");
    };
    assert!(
        conversion < storage_end
            && storage_end < relation_dropping
            && relation_dropping < relation_end,
        "{events:?}"
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn signature_strings<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    bindings: &Bindings<'db>,
) -> Vec<String> {
    bindings
        .iter_flat()
        .flat_map(IntoIterator::into_iter)
        .map(|binding| binding.signature.display(db, env).to_string())
        .collect()
}

fn conversion_fixture(call_member: bool) -> TestDb {
    let mut db = fixture();
    db.write_file(
        "src/main.py",
        if call_member {
            "class C:\n    def __call__(self): ...\n"
        } else {
            "class C: pass\n"
        },
    )
    .unwrap();
    db
}

fn conversion_input<'db>(prepared: &PreparedAnalysisFile<'db>, none: bool) -> Input<'db> {
    if none {
        Input::KnownInstance(KnownClass::NoneType)
    } else {
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        Input::InstanceDefinition(prepared.semantic_index().expect_single_definition(class))
    }
}

/// None and a custom instance without `__call__` have no callable signatures in controlled or
/// ordinary conversion. Omitted and admitted supplied guards permit either upcast policy.
#[test]
fn absent_call_member_conversion_preserves_ordinary_result() {
    for none in [true, false] {
        for policy in [UpcastPolicy::Sound, UpcastPolicy::Unsound] {
            for guard in [ConversionGuard::Omitted, ConversionGuard::Supplied] {
                let db = conversion_fixture(false);
                let prepared = prepare(&db);
                observations::reset(None);
                let recording = Recording::start(Stop::None);
                let result = controlled(
                    &prepared,
                    Request::Convert(conversion_input(&prepared, none), policy, guard),
                    &funded(),
                );
                drop(recording);
                let Ok(AnalysisOutcome::Complete(Output::Callables(actual))) = result else {
                    panic!("absent call member conversion: {result:?}");
                };
                assert_conversion_finished(guard);
                assert_eq!(
                    journal()
                        .commits
                        .iter()
                        .map(|(stage, _)| *stage)
                        .collect::<Vec<_>>(),
                    [Stage::Dependency, Stage::Exact]
                );

                let ordinary_db = conversion_fixture(false);
                let ordinary_prepared = prepare(&ordinary_db);
                let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
                let ordinary_type = match conversion_input(&ordinary_prepared, none) {
                    Input::KnownInstance(known) => known.to_instance(&ordinary_db, &ordinary_env),
                    Input::InstanceDefinition(definition) => {
                        let Some(class) = infer_definition_types(&ordinary_db, definition)
                            .original_class_type(definition)
                        else {
                            panic!("fixture definition is not a class");
                        };
                        Type::instance(&ordinary_db, &ordinary_env, ClassType::NonGeneric(class))
                    }
                    _ => panic!("fixture instance input"),
                };
                let ordinary = ordinary_type.try_upcast_to_callable_with_policy(
                    &ordinary_db,
                    &ordinary_env,
                    policy,
                );
                assert_eq!(ordinary, None);
                assert_eq!(actual, None);
            }
        }
    }
}

/// A native function-valued `__call__` binds with the supplied or conversion-owned guard.
/// Conversion then refuses at its unsupported continuation; the enclosing attempt retires its guard.
#[test]
fn present_call_member_conversion_binds_before_continuation_refusal() {
    for guard in [ConversionGuard::Omitted, ConversionGuard::Supplied] {
        let db = conversion_fixture(true);
        let prepared = prepare(&db);
        observations::reset(None);
        let recording = Recording::start(Stop::None);
        let result = controlled(
            &prepared,
            Request::Convert(
                conversion_input(&prepared, false),
                UpcastPolicy::default(),
                guard,
            ),
            &funded(),
        );
        drop(recording);
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(OperationId::CallableConversion(
                        CallableConversionOperation::Continuation
                    )),
                    completed: (),
                })
            ),
            "{result:?}"
        );
        assert_eq!(
            journal().commits.last().map(|(stage, _)| *stage),
            Some(Stage::Exact)
        );
        assert_conversion_finished(guard);
    }
}

/// Conversion reuses the supplied guard: an active upcast of the same type refuses at the cycle
/// boundary. A guard created outside admitted storage refuses before entry; replacing it with a
/// fresh admitted guard would instead return no callables.
#[test]
fn conversion_preserves_supplied_guard_identity_and_provenance() {
    for guard in [ConversionGuard::Exact, ConversionGuard::Untracked] {
        let db = fixture();
        let prepared = prepare(&db);
        observations::reset(None);
        let recording = Recording::start(Stop::None);
        let result = controlled(
            &prepared,
            Request::Convert(
                Input::KnownInstance(KnownClass::NoneType),
                UpcastPolicy::default(),
                guard,
            ),
            &funded(),
        );
        drop(recording);
        let operation = match guard {
            ConversionGuard::Exact => {
                let journal = journal();
                assert_eq!(
                    journal
                        .commits
                        .iter()
                        .map(|(stage, _)| *stage)
                        .collect::<Vec<_>>(),
                    [
                        Stage::Dependency,
                        Stage::Exact,
                        Stage::Dependency
                    ]
                );
                assert_eq!(journal.commits[1].1.exact, Some(true));
                assert_eq!(journal.commits[2].1.exact, Some(true));
                assert_eq!(journal.commits[2].1.definitions, Some(0));
                assert_cleanup();
                OperationId::CallableConversion(CallableConversionOperation::GuardCycle)
            }
            ConversionGuard::Untracked => {
                assert!(journal().commits.is_empty());
                assert_eq!(journal().retired.len(), 1);
                assert_eq!(observations::counts().0, 0);
                assert_no_active_attempt();
                OperationId::CallableGuard(CallableGuardOperation::StorageOrigin)
            }
            _ => panic!("fixture supplied guard"),
        };
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(actual),
                    completed: (),
                }) if actual == operation
            ),
            "{result:?}"
        );
    }
}

/// Cold property and object preparation use exact guard entry and preserve ordinary signatures.
/// Neither preparation populates the definition table used for growth approximation.
#[test]
fn exact_entry_known_classes_preserve_ordinary_bindings() {
    for known in [KnownClass::Property, KnownClass::Object] {
        let db = fixture();
        let prepared = prepare(&db);
        observations::reset(None);
        let recording = Recording::start(Stop::None);
        let result = controlled(&prepared, Request::Prepare(Input::Known(known)), &funded());
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Output::Bindings(bindings))) = result else {
            panic!("cold known-class preparation: {result:?}");
        };
        assert_cleanup();
        let journal = journal();
        let root_guard = journal.retired[0].guard;
        assert!(root_guard.is_some());
        let root_commits = journal
            .commits
            .iter()
            .filter(|(_, state)| state.guard == root_guard)
            .collect::<Vec<_>>();
        assert_eq!(
            root_commits
                .iter()
                .map(|(stage, _)| *stage)
                .collect::<Vec<_>>(),
            [Stage::Dependency, Stage::Exact]
        );
        assert_eq!(root_commits[1].1.definitions, Some(0));
        assert_eq!(root_commits[1].1.exact, Some(true));

        let ordinary_db = fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        let ordinary_type = known.to_class_literal(&ordinary_db, &ordinary_env);
        let ordinary = ordinary_type.bindings_impl(
            &ordinary_db,
            &ordinary_env,
            &CallableRecursionGuard::new(),
        );
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(
            signature_strings(&db, &env, &bindings),
            signature_strings(&ordinary_db, &ordinary_env, &ordinary)
        );
    }
}

/// Synthetic property construction checks a source-defined getter and retains that exact function.
/// Parameter matching and checking leave the omitted setter and deleter absent from the property.
#[test]
fn cold_synthetic_property_call_checks_and_retains_getter() {
    let mut db = fixture();
    db.write_file("src/main.py", "def getter(instance): ...\n")
        .unwrap();
    let prepared = prepare(&db);
    let Stmt::FunctionDef(getter) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture function")
    };
    let getter_definition = prepared.semantic_index().expect_single_definition(getter);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    invocation_observations::reset_invocations();
    let recording = Recording::start(Stop::None);
    let result = controlled(
        &prepared,
        Request::PropertyCallback(getter_definition),
        &funded(),
    );
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Output::Property { getter, result })) = result else {
        panic!("cold synthetic property invocation: {result:?}");
    };
    let Type::PropertyInstance(property) = result else {
        panic!("property invocation returned {result:?}");
    };
    assert!(matches!(getter, Type::FunctionLiteral(_)));
    assert_eq!(property.getter(&db), Some(getter));
    assert_eq!(property.setter(&db), None);
    assert_eq!(property.deleter(&db), None);
    let invocation = invocation_observations::invocation_snapshot();
    assert!(invocation.events[..invocation.count].iter().any(|event| {
        event.is_some_and(|event| {
            event.stage == invocation_observations::InvocationStage::BinderCheck
        })
    }));
    assert_cleanup();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// A whole-file property check retires its callable guard when decorator preparation is interrupted.
/// Work refusal leaves the getter definition unpublished; cancellation permits reuse only if that
/// definition completed. A funded retry preserves the canonical getter in the same revision.
#[test]
fn interrupted_property_file_check_retires_guard_and_retries() {
    let source = "class Example:\n    @property\n    def ready(self):\n        return True\n";
    for stop in [Stop::Work(Stage::Exact), Stop::Cancel(Stage::Exact)] {
        let mut db = fixture();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let Stmt::FunctionDef(function) = &class.body[0] else {
            panic!("fixture property getter");
        };
        let definition = prepared.semantic_index().expect_single_definition(function);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let recording = Recording::start(stop);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            check_file_with_policy(&prepared, &funded())
        }));
        drop(recording);
        match stop {
            Stop::Work(_) => assert!(
                matches!(
                    result,
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    }))
                ),
                "{result:?}",
            ),
            Stop::Cancel(_) => {
                assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            }
            _ => panic!("fixture interruption"),
        }
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events_db.take_salsa_events(),
            )
            .is_some()
        );
        let journal = journal();
        let Some(guard) = journal.commits.first().and_then(|(_, state)| state.guard) else {
            panic!("decorator preparation must enter an admitted guard: {journal:?}");
        };
        let Some((_, state)) = journal
            .commits
            .iter()
            .find(|(stage, state)| *stage == Stage::Exact && state.guard == Some(guard))
        else {
            panic!("decorator preparation must commit its exact entry: {journal:?}");
        };
        assert_eq!(state.definitions, Some(0));
        assert_eq!(state.exact, Some(true));
        assert!(state.live_builders > 0);
        assert_native_guard_retired();
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(()),
        );
        let published = FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ());
        if matches!(stop, Stop::Work(_)) {
            assert_eq!(published, Err(FinalSourceError::MissingMemo));
        }
        let definition_completed = published.is_ok();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        observations::reset(None);
        let retry = check_file_with_policy(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = retry else {
            panic!("property file retry: {retry:?}");
        };
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events_db.take_salsa_events(),
            )
            .is_none(),
            definition_completed,
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
        let inference = infer_definition_types(&db, definition);
        let Type::PropertyInstance(property) = inference.binding_type(definition) else {
            panic!("completed getter definition must bind a property");
        };
        let Some(getter @ Type::FunctionLiteral(_)) = inference.undecorated_type() else {
            panic!("completed getter definition must retain its undecorated function");
        };
        assert_eq!(property.getter(&db), Some(getter));
        assert_eq!(property.setter(&db), None);
        assert_eq!(property.deleter(&db), None);

        let mut ordinary_db = fixture();
        ordinary_db.write_file("src/main.py", source).unwrap();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary = crate::check_file(&ordinary_db, ordinary_prepared.program_file()).unwrap();
        assert_eq!(diagnostics, ordinary);
        assert_eq!(
            check_file_with_policy(&prepared, &funded()),
            Ok(AnalysisOutcome::Complete(Ok(diagnostics))),
        );
        assert!(std::ptr::eq(
            infer_definition_types(&db, definition),
            inference
        ));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn invocation_reached_binder() -> bool {
    invocation_observations::invocation_snapshot()
        .events
        .iter()
        .flatten()
        .any(|event| event.stage == invocation_observations::InvocationStage::BinderCheck)
}

/// A normal file check builds a reusable property through argument checking and retains its getter.
/// Interrupting callable preparation retires the guard and leaves the assignment retryable in the
/// same revision. Work refusal leaves the assignment unpublished; cancellation permits reuse only
/// if the assignment completed.
#[test]
fn interrupted_reusable_property_file_check_retires_guard_and_retries() {
    let source = "def getter(instance):\n    return True\nready = property(getter)\n";
    for stop in [
        Stop::None,
        Stop::Work(Stage::Exact),
        Stop::Cancel(Stage::Exact),
    ] {
        let mut db = fixture();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let [Stmt::FunctionDef(function), Stmt::Assign(assignment)] =
            prepared.parsed_module().syntax().body.as_slice()
        else {
            panic!("fixture defines a getter and assigns its property");
        };
        let getter_definition = prepared.semantic_index().expect_single_definition(function);
        let definition = assignment_definition(&prepared, assignment);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        invocation_observations::reset_invocations();
        let recording = Recording::start(stop);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            check_file_with_policy(&prepared, &funded())
        }));
        drop(recording);
        let initially_checked = invocation_reached_binder();
        match stop {
            Stop::None => {
                assert!(
                    matches!(
                        &result,
                        Ok(Ok(AnalysisOutcome::Complete(Ok(diagnostics)))) if diagnostics.is_empty()
                    ),
                    "{result:?}"
                );
                assert!(initially_checked);
            }
            Stop::Work(_) => assert!(
                matches!(
                    result,
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    }))
                ),
                "{result:?}",
            ),
            Stop::Cancel(_) => {
                assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            }
            _ => panic!("fixture interruption"),
        }
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events_db.take_salsa_events(),
            )
            .is_some()
        );
        let journal = journal();
        let Some(guard) = journal.commits.first().and_then(|(_, state)| state.guard) else {
            panic!("property call preparation must enter an admitted guard: {journal:?}");
        };
        let Some((_, state)) = journal
            .commits
            .iter()
            .find(|(stage, state)| *stage == Stage::Exact && state.guard == Some(guard))
        else {
            panic!("property call preparation must commit its exact entry: {journal:?}");
        };
        assert_eq!(state.definitions, Some(0));
        assert_eq!(state.exact, Some(true));
        assert!(state.live_builders > 0);
        assert_native_guard_retired();
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(()),
        );
        let published = FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ());
        if matches!(stop, Stop::Work(_)) {
            assert_eq!(published, Err(FinalSourceError::MissingMemo));
        }
        let assignment_completed = published.is_ok();
        if matches!(stop, Stop::None) {
            assert!(assignment_completed);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        observations::reset(None);
        invocation_observations::reset_invocations();
        let retry = check_file_with_policy(&prepared, &funded());
        let retry_checked = invocation_reached_binder();
        let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = retry else {
            panic!("reusable property file retry: {retry:?}");
        };
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(initially_checked || retry_checked);
        assert_eq!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events_db.take_salsa_events(),
            )
            .is_none(),
            assignment_completed,
        );
        for completed in [definition, getter_definition] {
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    completed.as_id(),
                )
                .is_ok()
            );
        }
        let inference = infer_definition_types(&db, definition);
        let Type::PropertyInstance(property) = inference.binding_type(definition) else {
            panic!("completed assignment must bind a property");
        };
        let getter = infer_definition_types(&db, getter_definition).binding_type(getter_definition);
        assert!(matches!(getter, Type::FunctionLiteral(_)));
        assert_eq!(property.getter(&db), Some(getter));
        assert_eq!(property.setter(&db), None);
        assert_eq!(property.deleter(&db), None);

        let mut ordinary_db = fixture();
        ordinary_db.write_file("src/main.py", source).unwrap();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary = crate::check_file(&ordinary_db, ordinary_prepared.program_file()).unwrap();
        assert_eq!(diagnostics, ordinary);
        assert_eq!(
            check_file_with_policy(&prepared, &funded()),
            Ok(AnalysisOutcome::Complete(Ok(diagnostics))),
        );
        assert!(std::ptr::eq(
            infer_definition_types(&db, definition),
            inference
        ));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(()),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

/// An exact cycle keeps the ancestor's entry and uses an unknown, non-recovery signature.
/// Both entry and repetition leave the growth-approximation definition table empty.
#[test]
fn exact_cycle_preserves_ancestor_and_unknown_signature() {
    let db = fixture();
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(Stop::None);
    let result = controlled(
        &prepared,
        Request::Exact(Input::Known(KnownClass::Property)),
        &funded(),
    );
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Output::Bindings(bindings))) = result else {
        panic!("exact cycle: {result:?}");
    };
    let signatures = bindings
        .iter_flat()
        .flat_map(IntoIterator::into_iter)
        .map(|binding| &binding.signature)
        .collect::<Vec<_>>();
    assert_eq!(signatures, [&Signature::unknown()]);
    assert!(!signatures[0].is_recursion_recovery());
    assert_eq!(
        journal()
            .commits
            .iter()
            .filter(|(stage, _)| *stage == Stage::Definition)
            .count(),
        0
    );
    assert_cleanup();
}

/// Distinct specializations enter together without consulting growth metadata. Each scope removes
/// its own exact entry, so a later entry completes in the same revision.
#[test]
fn distinct_specializations_enter_and_retry_after_scope_cleanup() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let Type::ClassLiteral(ClassLiteral::Static(origin)) =
        KnownClass::List.to_class_literal(&db, &env)
    else {
        panic!("list is not a static class");
    };
    let context = origin.generic_context(&db).unwrap();
    let aliases = [Type::int_literal(1), Type::bool_literal(true)].map(|argument| {
        Type::GenericAlias(GenericAlias::new(
            &db,
            origin,
            context.specialize(&db, &[argument]),
        ))
    });
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(Stop::None);
    let result = controlled(
        &prepared,
        Request::Repeated(aliases[0], aliases[1]),
        &funded(),
    );
    drop(recording);
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(Output::Decision(
                ExactCallableEntryDecision::Entered
            )))
        ),
        "{result:?}"
    );
    assert_eq!(
        journal()
            .commits
            .iter()
            .filter(|(stage, _)| *stage == Stage::Definition)
            .count(),
        0
    );
    assert_cleanup();
    let recording = Recording::start(Stop::None);
    let retry = controlled(
        &prepared,
        Request::Enter(Input::Type(aliases[1])),
        &funded(),
    );
    drop(recording);
    assert!(
        matches!(
            retry,
            Ok(AnalysisOutcome::Complete(Output::Decision(
                ExactCallableEntryDecision::Entered
            )))
        ),
        "{retry:?}"
    );
    assert_cleanup();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// A synthetic invocation refuses an untracked supplied guard without creating an admitted guard.
/// Preparation uses the supplied guard, whose storage history cannot be reconstructed safely.
#[test]
fn untracked_supplied_guard_refuses_before_storage_mutation() {
    let db = fixture();
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(Stop::None);
    let result = controlled(
        &prepared,
        Request::Untracked(Input::Known(KnownClass::Property)),
        &funded(),
    );
    drop(recording);
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::CallableGuard(
                    CallableGuardOperation::StorageOrigin
                )),
                completed: (),
            })
        ),
        "{result:?}"
    );
    assert!(journal().commits.is_empty());
    assert_eq!(journal().retired.len(), 1);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Entry and conversion retain each commit's ownership through refusal, rejected completion, and
/// cancellation until cleanup removes the committed guard state. A funded retry starts a fresh
/// entry for the same type in the same database revision.
#[test]
fn interruption_after_guard_commits_cleans_up_and_retries() {
    for stage in [Stage::Dependency, Stage::Exact] {
        for stop in [
            Stop::Work(stage),
            Stop::Allocation(stage),
            Stop::Completion(stage),
            Stop::Cancel(stage),
        ] {
            for request in [
                Request::Enter(Input::Known(KnownClass::Property)),
                Request::Convert(
                    Input::KnownInstance(KnownClass::NoneType),
                    UpcastPolicy::default(),
                    ConversionGuard::Supplied,
                ),
                Request::Convert(
                    Input::KnownInstance(KnownClass::NoneType),
                    UpcastPolicy::default(),
                    ConversionGuard::Omitted,
                ),
            ] {
                let db = fixture();
                let prepared = prepare(&db);
                let revision = salsa::plumbing::current_revision(&db);
                observations::reset(None);
                let recording = Recording::start(stop);
                let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    controlled(&prepared, request, &funded())
                }));
                drop(recording);
                match stop {
                    Stop::Work(_) | Stop::Completion(_) => assert!(
                        matches!(
                            result,
                            Ok(Ok(AnalysisOutcome::Incomplete {
                                reason: AnalysisIncomplete::WorkLimit,
                                completed: ()
                            }))
                        ),
                        "{result:?}"
                    ),
                    Stop::Allocation(_) => assert!(
                        matches!(
                            result,
                            Ok(Ok(AnalysisOutcome::Incomplete {
                                reason: AnalysisIncomplete::RequestedAllocationLimit,
                                completed: ()
                            }))
                        ),
                        "{result:?}"
                    ),
                    Stop::Cancel(_) => {
                        assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}")
                    }
                    Stop::None
                    | Stop::ChildCancellation(_)
                    | Stop::DefinitionChildCancellation(..) => panic!("fixture interruption"),
                }
                let journal = journal();
                assert_eq!(
                    journal.completion_marked,
                    matches!(stop, Stop::Completion(_))
                );
                assert_eq!(journal.commits.last().map(|(stage, _)| *stage), Some(stage));
                assert_eq!(journal.commits.last().unwrap().1.definitions, Some(0));
                if stage == Stage::Exact {
                    assert_eq!(journal.commits.last().unwrap().1.exact, Some(true));
                }
                if let Request::Convert(_, _, guard) = request {
                    assert_conversion_finished(guard);
                } else {
                    assert_cleanup();
                }
                let recording = Recording::start(Stop::None);
                let retry = controlled(&prepared, request, &funded());
                drop(recording);
                assert!(
                    matches!(
                        retry,
                        Ok(AnalysisOutcome::Complete(
                            Output::Decision(ExactCallableEntryDecision::Entered)
                                | Output::Callables(None)
                        ))
                    ),
                    "{retry:?}"
                );
                if let Request::Convert(_, _, guard) = request {
                    assert_conversion_finished(guard);
                } else {
                    assert_cleanup();
                }
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
            }
        }
    }
}

/// Callable-source comparison keeps its relation scope active while an independently owned
/// conversion guard is live. Interruption retires both without caching a relation result; a retry
/// in the same revision completes the absent-`__call__` comparison and publishes its canonical memo.
#[test]
fn callable_source_scopes_retire_after_interruption_and_retry() {
    for stop in [
        Stop::Work(Stage::Exact),
        Stop::Allocation(Stage::Exact),
        Stop::Completion(Stage::Exact),
        Stop::Cancel(Stage::Exact),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let target = CallableType::single(&db, Signature::unknown());
        let request = Request::Relate(Input::KnownInstance(KnownClass::NoneType), target);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let recording = Recording::start(stop);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, request, &funded())
        }));
        drop(recording);
        match stop {
            Stop::Work(_) | Stop::Completion(_) => assert!(
                matches!(
                    result,
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: ()
                    }))
                ),
                "{result:?}"
            ),
            Stop::Allocation(_) => assert!(
                matches!(
                    result,
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        completed: ()
                    }))
                ),
                "{result:?}"
            ),
            Stop::Cancel(_) => {
                assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}")
            }
            _ => panic!("fixture interruption"),
        }
        assert_eq!(
            journal()
                .commits
                .last()
                .map(|(stage, state)| (*stage, state.exact)),
            Some((Stage::Exact, Some(true)))
        );
        assert!(journal().retired.is_empty());
        assert_native_guard_retired();
        assert_relation_retired(false);
        assert!(
            find_will_execute_event_by_name(
                &db,
                "is_redundant_with_impl",
                None,
                &events_db.take_salsa_events()
            )
            .is_some()
        );

        observations::reset(None);
        let recording = Recording::start(Stop::None);
        let retry = controlled(&prepared, request, &funded());
        drop(recording);
        assert!(
            matches!(
                retry,
                Ok(AnalysisOutcome::Complete(Output::Relation(false)))
            ),
            "{retry:?}"
        );
        assert_native_guard_retired();
        assert_relation_retired(true);
        assert!(
            find_will_execute_event_by_name(
                &db,
                "is_redundant_with_impl",
                None,
                &events_db.take_salsa_events()
            )
            .is_some()
        );

        let recording = Recording::start(Stop::None);
        let cached = controlled(&prepared, request, &funded());
        drop(recording);
        assert!(
            matches!(
                cached,
                Ok(AnalysisOutcome::Complete(Output::Relation(false)))
            ),
            "{cached:?}"
        );
        assert!(journal().commits.is_empty());
        assert!(lifetime_events().is_empty());
        assert_function_query_was_not_run_by_name(
            &db,
            "is_redundant_with_impl",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

/// Legacy entry's definition metadata requests a canonical class-context child. Cancellation
/// drains that child before dropping the enclosing guard owner.
/// The dependency insertion precedes the child, and retry completes without changing the revision.
#[test]
fn cancelled_class_context_child_drains_before_guard_owner() {
    let db = forward_class_base_fixture();
    let file = system_path_to_file(&db, "src/main.pyi").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(Stop::ChildCancellation(Stage::Dependency));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(
            &prepared,
            Request::LegacyEnter(Input::Definition(definition)),
            &funded(),
        )
    }));
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(journal().child_cancellation_armed);
    assert!(observations::counts().1 > 0);
    assert_cleanup();
    observations::reset(None);
    let recording = Recording::start(Stop::None);
    let retry = controlled(
        &prepared,
        Request::LegacyEnter(Input::Definition(definition)),
        &funded(),
    );
    drop(recording);
    assert!(
        matches!(
            retry,
            Ok(AnalysisOutcome::Complete(Output::LegacyDecision(
                CallableEntryDecision::Entered
            )))
        ),
        "{retry:?}"
    );
    assert_cleanup();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Cancellation during `__call__` definition inference drains that canonical child before the
/// conversion's supplied or independently owned guard retires. When callable-source comparison
/// initiates conversion, its relation scope also stays active through child cleanup.
/// A same-revision retry reuses a completed child or reruns an
/// interrupted child before binding the native function descriptor and reaching the unsupported
/// conversion continuation. Another attempt reuses the completed child despite that refusal.
#[test]
fn cancelled_call_member_child_drains_before_guard_owner() {
    for (guard, relation) in [
        (ConversionGuard::Supplied, false),
        (ConversionGuard::Omitted, false),
        (ConversionGuard::Omitted, true),
    ] {
        let db = conversion_fixture(true);
        let prepared = prepare(&db);
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let Stmt::FunctionDef(call) = &class.body[0] else {
            panic!("fixture call method");
        };
        let definition = prepared.semantic_index().expect_single_definition(call);
        let request = if relation {
            Request::Relate(
                conversion_input(&prepared, false),
                CallableType::single(&db, Signature::unknown()),
            )
        } else {
            Request::Convert(
                conversion_input(&prepared, false),
                UpcastPolicy::default(),
                guard,
            )
        };
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let recording = Recording::start(Stop::DefinitionChildCancellation(
            Stage::Exact,
            definition.as_id(),
        ));
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, request, &funded())
        }));
        drop(recording);
        assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        assert!(journal().child_cancellation_armed);
        assert!(observations::counts().1 > 0);
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events,
            )
            .is_some()
        );
        let (opened, dropping) = assert_native_guard_retired();
        let events = lifetime_events();
        let Some(child_dropped) = events.iter().position(|event| {
            matches!(event,
                lifetime_observations::Event::SourceChildDropped { definition: Some(actual) }
                    if *actual == definition.as_id()
            )
        }) else {
            panic!("cancelled definition child did not drain: {events:?}");
        };
        assert!(
            opened < child_dropped && child_dropped < dropping,
            "{events:?}"
        );
        assert!(
            matches!(events[dropping], lifetime_observations::Event::ScopeDropBefore(state)
            if state.exact == 1 && state.definitions == 0),
            "{events:?}"
        );
        assert_conversion_finished(guard);
        if relation {
            assert_relation_retired(false);
        }

        // `infer_definition_types` can publish its fixpoint result while Local cancellation is
        // masked. Only a certified final definition may be reused; an interrupted definition must
        // execute again.
        let child_completed = FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok();
        for child_is_cached in [child_completed, true] {
            observations::reset(None);
            let recording = Recording::start(Stop::None);
            let retry = controlled(&prepared, request, &funded());
            drop(recording);
            assert!(
                matches!(
                    retry,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::UnavailableOperation(OperationId::CallableConversion(
                            CallableConversionOperation::Continuation
                        )),
                        completed: (),
                    })
                ),
                "{retry:?}"
            );
            let events = events_db.take_salsa_events();
            assert_eq!(
                find_will_execute_event_by_name(
                    &db,
                    "infer_definition_types",
                    Some(definition.as_id()),
                    &events
                )
                .is_none(),
                child_is_cached,
                "{events:?}",
            );
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    definition.as_id(),
                )
                .is_ok()
            );
            assert_conversion_finished(guard);
            if relation {
                assert_relation_retired(false);
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

/// Observes conversion after top parameters exist and before their instance child starts.
pub(in crate::types::infer) fn subclass_instance_boundary(
    db: &dyn Db,
    endpoint: &TaskEndpoint<'_, '_>,
) -> RunResult<()> {
    subclass::instance_boundary(db);
    endpoint.check_completion()
}

mod subclass {
    //! Exercise subclass callable identity and retained ownership in the real conversion harness.

    use ruff_python_ast::name::Name;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::types::callable::CallableTypeKind;
    use crate::types::{BoundTypeVarInstance, Parameters, SubclassOfType, TypeVarVariance};

    #[derive(Clone, Copy, Debug, Default)]
    struct Boundary {
        reached: bool,
        remaining: Option<usize>,
        active_scopes: usize,
        cancel: bool,
    }

    thread_local! {
        static BOUNDARY: Cell<Boundary> = const { Cell::new(Boundary {
            reached: false, remaining: None, active_scopes: 0, cancel: false,
        }) };
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Action {
        Record,
        Cancel,
    }

    /// Resets observation of the transition that retains top parameters across the instance child.
    fn observe(action: Action) {
        BOUNDARY.set(Boundary {
            cancel: action == Action::Cancel,
            ..Boundary::default()
        });
    }

    /// Records the retained-parameter boundary and optionally cancels before the child starts.
    pub(super) fn instance_boundary(db: &dyn Db) {
        if !RECORDING.get() {
            return;
        }
        let boundary = Boundary {
            reached: true,
            remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            active_scopes: lifetime_observations::snapshot().active_scopes,
            cancel: false,
        };
        let previous = BOUNDARY.replace(boundary);
        if previous.cancel {
            db.cancellation_token().cancel();
            db.unwind_if_revision_cancelled();
        }
    }

    /// Checks the completed signature and its canonical FunctionLike interner identity.
    fn completed<'db>(
        db: &'db TestDb,
        result: Result<AnalysisOutcome<Output<'db>>, AnalysisFailure>,
        expected: Type<'db>,
    ) -> CallableType<'db> {
        let Ok(AnalysisOutcome::Complete(Output::Callables(Some(callables)))) = result else {
            panic!("sound subclass conversion: {result:?}");
        };
        let Some(callable) = callables.exactly_one() else {
            panic!("subclass conversion must return one callable");
        };
        assert_eq!(callable.kind(db), CallableTypeKind::FunctionLike);
        let mut signatures = callable.signatures(db).iter();
        let Some(signature) = signatures.next() else {
            panic!("subclass callable must have a signature");
        };
        assert!(signatures.next().is_none());
        assert!(signature.parameters().is_top());
        assert!(!signature.parameters().is_gradual());
        assert_eq!(signature.return_ty, expected);
        assert_eq!(
            callable,
            CallableType::function_like(db, Signature::new(Parameters::top(), expected))
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        callable
    }

    /// A class resolved by the controlled definition producer yields the same nominal instance
    /// and canonical FunctionLike callable as ordinary conversion, retaining its supplied guard.
    #[test]
    fn subclass_callable_cold_class_preserves_instance_and_guard() {
        let db = conversion_fixture(false);
        let prepared = prepare(&db);
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let definition = prepared.semantic_index().expect_single_definition(class);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id()
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        observations::reset(None);
        observe(Action::Record);
        let recording = Recording::start(Stop::None);
        let result = controlled(
            &prepared,
            Request::Convert(
                Input::SubclassDefinition(definition),
                UpcastPolicy::Sound,
                ConversionGuard::Supplied,
            ),
            &funded(),
        );
        drop(recording);
        assert!(BOUNDARY.get().reached, "{result:?}");
        assert!(BOUNDARY.get().active_scopes > 0);
        assert_cleanup();
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let Some(class) = infer_definition_types(&db, definition).original_class_type(definition)
        else {
            panic!("fixture class definition");
        };
        let expected = Type::instance(&db, &env, ClassType::NonGeneric(class));
        completed(&db, result, expected);
        assert_eq!(
            journal()
                .commits
                .iter()
                .map(|(stage, _)| *stage)
                .collect::<Vec<_>>(),
            [Stage::Dependency, Stage::Exact]
        );
    }

    /// Prepared generic, dynamic, protocol, and TypeVar inputs keep their exact instance identities.
    /// Input construction prepares class and protocol metadata; callable conversion starts inside
    /// the controlled attempt.
    #[test]
    fn subclass_callable_preserves_stored_identities() {
        let mut db = TestDbBuilder::new()
            .with_python_version(ruff_python_ast::PythonVersion::PY313)
            .build()
            .unwrap();
        db.write_file("src/main.py", "class C[T]: pass\n").unwrap();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture generic class");
        };
        let definition = prepared.semantic_index().expect_single_definition(class);
        let Some(ClassLiteral::Static(origin)) =
            infer_definition_types(&db, definition).original_class_type(definition)
        else {
            panic!("fixture static class");
        };
        let Some(context) = origin.generic_context(&db) else {
            panic!("fixture generic context");
        };
        let alias = GenericAlias::new(
            &db,
            origin,
            context.specialize(&db, &[Type::bool_literal(true)]),
        );
        let generic = SubclassOfType::from(&db, &env, ClassType::Generic(alias));
        let Type::ProtocolInstance(protocol) = KnownClass::Hashable.to_instance(&db, &env) else {
            panic!("Hashable protocol input");
        };
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let inputs = [
            generic,
            SubclassOfType::subclass_of_any(),
            SubclassOfType::subclass_of_unknown(),
            SubclassOfType::from_protocol(protocol),
            SubclassOfType::from(&db, &env, variable),
        ];
        for input in inputs {
            let Type::SubclassOf(subclass) = input else {
                panic!("fixture must preserve its subclass wrapper");
            };
            observations::reset(None);
            observe(Action::Record);
            let recording = Recording::start(Stop::None);
            let result = controlled(
                &prepared,
                Request::Convert(
                    Input::Type(input),
                    UpcastPolicy::Sound,
                    ConversionGuard::Omitted,
                ),
                &funded(),
            );
            drop(recording);
            assert!(BOUNDARY.get().reached, "{result:?}");
            assert_eq!(BOUNDARY.get().active_scopes, 0);
            assert!(journal().commits.is_empty());
            assert!(journal().retired.is_empty());
            let expected = match subclass.subclass_of() {
                SubclassOfInner::Class(class) => {
                    let instance = Type::instance(&db, &env, class);
                    let Type::NominalInstance(nominal) = instance else {
                        panic!("fixture nominal instance");
                    };
                    assert_eq!(nominal.class(&db, &env), ClassType::Generic(alias));
                    instance
                }
                SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic),
                SubclassOfInner::Protocol(protocol) => Type::ProtocolInstance(protocol),
                SubclassOfInner::TypeVar(variable) => Type::TypeVar(variable),
            };
            let callable = completed(&db, result, expected);
            let retry = controlled(
                &prepared,
                Request::Convert(
                    Input::Type(input),
                    UpcastPolicy::Sound,
                    ConversionGuard::Omitted,
                ),
                &funded(),
            );
            assert_eq!(completed(&db, retry, expected), callable);
        }
    }

    /// Work, byte exhaustion, and cancellation after top-parameter construction release the supplied
    /// guard and leave a funded conversion reusable in the same database revision.
    #[test]
    fn subclass_callable_retained_parameters_interrupt_and_retry() {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Interruption {
            Work,
            Bytes,
            Cancel,
        }

        /// Executes one budgeted attempt and verifies a funded retry after all cleanup has run.
        fn run(limit: AnalysisPolicy, action: Action) -> (Boundary, Option<AnalysisIncomplete>) {
            let db = fixture();
            let prepared = prepare(&db);
            let revision = salsa::plumbing::current_revision(&db);
            let request = Request::Convert(
                Input::Type(SubclassOfType::subclass_of_any()),
                UpcastPolicy::Sound,
                ConversionGuard::Supplied,
            );
            observations::reset(None);
            observe(action);
            let recording = Recording::start(Stop::None);
            let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled(&prepared, request, &limit)
            }));
            drop(recording);
            let boundary = BOUNDARY.get();
            if boundary.reached {
                assert!(boundary.active_scopes > 0);
                assert_cleanup();
            }
            let incomplete = match result {
                Err(salsa::Cancelled::Local) if action == Action::Cancel => None,
                Ok(Ok(AnalysisOutcome::Incomplete {
                    reason,
                    completed: (),
                })) if action == Action::Record => Some(reason),
                Ok(Ok(AnalysisOutcome::Complete(Output::Callables(Some(_)))))
                    if action == Action::Record =>
                {
                    None
                }
                result => panic!("subclass conversion interruption: {result:?}"),
            };
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            observe(Action::Record);
            let recording = Recording::start(Stop::None);
            let retry = controlled(&prepared, request, &funded());
            drop(recording);
            completed(&db, retry, Type::any());
            assert_cleanup();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            (boundary, incomplete)
        }

        let (
            Boundary {
                reached: true,
                remaining: Some(remaining),
                ..
            },
            None,
        ) = run(funded(), Action::Record)
        else {
            panic!("funded conversion did not reach retained parameters");
        };
        for interruption in [
            Interruption::Work,
            Interruption::Bytes,
            Interruption::Cancel,
        ] {
            let mut policy = funded();
            match interruption {
                Interruption::Work => policy.semantic_work_limit -= remaining,
                Interruption::Bytes => {
                    let mut low = 0;
                    let mut high = policy.requested_bytes_limit;
                    while low < high {
                        let middle = low + (high - low) / 2;
                        let (boundary, _) = run(
                            AnalysisPolicy {
                                requested_bytes_limit: middle,
                                ..funded()
                            },
                            Action::Record,
                        );
                        if boundary.reached {
                            high = middle;
                        } else {
                            low = middle + 1;
                        }
                    }
                    policy.requested_bytes_limit = high;
                }
                Interruption::Cancel => {}
            }
            let action = match interruption {
                Interruption::Cancel => Action::Cancel,
                Interruption::Work | Interruption::Bytes => Action::Record,
            };
            let (boundary, incomplete) = run(policy, action);
            assert!(boundary.reached, "{interruption:?}");
            let expected = match interruption {
                Interruption::Work => Some(AnalysisIncomplete::WorkLimit),
                Interruption::Bytes => Some(AnalysisIncomplete::RequestedAllocationLimit),
                Interruption::Cancel => None,
            };
            assert_eq!(incomplete, expected, "{interruption:?}");
        }
    }
}
