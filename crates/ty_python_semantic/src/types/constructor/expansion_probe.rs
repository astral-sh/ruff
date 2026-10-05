//! An external experimental session for real constructor expansion.
//!
//! Salsa still owns source fixed points. This only supervises callable expansion; synchronous
//! source queries and descriptor relations do not yet have a complete local-work contract.

#[cfg(test)]
mod infinite_probe;

#[cfg(test)]
mod initializer_mapping;

#[cfg(test)]
pub(in crate::types) mod search_observation;

#[cfg(test)]
pub(in crate::types) mod charge_ledger;

#[cfg(test)]
pub(in crate::types) mod descriptor_observation;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::{Future, poll_fn, ready};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::frozen::FrozenMap;

use super::bindings::{ConstructorBindingsEffects, constructor_bindings_with};
use super::effects::{ConstructorError, checked_source};
use super::member_resolution::ObjectInitializer;
use super::{ConstructorMember, ConstructorMembers};
use crate::types::call::Bindings;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::constructor_preparation::ConstructorBindingStorageEffects;
use crate::types::call::bindings::{
    BindingsEffects, InlineBindingsEffects, InstanceBindingsWork, instance_bindings_with,
};
use crate::types::cyclic::{CallableExpansion, CallableRecursionGuard};
use crate::types::visitor::{SearchControl, SearchOperation, SearchWork};
use crate::types::{
    ClassLiteral, ClassType, DescriptorOrigin, GenericContext, KnownClass, MemberLookupPolicy,
    MemberLookupResult, StaticClassLiteral, SubclassOfInner, Truthiness, Type,
};
use crate::{Db, ProgramEnvironment};

thread_local! {
    static SESSION: RefCell<Option<Rc<RefCell<Session>>>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Incomplete {
    Scheduling(crate::types::relation::execution::SchedulingFailure),
    Allowance,
    RequestedAllocation,
    ConstraintCapacityExhausted,
    ExactCycle,
    UnsupportedBindings,
    #[cfg(test)]
    UnsupportedSignatureOperation(crate::types::signatures::effects::SignatureEffect),
    #[cfg(test)]
    UnsupportedPairOperation(crate::types::relation::runtime::UnsupportedPairOperation),
    #[cfg(test)]
    UnsupportedProtocolInterfaceOperation(
        crate::types::protocol_class::interface_build::runtime::UnsupportedProtocolInterfaceOperation,
    ),
    #[cfg(test)]
    UnsupportedSequentOperation(crate::types::constraints::UnsupportedSequentOperation),
    #[cfg(test)]
    UnsupportedSatisfactionOperation(
        crate::types::constraints::runtime::UnsupportedSatisfactionOperation,
    ),
    UnsupportedSearchOperation(SearchOperation),
    UnsupportedMroOperation(crate::types::mro::attempt::UnsupportedMroOperation),
    UnsupportedInstanceOperation(crate::types::instance::attempt::UnsupportedInstanceOperation),
    UnsupportedMappingOperation(crate::types::mapping::attempt::UnsupportedMappingOperation),
    UnsupportedSelfBindingOperation(
        crate::types::mapping::self_binding::attempt::UnsupportedSelfBindingOperation,
    ),
    NestedRoot,
    Interrupted,
    Start(salsa::attempt_probe::StartError),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum Observation {
    ClassContextEntered(salsa::Id),
    ClassContextExited(salsa::Id),
    ClassDefaultReadEntered(salsa::Id),
    ClassDefaultReadExited(salsa::Id),
    TupleNormalization(salsa::Id),
    InstanceBindingsWork {
        work: InstanceBindingsWork,
        accepted: bool,
    },
    SearchWork {
        work: SearchWork,
        accepted: bool,
    },
    InitializerBindingResumed,
    InitializerMappingStarted {
        variable: Option<salsa::Id>,
        receiver_class: Option<salsa::Id>,
    },
    InitializerMappingFinished {
        complete: bool,
        matches_receiver: bool,
        frames: usize,
        dropped_frames: usize,
    },
    #[cfg(test)]
    SearchStarted {
        scope: usize,
        parent: Option<usize>,
        owner: Option<salsa::Id>,
    },
    #[cfg(test)]
    SearchFinished(search_observation::SearchStatistics),
    Execute(salsa::DatabaseKeyIndex),
    Iterate(salsa::DatabaseKeyIndex),
    Finalize(salsa::DatabaseKeyIndex),
    Constructor(salsa::Id),
    ConstructorCompleted {
        class: salsa::Id,
    },
    ConstructorCallableCompleted {
        class: salsa::Id,
    },
    Debit {
        accepted: bool,
    },
    Refusal,
    RepeatedDemand,
    DefinitionSeed {
        query: salsa::Id,
        definition: salsa::Id,
        canonical: bool,
    },
    DefinitionRecovery(salsa::Id),
    DefinitionNormalization(salsa::Id),
    DefinitionNormalized {
        query: salsa::Id,
        binding_changed: bool,
        result_changed: bool,
    },
    PhaseEntered {
        scope: usize,
        parent: Option<usize>,
        phase: NormalizationPhase,
    },
    PhaseExited(usize),
    PhaseConstructor {
        scope: usize,
        class: salsa::Id,
    },
    PhaseDebit {
        scope: usize,
        accepted: bool,
    },
    ComparisonMaps {
        phase: NormalizationPhase,
        current: usize,
        previous: usize,
    },
    ComparisonEntry {
        phase: NormalizationPhase,
        expression: ExpressionNodeKey,
        current: Option<Truthiness>,
        previous: Option<Truthiness>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum NormalizationOwner {
    Definition(salsa::Id),
    Expression(salsa::Id),
    Statement(salsa::Id),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum FallbackSide {
    Current,
    Previous,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum NormalizationPhaseKind {
    Callback,
    ComparisonWidening,
    TypeFields,
    AbsentOverride {
        expression: ExpressionNodeKey,
        side: FallbackSide,
        nominal_instance: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct NormalizationPhase {
    owner: NormalizationOwner,
    query: salsa::Id,
    query_name: &'static str,
    iteration: u32,
    kind: NormalizationPhaseKind,
}

#[derive(Clone, Copy)]
struct ActivePhase {
    scope: usize,
    phase: NormalizationPhase,
}

pub(in crate::types) struct ObservationScope {
    installed: Option<(Rc<RefCell<Session>>, Option<ActivePhase>, usize)>,
}

impl Drop for ObservationScope {
    fn drop(&mut self) {
        if let Some((state, previous, scope)) = &self.installed {
            let mut state = state.borrow_mut();
            state.phase = *previous;
            if let Some(observations) = &mut state.statistics.observations {
                observations.push(Observation::PhaseExited(*scope));
            }
        }
    }
}

fn enter_phase(phase: Option<NormalizationPhase>) -> ObservationScope {
    let installed = SESSION.with(|current| {
        let state = current.borrow().as_ref()?.clone();
        let phase = phase?;
        let (previous, scope) = {
            let mut state = state.borrow_mut();
            let previous = state.phase;
            let observations = state.statistics.observations.as_mut()?;
            let scope = observations.len();
            observations.push(Observation::PhaseEntered {
                scope,
                parent: previous.map(|previous| previous.scope),
                phase,
            });
            state.phase = Some(ActivePhase { scope, phase });
            (previous, scope)
        };
        Some((state, previous, scope))
    });
    ObservationScope { installed }
}

fn current_phase() -> Option<ActivePhase> {
    SESSION.with(|current| current.borrow().as_ref()?.borrow().phase)
}

pub(in crate::types) fn observe_normalizer(
    cycle: &salsa::Cycle,
    query_name: &'static str,
    owner: NormalizationOwner,
) -> ObservationScope {
    enter_phase(Some(NormalizationPhase {
        owner,
        query: cycle.id(),
        query_name,
        iteration: cycle.iteration(),
        kind: NormalizationPhaseKind::Callback,
    }))
}

pub(in crate::types) fn observe_normalization_phase(
    kind: NormalizationPhaseKind,
) -> ObservationScope {
    enter_phase(current_phase().map(|active| NormalizationPhase {
        kind,
        ..active.phase
    }))
}

pub(in crate::types) fn observe(observation: Observation) {
    SESSION.with(|current| {
        if let Some(state) = current.borrow().as_ref()
            && let Some(observations) = state.borrow_mut().statistics.observations.as_mut()
        {
            observations.push(observation);
        }
    });
}

pub(in crate::types) struct ClassContextObservation(salsa::Id);

pub(in crate::types) fn observe_class_context(
    class: StaticClassLiteral<'_>,
) -> ClassContextObservation {
    let id = class.as_id();
    observe(Observation::ClassContextEntered(id));
    ClassContextObservation(id)
}

impl Drop for ClassContextObservation {
    fn drop(&mut self) {
        observe(Observation::ClassContextExited(self.0));
    }
}

pub(in crate::types) fn observe_constructor_callable_completed(class: ClassType<'_>) {
    if let ClassType::NonGeneric(ClassLiteral::Static(class)) = class {
        observe(Observation::ConstructorCallableCompleted {
            class: class.as_id(),
        });
    }
}

pub(in crate::types) fn observe_comparison_maps(
    current: Option<&FrozenMap<ExpressionNodeKey, Truthiness>>,
    previous: Option<&FrozenMap<ExpressionNodeKey, Truthiness>>,
) {
    let Some(ActivePhase { phase, .. }) = current_phase() else {
        return;
    };
    observe(Observation::ComparisonMaps {
        phase,
        current: current.map_or(0, |map| map.iter().count()),
        previous: previous.map_or(0, |map| map.iter().count()),
    });
    for (expression, _) in current.into_iter().chain(previous).flatten() {
        observe(Observation::ComparisonEntry {
            phase,
            expression: *expression,
            current: current.and_then(|map| map.get(expression)).copied(),
            previous: previous.and_then(|map| map.get(expression)).copied(),
        });
    }
}

pub(in crate::types) fn observing() -> bool {
    SESSION.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|state| state.borrow().statistics.observations.is_some())
    })
}

pub(in crate::types) fn observe_salsa_event(event: &salsa::EventKind) {
    let observation = match *event {
        salsa::EventKind::WillExecute { database_key } => Observation::Execute(database_key),
        salsa::EventKind::WillIterateCycle { database_key, .. } => {
            Observation::Iterate(database_key)
        }
        salsa::EventKind::DidFinalizeCycle { database_key, .. } => {
            Observation::Finalize(database_key)
        }
        _ => return,
    };
    observe(observation);
}

#[derive(Debug, Default)]
pub(in crate::types) struct Statistics {
    debits: usize,
    search_work: usize,
    search_predicates: usize,
    search_peak_pending: usize,
    expansions: usize,
    direct: usize,
    conversions: usize,
    task_polls: usize,
    producers: usize,
    drivers: usize,
    active_drivers: usize,
    max_drivers: usize,
    stack_span: usize,
    observations: Option<Vec<Observation>>,
}

impl Statistics {
    pub(in crate::types) fn observations(&self) -> &[Observation] {
        self.observations.as_deref().unwrap_or_default()
    }
}

struct Session {
    reason: Option<Incomplete>,
    stack_origin: usize,
    statistics: Statistics,
    phase: Option<ActivePhase>,
    mro_effects: bool,
}

struct InstalledSession;

impl Drop for InstalledSession {
    fn drop(&mut self) {
        SESSION.with(|current| current.borrow_mut().take());
    }
}

pub(in crate::types) fn run<R>(
    db: &dyn Db,
    allowance: usize,
    body: impl FnOnce() -> R,
) -> (Result<R, Incomplete>, Statistics) {
    run_inner(db, allowance, false, body)
}

fn run_inner<R>(
    db: &dyn Db,
    allowance: usize,
    observe: bool,
    body: impl FnOnce() -> R,
) -> (Result<R, Incomplete>, Statistics) {
    run_selected(db, allowance, observe, false, body)
}

pub(in crate::types) fn run_mro<R>(
    db: &dyn Db,
    allowance: usize,
    body: impl FnOnce() -> R,
) -> (Result<R, Incomplete>, Statistics) {
    run_selected(db, allowance, false, true, body)
}

pub(in crate::types) fn run_mro_observed<R>(
    db: &dyn Db,
    allowance: usize,
    body: impl FnOnce() -> R,
) -> (Result<R, Incomplete>, Statistics) {
    run_selected(db, allowance, true, true, body)
}

pub(in crate::types) fn bind_initializer_self<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    initializer: Type<'db>,
    receiver: Type<'db>,
) -> Result<Type<'db>, Incomplete> {
    continue_work(db)?;
    let receiver_class = receiver.as_nominal_instance().and_then(|instance| {
        match instance.children_for_visitor(db) {
            crate::types::instance::NominalVisitorChildren::Class(Type::ClassLiteral(
                ClassLiteral::Static(class),
            )) => Some(class.as_id()),
            _ => None,
        }
    });
    observe(Observation::InitializerMappingStarted {
        variable: initializer.as_typevar().map(|variable| variable.as_id()),
        receiver_class,
    });
    let (result, statistics) =
        crate::types::mapping::attempt::bind_self(db, env, initializer, receiver, None);
    observe(Observation::InitializerMappingFinished {
        complete: result.is_ok(),
        matches_receiver: result == Ok(receiver),
        frames: statistics.frames,
        dropped_frames: statistics.dropped_frames,
    });
    result
}

fn run_selected<R>(
    db: &dyn Db,
    allowance: usize,
    observe: bool,
    mro_effects: bool,
    body: impl FnOnce() -> R,
) -> (Result<R, Incomplete>, Statistics) {
    if active() {
        return (Err(Incomplete::NestedRoot), Statistics::default());
    }
    let stack_marker = 0u8;
    let state = Rc::new(RefCell::new(Session {
        reason: None,
        stack_origin: (&stack_marker as *const u8) as usize,
        statistics: Statistics {
            observations: observe.then(Vec::new),
            ..Statistics::default()
        },
        phase: None,
        mro_effects,
    }));
    SESSION.with(|current| current.replace(Some(Rc::clone(&state))));
    let installed = InstalledSession;
    let result = match salsa::attempt_probe::try_with_attempt(db, allowance, body) {
        Ok(salsa::attempt_probe::AttemptOutcome::Complete(value)) => Ok(value),
        Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(reason)) => {
            Err(runtime_failure(reason))
        }
        Err(reason) => Err(Incomplete::Start(reason)),
    };
    drop(installed);
    let statistics = std::mem::take(&mut state.borrow_mut().statistics);
    (result, statistics)
}

pub(in crate::types) fn active() -> bool {
    SESSION.with(|current| current.borrow().is_some())
}

pub(in crate::types) fn mro_effects_enabled() -> bool {
    SESSION.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|state| state.borrow().mro_effects)
    })
}

fn remember_failure(reason: Incomplete) -> Incomplete {
    SESSION.with(|current| {
        let state = current.borrow();
        let Some(state) = state.as_ref() else {
            return reason;
        };
        let mut state = state.borrow_mut();
        if state.reason.is_none()
            && let Some(observations) = &mut state.statistics.observations
        {
            observations.push(Observation::Refusal);
        }
        *state.reason.get_or_insert(reason)
    })
}

fn runtime_failure(reason: salsa::attempt_probe::Incomplete) -> Incomplete {
    remember_failure(match reason {
        salsa::attempt_probe::Incomplete::Allowance => Incomplete::Allowance,
        salsa::attempt_probe::Incomplete::RequestedAllocation => Incomplete::RequestedAllocation,
        salsa::attempt_probe::Incomplete::Interrupted => Incomplete::Interrupted,
    })
}

#[cfg_attr(test, track_caller)]
pub(in crate::types) fn refuse(db: &dyn Db, reason: Incomplete) -> Incomplete {
    #[cfg(test)]
    charge_ledger::refusal(reason);
    match salsa::attempt_probe::report_incomplete(db, salsa::attempt_probe::Incomplete::Interrupted)
    {
        salsa::attempt_probe::Incomplete::Interrupted => remember_failure(reason),
        reason => runtime_failure(reason),
    }
}

pub(in crate::types) fn stopped(db: &dyn Db) -> bool {
    active() && salsa::attempt_probe::is_incomplete(db)
}

#[cfg_attr(test, track_caller)]
pub(in crate::types) fn continue_work(db: &dyn Db) -> Result<(), Incomplete> {
    let charged = salsa::attempt_probe::charge(db, 0);
    #[cfg(test)]
    charge_ledger::record(charge_ledger::Channel::Continuation, 0, charged);
    charged.map_err(runtime_failure)
}

#[cfg_attr(test, track_caller)]
pub(in crate::types) fn charge_work(db: &dyn Db, units: usize) -> Result<(), Incomplete> {
    let charged = salsa::attempt_probe::charge(db, units);
    #[cfg(test)]
    charge_ledger::record(charge_ledger::Channel::Weighted, units, charged);
    charged.map_err(runtime_failure)
}

pub(in crate::types) struct AttemptSearchControl<'db> {
    db: &'db dyn Db,
}

impl<'db> AttemptSearchControl<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl SearchControl for AttemptSearchControl<'_> {
    type Error = Incomplete;

    fn admit(&mut self, work: SearchWork) -> Result<(), Self::Error> {
        #[cfg(test)]
        let _charge = charge_ledger::scope(&work);
        let result = if let SearchWork::Semantic(operation) = work {
            continue_work(self.db).and_then(|()| {
                Err(refuse(
                    self.db,
                    Incomplete::UnsupportedSearchOperation(operation),
                ))
            })
        } else {
            debit(self.db, |statistics| {
                statistics.search_work += 1;
                match work {
                    SearchWork::Predicate => statistics.search_predicates += 1,
                    SearchWork::PendingFrame { held } => {
                        statistics.search_peak_pending =
                            statistics.search_peak_pending.max(held + 1);
                    }
                    _ => {}
                }
            })
        };
        observe(Observation::SearchWork {
            work,
            accepted: result.is_ok(),
        });
        result
    }
}

#[cfg_attr(test, track_caller)]
fn debit(db: &dyn Db, update: impl FnOnce(&mut Statistics)) -> Result<(), Incomplete> {
    let charged = salsa::attempt_probe::charge(db, 1);
    #[cfg(test)]
    charge_ledger::record(charge_ledger::Channel::Checkpoint, 1, charged);
    observe(Observation::Debit {
        accepted: charged.is_ok(),
    });
    if let Some(phase) = current_phase() {
        observe(Observation::PhaseDebit {
            scope: phase.scope,
            accepted: charged.is_ok(),
        });
    }
    charged.map_err(runtime_failure)?;
    let stack_marker = 0u8;
    SESSION.with(|current| {
        let state = current.borrow().clone();
        let Some(state) = state else { return };
        let mut state = state.borrow_mut();
        let distance = state
            .stack_origin
            .abs_diff((&stack_marker as *const u8) as usize);
        state.statistics.stack_span = state.statistics.stack_span.max(distance);
        state.statistics.debits += 1;
        update(&mut state.statistics);
    });
    Ok(())
}

pub(in crate::types) fn admit(db: &dyn Db, mode: CallableExpansion) -> Result<(), Incomplete> {
    #[cfg(test)]
    let _charge = charge_ledger::scope(&mode);
    debit(db, |statistics| {
        statistics.expansions += 1;
        match mode {
            CallableExpansion::Bindings => statistics.direct += 1,
            CallableExpansion::Upcast => statistics.conversions += 1,
        }
    })
}

pub(in crate::types) fn exact_cycle(db: &dyn Db) -> Incomplete {
    refuse(db, Incomplete::ExactCycle)
}

struct DriverScope;

impl DriverScope {
    fn enter(db: &dyn Db) -> Result<Self, Incomplete> {
        debit(db, |statistics| {
            statistics.drivers += 1;
            statistics.active_drivers += 1;
            statistics.max_drivers = statistics.max_drivers.max(statistics.active_drivers);
        })?;
        Ok(Self)
    }
}

impl Drop for DriverScope {
    fn drop(&mut self) {
        SESSION.with(|current| {
            if let Some(state) = current.borrow().as_ref() {
                state.borrow_mut().statistics.active_drivers -= 1;
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Request<'db> {
    Constructor {
        receiver: Type<'db>,
        class: ClassType<'db>,
    },
    Callable {
        ty: Type<'db>,
        unknown_is_recovery: bool,
    },
}

#[derive(Default)]
struct Replies<'db> {
    completed: FxHashMap<Request<'db>, Bindings<'db>>,
    demands: Vec<(Request<'db>, Request<'db>)>,
}

struct Effects<'a, 'db> {
    db: &'db dyn Db,
    replies: &'a RefCell<Replies<'db>>,
    owner: Request<'db>,
    inline: InlineBindingsEffects<'a, 'db>,
}

impl<'db> Effects<'_, 'db> {
    async fn demand(&self, child: Request<'db>) -> Result<Bindings<'db>, Incomplete> {
        let mut requested = false;
        poll_fn(|_| {
            if let Err(reason) = continue_work(self.db) {
                return Poll::Ready(Err(reason));
            }
            let mut replies = self.replies.borrow_mut();
            if let Some(value) = replies.completed.get(&child) {
                return Poll::Ready(Ok(value.clone()));
            }
            if !requested {
                replies.demands.push((self.owner, child));
                requested = true;
            }
            Poll::Pending
        })
        .await
    }
}

impl<'db> BindingsEffects<'db> for Effects<'_, 'db> {
    type Error = Incomplete;

    fn checkpoint(&self, db: &dyn Db, work: InstanceBindingsWork) -> Result<(), Self::Error> {
        #[cfg(test)]
        let _charge = charge_ledger::scope(&work);
        let result = debit(db, |_| {});
        observe(Observation::InstanceBindingsWork {
            work,
            accepted: result.is_ok(),
        });
        result
    }

    fn call_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(
            checked_source(db, || {
                Ok(ty.member_lookup_with_recursion_guard(
                    db,
                    env,
                    "__call__",
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    None,
                    Some(guard),
                ))
            })
            .map_err(|error| match error {
                ConstructorError::Incomplete(reason) => reason,
            }),
        )
    }

    async fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        let mut bindings = self
            .demand(Request::Callable {
                ty,
                unknown_is_recovery: origin.return_contains_recursive_recovery,
            })
            .await?;
        continue_work(db)?;
        bindings.add_descriptor_origin(db, origin);
        Ok(bindings)
    }
}

impl<'db> ConstructorBindingStorageEffects<'db> for Effects<'_, 'db> {
    async fn bind_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
        self_type: Type<'db>,
        instance_type: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.inline
            .bind_new(db, env, bindings, self_type, instance_type)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn wrap_constructor(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error> {
        self.inline
            .wrap_constructor(db, bindings, instance_type, kind)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn mark_unbound(
        &self,
        bindings: &mut Bindings<'db>,
        kind: ConstructorCallableKind,
    ) -> Result<(), Self::Error> {
        self.inline
            .mark_unbound(bindings, kind)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn bind_initializer_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        self.inline
            .bind_initializer_self(db, env, bindings)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn attach_downstream(
        &self,
        bindings: &mut Bindings<'db>,
        downstream: &Bindings<'db>,
    ) -> Result<(), Self::Error> {
        self.inline
            .attach_downstream(bindings, downstream)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn apply_class_context(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        context: Option<GenericContext<'db>>,
    ) -> Result<(), Self::Error> {
        self.inline
            .apply_class_context(db, bindings, context)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn fallback(
        &self,
        receiver: Type<'db>,
        context: Option<GenericContext<'db>>,
        return_type: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        self.inline
            .fallback(receiver, context, return_type)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn transfer(&self, bindings: &Bindings<'db>) -> Result<(), Self::Error> {
        self.inline
            .transfer(bindings)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }
}

impl<'db> ConstructorBindingsEffects<'db> for Effects<'_, 'db> {
    async fn decision<T: Copy>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.inline
            .decision(action)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error> {
        self.inline
            .class_literal(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        self.inline
            .generic_context(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.inline
            .is_typed_dict(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn generated_typed_dict(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.inline
            .generated_typed_dict(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn known(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        self.inline
            .known(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn enum_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.inline
            .enum_class(db, env)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn is_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        self.inline
            .is_subclass(db, env, class, target)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn identity_class(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.inline
            .identity_class(db, class)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn instance_approximation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.inline
            .instance_approximation(db, env, receiver)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn to_class_type(
        &self,
        db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.inline
            .to_class_type(db, receiver)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn metaclass_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error> {
        self.inline
            .metaclass_call(db, env, members, guard)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    async fn new_method(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<ConstructorMember<'db>, Self::Error> {
        self.inline
            .new_method(db, env, members, guard)
            .await
            .map_err(|ConstructorError::Incomplete(reason)| reason)
    }

    fn initializer(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        include_object: ObjectInitializer,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> impl Future<Output = Result<ConstructorMember<'db>, Self::Error>> {
        ready(
            checked_source(db, || {
                members.initializer(
                    db,
                    env,
                    match include_object {
                        ObjectInitializer::Include => true,
                        ObjectInitializer::Exclude => false,
                    },
                    recursion_guard,
                )
            })
            .map_err(|error| match error {
                ConstructorError::Incomplete(reason) => reason,
            }),
        )
    }
}

type Task<'a, 'db> = Pin<Box<dyn Future<Output = Result<Bindings<'db>, Incomplete>> + 'a>>;

fn task<'a, 'db: 'a>(
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    replies: &'a RefCell<Replies<'db>>,
    request: Request<'db>,
) -> Task<'a, 'db> {
    Box::pin(async move {
        continue_work(db)?;
        let guard = CallableRecursionGuard::new();
        let effects = Effects {
            db,
            replies,
            owner: request,
            inline: InlineBindingsEffects {
                recursion_guard: &guard,
            },
        };
        let (receiver, class) = match request {
            Request::Constructor { receiver, class } => (receiver, class),
            Request::Callable {
                ty,
                unknown_is_recovery,
            } => {
                admit(db, CallableExpansion::Bindings)?;
                match ty {
                    Type::ClassLiteral(class) => {
                        if let Some(bindings) = ty.known_class_literal_bindings(db, env, class) {
                            continue_work(db)?;
                            return Ok(bindings);
                        }
                        continue_work(db)?;
                        (ty, ClassType::NonGeneric(class))
                    }
                    Type::GenericAlias(alias) => (ty, ClassType::Generic(alias)),
                    Type::SubclassOf(subclass)
                        if let SubclassOfInner::Class(class) = subclass.subclass_of() =>
                    {
                        (ty, class)
                    }
                    Type::Union(union) => {
                        let mut alternatives = Vec::new();
                        for ty in union.elements(db) {
                            alternatives.push(
                                effects
                                    .demand(Request::Callable {
                                        ty: *ty,
                                        unknown_is_recovery,
                                    })
                                    .await?,
                            );
                        }
                        return Ok(Bindings::from_union(ty, alternatives));
                    }
                    Type::NominalInstance(_)
                    | Type::ProtocolInstance(_)
                    | Type::NewTypeInstance(_) => {
                        return instance_bindings_with(db, env, ty, &guard, &effects).await;
                    }
                    Type::FunctionLiteral(_)
                    | Type::Callable(_)
                    | Type::BoundMethod(_)
                    | Type::Dynamic(_) => {
                        let bindings =
                            ty.bindings_with_recovery(db, env, &guard, unknown_is_recovery);
                        continue_work(db)?;
                        return Ok(bindings);
                    }
                    _ => return Err(refuse(db, Incomplete::UnsupportedBindings)),
                }
            }
        };
        let bindings =
            constructor_bindings_with(db, env, receiver, class, &guard, &effects).await?;
        continue_work(db)?;
        Ok(bindings)
    })
}

pub(in crate::types) fn bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Type<'db>,
    class: ClassType<'db>,
) -> Result<Bindings<'db>, Incomplete> {
    if let ClassType::NonGeneric(ClassLiteral::Static(class)) = class {
        if let Some(phase) = current_phase() {
            observe(Observation::PhaseConstructor {
                scope: phase.scope,
                class: class.as_id(),
            });
        }
        observe(Observation::Constructor(class.as_id()));
    }
    let _scope = DriverScope::enter(db)?;
    let root = Request::Constructor { receiver, class };
    let replies = RefCell::new(Replies::default());
    let mut tasks = FxHashMap::default();
    let mut ready = VecDeque::from([root]);
    let mut waiting: FxHashMap<_, FxHashSet<_>> = FxHashMap::default();
    debit(db, |statistics| statistics.producers += 1)?;
    tasks.insert(root, task(db, env, &replies, root));
    let mut context = Context::from_waker(Waker::noop());
    while let Some(request) = ready.pop_front() {
        debit(db, |statistics| statistics.task_polls += 1)?;
        let Some(future) = tasks.get_mut(&request) else {
            continue;
        };
        let answer = future.as_mut().poll(&mut context);
        continue_work(db)?;
        if let Poll::Ready(answer) = answer {
            let answer = answer?;
            match request {
                Request::Constructor {
                    class: ClassType::NonGeneric(ClassLiteral::Static(class)),
                    ..
                }
                | Request::Callable {
                    ty: Type::ClassLiteral(ClassLiteral::Static(class)),
                    ..
                } => observe(Observation::ConstructorCompleted {
                    class: class.as_id(),
                }),
                Request::Callable {
                    ty: Type::SubclassOf(subclass),
                    ..
                } if let SubclassOfInner::Class(ClassType::NonGeneric(ClassLiteral::Static(
                    class,
                ))) = subclass.subclass_of() =>
                {
                    observe(Observation::ConstructorCompleted {
                        class: class.as_id(),
                    });
                }
                _ => {}
            }
            tasks.remove(&request);
            if request == root {
                return Ok(answer);
            }
            replies.borrow_mut().completed.insert(request, answer);
            if let Some(parents) = waiting.remove(&request) {
                ready.extend(parents);
            }
        }
        let demands = std::mem::take(&mut replies.borrow_mut().demands);
        for (parent, child) in demands {
            if replies.borrow().completed.contains_key(&child) {
                ready.push_back(parent);
            } else {
                waiting.entry(child).or_default().insert(parent);
                if let std::collections::hash_map::Entry::Vacant(entry) = tasks.entry(child) {
                    debit(db, |statistics| statistics.producers += 1)?;
                    entry.insert(task(db, env, &replies, child));
                    ready.push_back(child);
                }
            }
        }
    }
    Err(exact_cycle(db))
}

#[cfg(test)]
mod tests {
    mod descriptor_relation;
    mod mapping_timing;

    use ruff_db::diagnostic::Diagnostic;
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::PythonVersion;
    use salsa::Database;
    use salsa::plumbing::FromId;
    use salsa::prepared_source_probe::Stamp;
    use std::collections::BTreeSet;
    use ty_python_core::definition::{Definition, DefinitionKind};
    use ty_python_core::expression::Expression;
    use ty_python_core::statement::StatementInner;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::types::StaticClassLiteral;
    use crate::types::mapping::attempt::frame_observation;

    const PEP695: &str = include_str!("../../../resources/mdtest/generics/pep695/callables.md");
    const LEGACY: &str = include_str!("../../../resources/mdtest/generics/legacy/callables.md");

    #[test]
    fn runtime_incomplete_round_trip() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        for (runtime_reason, probe_reason) in [
            (
                salsa::attempt_probe::Incomplete::Allowance,
                Incomplete::Allowance,
            ),
            (
                salsa::attempt_probe::Incomplete::RequestedAllocation,
                Incomplete::RequestedAllocation,
            ),
            (
                salsa::attempt_probe::Incomplete::Interrupted,
                Incomplete::Interrupted,
            ),
        ] {
            let (outcome, _) = run(&db, 10, || {
                assert_eq!(
                    salsa::attempt_probe::report_incomplete(&db, runtime_reason),
                    runtime_reason
                );
                assert_eq!(continue_work(&db), Err(probe_reason));
            });
            assert_eq!(outcome, Err(probe_reason));
            assert!(!active());
        }
        Ok(())
    }

    #[test]
    fn runtime_first_cause_precedes_probe_refusal() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        for (runtime_reason, probe_reason) in [
            (
                salsa::attempt_probe::Incomplete::Allowance,
                Incomplete::Allowance,
            ),
            (
                salsa::attempt_probe::Incomplete::RequestedAllocation,
                Incomplete::RequestedAllocation,
            ),
        ] {
            let (outcome, _) = run(&db, 10, || {
                salsa::attempt_probe::report_incomplete(&db, runtime_reason);
                assert_eq!(refuse(&db, Incomplete::ExactCycle), probe_reason);
                assert_eq!(continue_work(&db), Err(probe_reason));
            });
            assert_eq!(outcome, Err(probe_reason));
        }
        Ok(())
    }

    #[test]
    fn probe_first_cause_precedes_requested_allocation() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let (outcome, _) = run(&db, 10, || {
            assert_eq!(refuse(&db, Incomplete::ExactCycle), Incomplete::ExactCycle);
            assert_eq!(
                salsa::attempt_probe::report_incomplete(
                    &db,
                    salsa::attempt_probe::Incomplete::RequestedAllocation,
                ),
                salsa::attempt_probe::Incomplete::Interrupted
            );
            assert_eq!(continue_work(&db), Err(Incomplete::ExactCycle));
        });
        assert_eq!(outcome, Err(Incomplete::ExactCycle));
        Ok(())
    }

    fn assert_expected(source: &str, diagnostics: &[Diagnostic]) {
        let mut expected: Vec<_> = source
            .lines()
            .enumerate()
            .filter_map(|(line, text)| {
                let (_, comment) = text.split_once("# error: [")?;
                Some((line + 1, comment.split(']').next().unwrap_or(comment)))
            })
            .collect();
        let mut actual: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| {
                let start = diagnostic
                    .primary_span()
                    .and_then(|span| span.range())
                    .map_or(0, |range| usize::from(range.start()));
                let line = source[..start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1;
                (line, diagnostic.id().as_str())
            })
            .collect();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{diagnostics:#?}");
    }

    fn code(markdown: &str, heading: &str) -> anyhow::Result<String> {
        let (_, section) = markdown
            .split_once(&format!("## {heading}\n"))
            .ok_or_else(|| anyhow::anyhow!("missing section {heading}"))?;
        let section = section.split("\n## ").next().unwrap_or(section);
        let (_, code) = section
            .split_once("```py\n")
            .ok_or_else(|| anyhow::anyhow!("missing code for {heading}"))?;
        Ok(code.split("\n```").next().unwrap_or(code).to_owned())
    }

    fn observations(statistics: &Statistics) -> &[Observation] {
        statistics.observations.as_deref().unwrap_or_default()
    }

    fn normalization_file(db: &TestDb, owner: NormalizationOwner) -> File {
        match owner {
            NormalizationOwner::Definition(id) => Definition::from_id(id).file(db),
            NormalizationOwner::Expression(id) => Expression::from_id(id).file(db),
            NormalizationOwner::Statement(id) => StatementInner::from_id(id).file(db),
        }
    }

    fn assert_normalizer_drained(statistics: &Statistics) {
        assert_eq!(statistics.active_drivers, 0);
        assert!(!active());
        let mut scopes = Vec::new();
        for (index, event) in observations(statistics).iter().enumerate() {
            match *event {
                Observation::PhaseEntered { scope, parent, .. } => {
                    assert_eq!(scope, index);
                    assert_eq!(parent, scopes.last().copied());
                    scopes.push(scope);
                }
                Observation::PhaseExited(scope) => assert_eq!(scopes.pop(), Some(scope)),
                Observation::PhaseConstructor { scope, .. }
                | Observation::PhaseDebit { scope, .. } => {
                    assert_eq!(scopes.last(), Some(&scope));
                }
                _ => {}
            }
        }
        assert!(scopes.is_empty());
    }

    struct NormalizerCharge {
        phase: NormalizationPhase,
        class: salsa::Id,
        expression: ExpressionNodeKey,
        callback: usize,
        fallback_exit: usize,
        callback_exit: usize,
        debit: usize,
    }

    impl NormalizerCharge {
        fn matches_query(&self, phase: NormalizationPhase) -> bool {
            phase.query == self.phase.query && phase.query_name == self.phase.query_name
        }

        fn matches_key(&self, db: &TestDb, key: salsa::DatabaseKeyIndex) -> bool {
            key.key_index() == self.phase.query
                && db.ingredient_debug_name(key.ingredient_index()) == self.phase.query_name
        }

        fn preceding_debits(&self, events: &[Observation]) -> usize {
            events[..self.debit]
                .iter()
                .filter(|event| matches!(event, Observation::Debit { accepted: true }))
                .count()
        }

        fn assert_completed(&self, db: &TestDb, events: &[Observation]) {
            assert!(events[self.fallback_exit + 1..self.callback_exit].iter().any(|event| {
                matches!(event, Observation::PhaseEntered { parent: Some(parent), phase, .. }
                    if *parent == self.callback && self.matches_query(*phase)
                        && phase.iteration == self.phase.iteration
                        && matches!(phase.kind, NormalizationPhaseKind::TypeFields))
            }), "normalizer did not resume type fields after the fallback");
            assert!(
                events[self.callback_exit + 1..].iter().any(|event| {
                    matches!(event, Observation::Finalize(key) if self.matches_key(db, *key))
                }),
                "normalizer's source query did not finalize"
            );
        }
    }

    fn normalizer_charge(
        db: &TestDb,
        file: File,
        events: &[Observation],
        accepted: bool,
    ) -> anyhow::Result<NormalizerCharge> {
        for (entry, event) in events.iter().enumerate() {
            let Observation::PhaseEntered {
                scope,
                parent: Some(parent),
                phase,
            } = *event
            else {
                continue;
            };
            let NormalizationPhaseKind::AbsentOverride {
                expression,
                side: FallbackSide::Previous,
                nominal_instance: true,
            } = phase.kind
            else {
                continue;
            };
            if phase.iteration <= crate::TAINTED_CYCLES
                || normalization_file(db, phase.owner) != file
            {
                continue;
            }
            let Some(Observation::PhaseEntered {
                parent: Some(callback),
                phase: comparison,
                ..
            }) = events.get(parent)
            else {
                continue;
            };
            let Some(Observation::PhaseEntered {
                phase: callback_phase,
                ..
            }) = events.get(*callback)
            else {
                continue;
            };
            if !matches!(comparison.kind, NormalizationPhaseKind::ComparisonWidening)
                || !matches!(callback_phase.kind, NormalizationPhaseKind::Callback)
                || [comparison, callback_phase].iter().any(|parent_phase| {
                    parent_phase.query != phase.query
                        || parent_phase.query_name != phase.query_name
                        || parent_phase.iteration != phase.iteration
                })
            {
                continue;
            }
            let exit_for = |scope| {
                events[entry + 1..].iter().position(|event| {
                    matches!(event, Observation::PhaseExited(exited) if *exited == scope)
                }).map(|offset| entry + 1 + offset)
                    .ok_or_else(|| anyhow::anyhow!("normalizer scope {scope} did not exit"))
            };
            let fallback_exit = exit_for(scope)?;
            let callback_exit = exit_for(*callback)?;
            for (offset, sequence) in events[entry + 1..fallback_exit].windows(4).enumerate() {
                let [
                    Observation::PhaseConstructor {
                        scope: constructor_scope,
                        class,
                    },
                    Observation::Constructor(global_class),
                    Observation::Debit {
                        accepted: global_accepted,
                    },
                    Observation::PhaseDebit {
                        scope: debit_scope,
                        accepted: phase_accepted,
                    },
                ] = sequence
                else {
                    continue;
                };
                if *constructor_scope != scope
                    || *debit_scope != scope
                    || class != global_class
                    || *global_accepted != accepted
                    || *phase_accepted != accepted
                {
                    continue;
                }
                let constructor = StaticClassLiteral::from_id(*class);
                if constructor.name(db) != "FalseFactory"
                    || constructor.definition(db).file(db) != file
                {
                    continue;
                }
                let charge = NormalizerCharge {
                    phase,
                    class: *class,
                    expression,
                    callback: *callback,
                    fallback_exit,
                    callback_exit,
                    debit: entry + 1 + offset + 2,
                };
                assert!(
                    events[..charge.callback].iter().any(|event| {
                        matches!(event, Observation::Iterate(key) if charge.matches_key(db, *key))
                    }),
                    "normalizer query did not iterate before its fallback"
                );
                return Ok(charge);
            }
        }
        anyhow::bail!(
            "no matching FalseFactory debit (accepted={accepted}) in a previous-value normalizer fallback after iteration {}",
            crate::TAINTED_CYCLES
        )
    }

    // A class-valued __bool__ reaches constructor work while a sparse comparison override is
    // normalized. Refusal must stop that callback and discard its provisional result, so a fresh
    // attempt can complete the same source cycle and a later attempt can reuse it.
    #[test]
    fn real_expansion_normalizer_constructor_control() -> anyhow::Result<()> {
        const SOURCE: &str = "\
from typing import Literal, overload

class FalseFactory:
    def __new__(cls) -> Literal[False]:
        return False

class FalseResult:
    __bool__ = FalseFactory

class MaybeResult:
    def __bool__(self) -> bool:
        raise NotImplementedError

class S0:
    next: \"S1\"

class S1:
    next: \"S2\"

class S2:
    next: \"S3\"

class S3:
    next: \"S4\"

class S4:
    next: \"S5\"

class S5:
    next: \"S6\"

class S6:
    next: \"S6\"

class Left:
    @overload
    def __lt__(self, other: S0 | S1 | S2 | S3 | S4 | S5) -> Literal[True]: ...
    @overload
    def __lt__(self, other: object) -> MaybeResult | Literal[True]: ...
    def __lt__(self, other: object) -> MaybeResult | Literal[True]:
        raise NotImplementedError

class Right:
    def __gt__(self, other: object) -> FalseResult:
        raise NotImplementedError

def check(flag: bool, left: Left, right: Right, initial: S0) -> None:
    state = initial
    while flag:
        if left < state < right:
            break
        state = state.next
";
        let mut control_db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        control_db.write_file("/src/normalizer.py", SOURCE)?;
        let control_file = system_path_to_file(&control_db, "/src/normalizer.py")?;
        let (control, control_statistics) = run_inner(&control_db, 100_000, true, || {
            control_db.check_file(control_file)
        });
        let control_diagnostics = control
            .map_err(|reason| anyhow::anyhow!("normalizer control incomplete: {reason:?}"))?;
        assert_expected(SOURCE, &control_diagnostics);
        assert_normalizer_drained(&control_statistics);
        let control_events = observations(&control_statistics);
        let control_charge = normalizer_charge(&control_db, control_file, control_events, true)?;
        control_charge.assert_completed(&control_db, control_events);
        let allowance = control_charge.preceding_debits(control_events);

        // Count only accepted global charges, then independently identify the refused charge in
        // a cold database. An allowance alone cannot establish where inference was interrupted.
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/normalizer.py", SOURCE)?;
        let file = system_path_to_file(&db, "/src/normalizer.py")?;
        let stamp = Stamp::current(&db);
        let (limited, limited_statistics) = run_inner(&db, allowance, true, || {
            let _ = db.check_file(file);
            observe(Observation::RepeatedDemand);
            db.check_file(file)
        });
        assert_eq!(limited.err(), Some(Incomplete::Allowance));
        assert_normalizer_drained(&limited_statistics);
        assert_eq!(Stamp::current(&db), stamp);
        let limited_events = observations(&limited_statistics);
        let refused_charge = normalizer_charge(&db, file, limited_events, false)?;
        assert_eq!(refused_charge.preceding_debits(limited_events), allowance);
        assert_eq!(
            refused_charge.phase.query_name,
            control_charge.phase.query_name
        );
        assert_eq!(
            refused_charge.phase.iteration,
            control_charge.phase.iteration
        );
        assert_eq!(refused_charge.expression, control_charge.expression);
        let refusal = refused_charge.debit + 2;
        assert!(matches!(
            limited_events.get(refusal),
            Some(Observation::Refusal)
        ));
        assert!(refusal < refused_charge.fallback_exit);
        assert!(
            !limited_events[refusal + 1..refused_charge.callback_exit]
                .iter()
                .any(|event| {
                    matches!(event, Observation::PhaseEntered { phase, .. }
                if matches!(phase.kind, NormalizationPhaseKind::AbsentOverride { .. }
                    | NormalizationPhaseKind::TypeFields))
                }),
            "interrupted callback continued fallback or type-field normalization"
        );
        for event in &limited_events[refusal + 1..] {
            assert!(
                !matches!(event, Observation::PhaseEntered { phase, .. }
                if refused_charge.matches_query(*phase)),
                "incomplete normalizer resumed: {event:?}"
            );
            assert!(
                !matches!(event, Observation::Finalize(key)
                if refused_charge.matches_key(&db, *key)),
                "incomplete normalizer finalized"
            );
            assert!(
                !matches!(event, Observation::Debit { accepted: true }),
                "constructor work continued after refusal"
            );
        }
        let repeated = limited_events
            .iter()
            .position(|event| matches!(event, Observation::RepeatedDemand))
            .ok_or_else(|| anyhow::anyhow!("missing repeated-demand observation"))?;
        assert!(repeated > refused_charge.callback_exit);
        assert!(
            !limited_events[repeated + 1..]
                .iter()
                .any(|event| { matches!(event, Observation::Debit { accepted: true }) }),
            "repeated demand renewed constructor work"
        );

        let (retry, retry_statistics) = run_inner(&db, 100_000, true, || db.check_file(file));
        let retry_diagnostics =
            retry.map_err(|reason| anyhow::anyhow!("normalizer retry incomplete: {reason:?}"))?;
        assert_expected(SOURCE, &retry_diagnostics);
        assert_eq!(retry_diagnostics.len(), control_diagnostics.len());
        assert_normalizer_drained(&retry_statistics);
        assert_eq!(Stamp::current(&db), stamp);
        let retry_events = observations(&retry_statistics);
        let retry_charge = normalizer_charge(&db, file, retry_events, true)?;
        assert!(refused_charge.matches_query(retry_charge.phase));
        assert_eq!(retry_charge.class, refused_charge.class);
        assert_eq!(retry_charge.expression, refused_charge.expression);
        retry_charge.assert_completed(&db, retry_events);
        assert!(
            !retry_events
                .iter()
                .any(|event| matches!(event, Observation::Refusal))
        );
        assert!(
            retry_events.iter().any(|event| {
                matches!(event, Observation::Execute(key) if refused_charge.matches_key(&db, *key))
            }),
            "retry reused the incomplete inference result"
        );

        let (warm, warm_statistics) = run_inner(&db, 100_000, true, || db.check_file(file));
        let warm_diagnostics =
            warm.map_err(|reason| anyhow::anyhow!("warm normalizer retry incomplete: {reason:?}"))?;
        assert_expected(SOURCE, &warm_diagnostics);
        assert_eq!(warm_diagnostics.len(), control_diagnostics.len());
        assert_normalizer_drained(&warm_statistics);
        assert_eq!(Stamp::current(&db), stamp);
        assert!(warm_statistics.expansions < retry_statistics.expansions);
        for event in observations(&warm_statistics) {
            assert!(
                !matches!(event, Observation::Execute(key)
                | Observation::Iterate(key) | Observation::Finalize(key)
                if refused_charge.matches_key(&db, *key)),
                "warm retry did not reuse the completed query: {event:?}"
            );
            assert!(
                !matches!(event, Observation::PhaseEntered { phase, .. }
                if refused_charge.matches_query(*phase)),
                "warm retry repeated normalization: {event:?}"
            );
            assert!(!matches!(event, Observation::Refusal));
        }
        eprintln!(
            "normalizer refusal: query={}, iteration={}, allowance={allowance}, expansions control/retry/warm={}/{}/{}",
            refused_charge.phase.query_name,
            refused_charge.phase.iteration,
            control_statistics.expansions,
            retry_statistics.expansions,
            warm_statistics.expansions,
        );
        Ok(())
    }

    fn source_cycle(
        db: &TestDb,
        file: File,
        observations: &[Observation],
        key: salsa::DatabaseKeyIndex,
    ) -> bool {
        db.ingredient_debug_name(key.ingredient_index()) == "infer_definition_types"
            && observations.iter().any(|observation| {
                let Observation::DefinitionSeed {
                    query,
                    definition,
                    canonical: true,
                } = *observation
                else {
                    return false;
                };
                let definition = Definition::from_id(definition);
                query == key.key_index()
                    && definition.file(db) == file
                    && matches!(
                        definition.kind(db),
                        DefinitionKind::Assignment(_) | DefinitionKind::LoopHeader(_)
                    )
            })
    }

    fn constructor_charge_in_cycle(
        db: &TestDb,
        file: File,
        observations: &[Observation],
        accepted: bool,
    ) -> anyhow::Result<(salsa::DatabaseKeyIndex, usize)> {
        for (iteration_index, observation) in observations.iter().enumerate() {
            let Observation::Iterate(key) = *observation else {
                continue;
            };
            if !source_cycle(db, file, observations, key) {
                continue;
            }
            for (offset, observation) in observations[iteration_index + 1..].iter().enumerate() {
                if matches!(observation, Observation::Finalize(finalized) if *finalized == key) {
                    break;
                }
                let Observation::Constructor(class) = *observation else {
                    continue;
                };
                let class = StaticClassLiteral::from_id(class);
                if class.name(db) != "Box" || class.definition(db).file(db) != file {
                    continue;
                }
                let debit_index = iteration_index + offset + 2;
                if matches!(observations.get(debit_index), Some(Observation::Debit { accepted: actual }) if *actual == accepted)
                {
                    let preceding_debits = observations[..debit_index]
                        .iter()
                        .filter(|event| matches!(event, Observation::Debit { accepted: true }))
                        .count();
                    return Ok((key, preceding_debits));
                }
            }
        }
        anyhow::bail!("no Box constructor charge in a provisional source cycle: {observations:#?}")
    }

    fn assert_productive_cycle(
        observations: &[Observation],
        key: salsa::DatabaseKeyIndex,
    ) -> anyhow::Result<()> {
        let query_id = key.key_index();
        let seed = observations
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Observation::DefinitionSeed { query, canonical: true, .. } if *query == query_id
                )
            })
            .ok_or_else(|| {
                anyhow::anyhow!("cycle did not use its canonical seed: {observations:#?}")
            })?;
        let progress = observations[seed + 1..].iter().position(|event| matches!(
            event,
            Observation::DefinitionNormalized { query, binding_changed: true, .. } if *query == query_id
        )).map(|offset| seed + 1 + offset)
            .ok_or_else(|| anyhow::anyhow!("source binding did not change: {observations:#?}"))?;
        let stable = observations[progress + 1..].iter().position(|event| matches!(
            event,
            Observation::DefinitionNormalized { query, binding_changed: false, result_changed: false } if *query == query_id
        )).map(|offset| progress + 1 + offset)
            .ok_or_else(|| anyhow::anyhow!("source result did not stabilize: {observations:#?}"))?;
        assert!(
            observations[stable + 1..].iter().any(|event| {
                matches!(event, Observation::Finalize(finalized) if *finalized == key)
            }),
            "stable source cycle did not finalize: {observations:#?}"
        );
        Ok(())
    }

    // The loop's constructor argument reads its own loop-back binding. Refusing a constructor
    // charge after this source cycle has iterated must not turn recovery bindings into a fixed
    // point, and an unchanged-database retry must start with the ordinary divergent seed.
    #[test]
    fn real_expansion_productive_source_cycle_retries() -> anyhow::Result<()> {
        const SOURCE: &str = "\
from typing import assert_type

class Box:
    def __init__(self, previous: object, marker: int) -> None: ...

def build(flag: bool) -> None:
    value = None
    while flag:
        value = Box(value, \"wrong\")  # error: [invalid-argument-type]
    assert_type(value, Box | None)
";

        let mut control_db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        control_db.write_file("/src/cycle.py", SOURCE)?;
        let control_file = system_path_to_file(&control_db, "/src/cycle.py")?;
        let (control, control_statistics) = run_inner(&control_db, 100_000, true, || {
            control_db.check_file(control_file)
        });
        assert_expected(
            SOURCE,
            &control.map_err(|reason| anyhow::anyhow!("control: {reason:?}"))?,
        );
        let control_events = observations(&control_statistics);
        let (control_cycle, allowance) =
            constructor_charge_in_cycle(&control_db, control_file, control_events, true)?;
        assert_productive_cycle(control_events, control_cycle)?;

        // Calibrate on a different database so the interrupted attempt remains cold. Verify its
        // actual event order again; the debit count alone is not evidence of cycle interruption.
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/cycle.py", SOURCE)?;
        let file = system_path_to_file(&db, "/src/cycle.py")?;
        let stamp = Stamp::current(&db);
        let (limited, limited_statistics) = run_inner(&db, allowance, true, || {
            let _ = db.check_file(file);
            observe(Observation::RepeatedDemand);
            db.check_file(file)
        });
        assert_eq!(limited.err(), Some(Incomplete::Allowance));
        assert_eq!(limited_statistics.active_drivers, 0);
        assert_eq!(Stamp::current(&db), stamp);
        assert!(!active());
        let limited_events = observations(&limited_statistics);
        let (cycle, spent) = constructor_charge_in_cycle(&db, file, limited_events, false)?;
        assert_eq!(spent, allowance);
        eprintln!(
            "productive source cycle: allowance={allowance}, control expansions={}, limited expansions={}",
            control_statistics.expansions, limited_statistics.expansions
        );
        let query_id = cycle.key_index();
        let refused = limited_events
            .iter()
            .position(|event| matches!(event, Observation::Refusal))
            .ok_or_else(|| anyhow::anyhow!("missing refusal observation"))?;
        assert!(limited_events[..refused].iter().any(|event| matches!(
            event,
            Observation::DefinitionNormalized { query, binding_changed: true, .. } if *query == query_id
        )), "interrupted source binding had not progressed: {limited_events:#?}");
        for event in &limited_events[refused + 1..] {
            assert!(
                !matches!(event,
                    Observation::DefinitionRecovery(query)
                        | Observation::DefinitionNormalization(query)
                        | Observation::DefinitionNormalized { query, .. }
                        | Observation::DefinitionSeed { query, .. } if *query == query_id
                ),
                "incomplete source cycle resumed: {event:?}"
            );
            assert!(
                !matches!(event, Observation::Finalize(key) if *key == cycle),
                "incomplete source cycle finalized"
            );
        }
        let repeated = limited_events
            .iter()
            .position(|event| matches!(event, Observation::RepeatedDemand))
            .ok_or_else(|| anyhow::anyhow!("missing repeated-demand observation"))?;
        assert!(repeated > refused);
        assert!(
            !limited_events[repeated + 1..]
                .iter()
                .any(|event| { matches!(event, Observation::Debit { accepted: true }) }),
            "repeated demand renewed constructor work"
        );

        let completed_source_keys: FxHashSet<_> = limited_events
            .iter()
            .filter_map(|event| {
                let Observation::Execute(key) = *event else {
                    return None;
                };
                matches!(
                    db.ingredient_debug_name(key.ingredient_index()).as_ref(),
                    "source_text" | "parsed_module" | "semantic_index"
                )
                .then_some(key)
            })
            .collect();
        for name in ["source_text", "parsed_module", "semantic_index"] {
            assert!(
                completed_source_keys
                    .iter()
                    .any(|key| { db.ingredient_debug_name(key.ingredient_index()) == name }),
                "cold attempt did not execute {name}"
            );
        }
        let mut first_retry_expansions = None;
        for retry in 1..=2 {
            let (outcome, statistics) = run_inner(&db, 100_000, true, || db.check_file(file));
            assert_expected(
                SOURCE,
                &outcome.map_err(|reason| {
                    anyhow::anyhow!("retry {retry}: {reason:?}; {statistics:?}")
                })?,
            );
            assert_eq!(statistics.active_drivers, 0);
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!active());
            let events = observations(&statistics);
            eprintln!(
                "productive source cycle retry {retry}: expansions={}",
                statistics.expansions
            );
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, Observation::Refusal))
            );
            for event in events {
                if let Observation::Execute(key) = event {
                    assert!(
                        !completed_source_keys.contains(key),
                        "completed source child reexecuted: {key:?}"
                    );
                }
            }
            if let Some(first_expansions) = first_retry_expansions {
                assert!(statistics.expansions < first_expansions);
                assert!(
                    !events.iter().any(|event| matches!(event,
                        Observation::DefinitionSeed { query, .. }
                            | Observation::DefinitionRecovery(query)
                            | Observation::DefinitionNormalization(query) if *query == query_id
                    )),
                    "warm attempt did not reuse the completed source cycle"
                );
            } else {
                assert!(source_cycle(&db, file, events, cycle));
                assert_productive_cycle(events, cycle)?;
                first_retry_expansions = Some(statistics.expansions);
            }
        }
        Ok(())
    }

    #[test]
    fn real_expansion_query_inventory() -> anyhow::Result<()> {
        let mut queries = BTreeSet::new();
        for syntax in [PEP695, LEGACY] {
            for heading in [
                "Constructor stopping types introduced by forwarding",
                "Constructor stopping types supplied through inherited aliases",
                "Constructor stopping types changed by later forwarding",
                "Alternative constructor paths with different stopping types",
            ] {
                let source = code(syntax, heading)?;
                let mut db = TestDbBuilder::new()
                    .with_python_version(PythonVersion::PY313)
                    .build()?;
                db.write_file("/src/forwarding.py", &source)?;
                let file = system_path_to_file(&db, "/src/forwarding.py")?;
                let _ = db.check_file(file);
                for event in db.take_salsa_events() {
                    if let salsa::EventKind::WillExecute { database_key } = event.kind {
                        queries.insert(
                            db.ingredient_debug_name(database_key.ingredient_index())
                                .into_owned(),
                        );
                    }
                }
            }
        }
        for query in queries {
            eprintln!("QUERY: {query}");
        }
        Ok(())
    }

    #[test]
    fn real_expansion_original_forwarding_cases() -> anyhow::Result<()> {
        let mut failures = Vec::new();
        for (syntax, mro_effects) in [PEP695, LEGACY]
            .into_iter()
            .flat_map(|syntax| [false, true].map(|mro_effects| (syntax, mro_effects)))
        {
            for heading in [
                "Constructor stopping types introduced by forwarding",
                "Constructor stopping types supplied through inherited aliases",
                "Constructor stopping types changed by later forwarding",
                "Alternative constructor paths with different stopping types",
            ] {
                let source = code(syntax, heading)?;
                let mut db = TestDbBuilder::new()
                    .with_python_version(PythonVersion::PY313)
                    .build()?;
                db.write_file("/src/forwarding.py", &source)?;
                let file = system_path_to_file(&db, "/src/forwarding.py")?;
                let (outcome, statistics) =
                    run_selected(&db, 100_000, false, mro_effects, || db.check_file(file));
                let diagnostics = match outcome {
                    Ok(diagnostics) => diagnostics,
                    Err(reason) => {
                        failures.push(format!(
                            "{heading}, mro_effects={mro_effects}: {reason:?}; {statistics:?}"
                        ));
                        continue;
                    }
                };
                eprintln!("{heading}: {statistics:?}");
                assert_expected(&source, &diagnostics);
                assert!(statistics.direct > 0 && statistics.conversions > 0);
            }
        }
        anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    }

    #[test]
    fn real_expansion_forwarding_charge_ledger() -> anyhow::Result<()> {
        for (syntax_name, syntax) in [("pep695", PEP695), ("legacy", LEGACY)] {
            for (case, heading) in [
                "Constructor stopping types introduced by forwarding",
                "Constructor stopping types supplied through inherited aliases",
                "Constructor stopping types changed by later forwarding",
                "Alternative constructor paths with different stopping types",
            ]
            .into_iter()
            .enumerate()
            {
                let source = code(syntax, heading)?;
                for mro_effects in [false, true] {
                    for allowance in [100_000, 200_000, 1_000_000] {
                        let mut control = None;
                        let mut basic_events = None;
                        let mut basic_frames = None;
                        for detailed in [None, Some(false), Some(true)] {
                            let mut db = TestDbBuilder::new()
                                .with_python_version(PythonVersion::PY313)
                                .build()?;
                            db.write_file("/src/forwarding.py", &source)?;
                            let file = system_path_to_file(&db, "/src/forwarding.py")?;
                            db.take_salsa_events();
                            let body = || {
                                run_selected(&db, allowance, false, mro_effects, || {
                                    db.check_file(file)
                                })
                            };
                            let (((outcome, statistics), events), frames) = match detailed {
                                None => ((body(), Vec::new()), Vec::new()),
                                Some(detailed) => frame_observation::capture(|| {
                                    charge_ledger::capture(detailed, body)
                                }),
                            };
                            let incomplete = outcome.as_ref().err().copied();
                            if let Ok(diagnostics) = &outcome {
                                assert_expected(&source, diagnostics);
                            }
                            let actual = outcome
                                .as_ref()
                                .map(|diagnostics| {
                                    diagnostics
                                        .iter()
                                        .map(|diagnostic| {
                                            (
                                                diagnostic.id().as_str().to_owned(),
                                                diagnostic.headline_message().to_owned(),
                                                diagnostic
                                                    .primary_span()
                                                    .and_then(|span| span.range()),
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .map_err(|reason| *reason);
                            let query_sequence: Vec<_> = db
                                .take_salsa_events()
                                .into_iter()
                                .filter_map(|event| {
                                    let (kind, key) = match event.kind {
                                        salsa::EventKind::WillExecute { database_key } => {
                                            ("execute", database_key)
                                        }
                                        salsa::EventKind::WillIterateCycle {
                                            database_key, ..
                                        } => ("iterate", database_key),
                                        salsa::EventKind::DidFinalizeCycle {
                                            database_key, ..
                                        } => ("finalize", database_key),
                                        _ => return None,
                                    };
                                    Some((
                                        kind,
                                        db.ingredient_debug_name(key.ingredient_index())
                                            .into_owned(),
                                    ))
                                })
                                .collect();
                            let actual = (
                                actual,
                                query_sequence,
                                statistics.debits,
                                statistics.expansions,
                                statistics.task_polls,
                            );
                            if let Some(control) = &control {
                                assert_eq!(
                                    &actual, control,
                                    "observation changed {syntax_name}/{case}/{mro_effects}/{allowance}"
                                );
                            } else {
                                control = Some(actual);
                            }
                            match detailed {
                                None => {}
                                Some(false) => {
                                    basic_events = Some(events);
                                    basic_frames = Some(frames);
                                }
                                Some(true) => {
                                    assert_eq!(
                                        Some(charge_ledger::without_work(&events)),
                                        basic_events
                                    );
                                    let name = format!(
                                        "{syntax_name}-case{case}-mro{mro_effects}-{allowance}"
                                    );
                                    charge_ledger::write_trace(
                                        &name, allowance, &events, incomplete,
                                    )?;
                                    assert_eq!(Some(&frames), basic_frames.as_ref());
                                    let allocated = frames
                                        .iter()
                                        .filter(|event| {
                                            matches!(
                                                event,
                                                frame_observation::Event::Allocated { .. }
                                            )
                                        })
                                        .count();
                                    let admitted = events.iter().filter(|event| matches!(event,
                                        charge_ledger::Event::Charge { work: Some(work), outcome: Ok(()), .. }
                                            if work.value == "\"MappingFrame\""
                                    )).count();
                                    assert_eq!(allocated, admitted);
                                    frame_observation::write_trace(&name, &frames)?;
                                }
                            }
                            assert_eq!(incomplete, None);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn real_expansion_forwarding_mapping_allowance_probe() -> anyhow::Result<()> {
        let mut failures = Vec::new();
        for (syntax_name, syntax) in [("pep695", PEP695), ("legacy", LEGACY)] {
            for heading in [
                "Constructor stopping types introduced by forwarding",
                "Constructor stopping types supplied through inherited aliases",
                "Constructor stopping types changed by later forwarding",
                "Alternative constructor paths with different stopping types",
            ] {
                let source = code(syntax, heading)?;
                for allowance in [200_000, 1_000_000] {
                    let mut db = TestDbBuilder::new()
                        .with_python_version(PythonVersion::PY313)
                        .build()?;
                    db.write_file("/src/forwarding.py", &source)?;
                    let file = system_path_to_file(&db, "/src/forwarding.py")?;
                    let (outcome, _) = run_mro(&db, allowance, || db.check_file(file));
                    match outcome {
                        Ok(diagnostics) => {
                            assert_expected(&source, &diagnostics);
                            break;
                        }
                        Err(reason) => {
                            if allowance == 1_000_000 {
                                failures.push(format!("{syntax_name}, {heading}: {reason:?}"));
                            }
                        }
                    }
                }
            }
        }
        anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    }

    #[test]
    fn real_expansion_allowance_and_warm_roots() -> anyhow::Result<()> {
        let source = code(
            PEP695,
            "Constructor stopping types changed by later forwarding",
        )?;
        for allowance in [0, 2, 20, 100] {
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", &source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let (outcome, statistics) = run(&db, allowance, || db.check_file(file));
            assert_eq!(outcome.err(), Some(Incomplete::Allowance));
            assert_eq!(statistics.active_drivers, 0);
            assert!(!active());
        }
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/forwarding.py", &source)?;
        let file = system_path_to_file(&db, "/src/forwarding.py")?;
        let (first, first_statistics) = run(&db, 100_000, || db.check_file(file));
        let first = first.map_err(|reason| anyhow::anyhow!("{reason:?}"))?;
        assert_expected(&source, &first);
        let (warm, warm_statistics) = run(&db, 100_000, || db.check_file(file));
        assert_expected(
            &source,
            &warm.map_err(|reason| anyhow::anyhow!("{reason:?}"))?,
        );
        assert!(warm_statistics.expansions < first_statistics.expansions);
        let (nested, _) = run(&db, 10, || run(&db, 10, || ()).0);
        assert_eq!(nested, Ok(Err(Incomplete::NestedRoot)));
        Ok(())
    }

    #[test]
    fn real_expansion_same_database_retry_after_incomplete() -> anyhow::Result<()> {
        for (syntax_name, syntax) in [("pep695", PEP695), ("legacy", LEGACY)] {
            let source = code(
                syntax,
                "Constructor stopping types changed by later forwarding",
            )?;
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", &source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let stamp = Stamp::current(&db);
            db.clear_salsa_events();
            let (limited, limited_statistics) = run(&db, 100, || db.check_file(file));
            eprintln!("{syntax_name}: limited={limited:?}; {limited_statistics:?}");
            assert_eq!(limited.err(), Some(Incomplete::Allowance));
            assert_eq!(limited_statistics.active_drivers, 0);
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!active());

            let completed_source_keys: FxHashSet<_> = db
                .take_salsa_events()
                .into_iter()
                .filter_map(|event| {
                    let salsa::EventKind::WillExecute { database_key } = event.kind else {
                        return None;
                    };
                    matches!(
                        db.ingredient_debug_name(database_key.ingredient_index())
                            .as_ref(),
                        "source_text" | "parsed_module" | "semantic_index"
                    )
                    .then_some(database_key)
                })
                .collect();
            for query in ["source_text", "parsed_module", "semantic_index"] {
                assert!(
                    completed_source_keys
                        .iter()
                        .any(|key| db.ingredient_debug_name(key.ingredient_index()) == query),
                    "cold attempt did not execute {query}"
                );
            }

            let mut first_retry_expansions = None;
            for retry in 1..=2 {
                let (outcome, statistics) = run(&db, 100_000, || db.check_file(file));
                let diagnostics = outcome.map_err(|reason| {
                    anyhow::anyhow!("{syntax_name}: retry {retry}: {reason:?}; {statistics:?}")
                })?;
                assert_expected(&source, &diagnostics);
                assert_eq!(statistics.active_drivers, 0);
                assert_eq!(Stamp::current(&db), stamp);
                assert!(!active());
                for event in db.take_salsa_events() {
                    if let salsa::EventKind::WillExecute { database_key } = event.kind {
                        assert!(
                            !completed_source_keys.contains(&database_key),
                            "completed source query was reexecuted: {database_key:?}"
                        );
                    }
                }
                if let Some(first) = first_retry_expansions {
                    assert!(statistics.expansions < first);
                } else {
                    assert!(statistics.expansions > 0);
                    first_retry_expansions = Some(statistics.expansions);
                }
            }

            let mut fresh_db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            fresh_db.write_file("/src/forwarding.py", &source)?;
            let fresh_file = system_path_to_file(&fresh_db, "/src/forwarding.py")?;
            let (fresh, fresh_statistics) =
                run(&fresh_db, 100_000, || fresh_db.check_file(fresh_file));
            let fresh =
                fresh.map_err(|reason| anyhow::anyhow!("{syntax_name}: fresh: {reason:?}"))?;
            eprintln!("{syntax_name}: fresh={fresh_statistics:?}");
            assert_expected(&source, &fresh);
            assert_eq!(fresh_statistics.active_drivers, 0);
            assert!(!active());
        }
        Ok(())
    }

    #[test]
    fn real_expansion_same_database_edit_after_incomplete() -> anyhow::Result<()> {
        for (syntax_name, syntax) in [("pep695", PEP695), ("legacy", LEGACY)] {
            let source = code(
                syntax,
                "Constructor stopping types changed by later forwarding",
            )?;
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", &source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let (limited, limited_statistics) = run(&db, 100, || db.check_file(file));
            assert_eq!(limited.err(), Some(Incomplete::Allowance));
            assert_eq!(limited_statistics.active_drivers, 0);
            assert!(!active());

            // A relevant edit after incomplete evaluation must use the new stopping type.
            // The longer stopping type still preserves End's argument requirements.
            let edited = source.replace(
                "list[list[list[list[V]]]]",
                "list[list[list[list[list[V]]]]]",
            );
            assert_ne!(edited, source);
            db.write_file("/src/forwarding.py", &edited)?;
            let (edited_retry, edited_statistics) = run(&db, 100_000, || db.check_file(file));
            let edited_retry = edited_retry.map_err(|reason| {
                anyhow::anyhow!("{syntax_name}: edit incomplete: {reason:?}; {edited_statistics:?}")
            })?;
            eprintln!("{syntax_name}: edited_retry={edited_statistics:?}");
            assert_expected(&edited, &edited_retry);
            assert_eq!(edited_statistics.active_drivers, 0);
            assert!(!active());

            let (warm, warm_statistics) = run(&db, 100_000, || db.check_file(file));
            let warm = warm.map_err(|reason| anyhow::anyhow!("{syntax_name}: warm: {reason:?}"))?;
            eprintln!("{syntax_name}: edited_warm={warm_statistics:?}");
            assert_expected(&edited, &warm);
            assert!(warm_statistics.expansions < edited_statistics.expansions);
            assert_eq!(warm_statistics.active_drivers, 0);
            assert!(!active());

            let mut fresh_db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            fresh_db.write_file("/src/forwarding.py", &edited)?;
            let fresh_file = system_path_to_file(&fresh_db, "/src/forwarding.py")?;
            let (fresh, fresh_statistics) =
                run(&fresh_db, 100_000, || fresh_db.check_file(fresh_file));
            let fresh =
                fresh.map_err(|reason| anyhow::anyhow!("{syntax_name}: fresh: {reason:?}"))?;
            eprintln!("{syntax_name}: edited_fresh={fresh_statistics:?}");
            assert_expected(&edited, &fresh);
            assert_eq!(fresh_statistics.active_drivers, 0);
            assert!(!active());
        }
        Ok(())
    }

    #[test]
    fn real_expansion_longer_finite_chains() -> anyhow::Result<()> {
        let original = code(
            PEP695,
            "Constructor stopping types changed by later forwarding",
        )?;
        for depth in [4, 8, 16, 32] {
            let stopping = format!("{}V{}", "list[".repeat(depth), "]".repeat(depth));
            let source = original.replace("list[list[list[list[V]]]]", &stopping);
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", &source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let start = std::time::Instant::now();
            let (outcome, statistics) = run(&db, 1_000_000, || db.check_file(file));
            eprintln!("depth={depth} elapsed={:?} {statistics:?}", start.elapsed());
            assert_expected(
                &source,
                &outcome.map_err(|reason| anyhow::anyhow!("{reason:?}"))?,
            );
            assert_eq!(statistics.active_drivers, 0);
            assert!(statistics.max_drivers <= 3, "{statistics:?}");
        }
        Ok(())
    }
}
