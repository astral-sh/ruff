//! Runs the shared satisfaction traversal with the caller's builder and canonical sequent routes.

use std::borrow::Cow;
use std::future::Future;
use std::ops::{ControlFlow, Range};

use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use smallvec::SmallVec;

use super::EndpointAdmission;
use crate::types::constraints::apply::{Operation, TddApply};
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{
    AllocationKind, PathReserve, PathTypevarSet, PathWork, TableKind, TddControl, TddError,
    TddWork, admit_path_work, reserve_map, reserve_smallvec,
};
use crate::types::constraints::paths::{
    PathAssignments, PathEffects, PathTrace, PathVisitEffects, PathVisitFrame,
    reserve_path_frames_with, reserve_path_replay_with, reserve_path_typevars_with,
    reserve_path_with,
};
use crate::types::constraints::satisfaction::{
    SatisfactionEffects, SatisfactionKind, node_satisfaction_with,
};
use crate::types::constraints::sequents::SequentMap;
use crate::types::constraints::sequents::runtime::{SequentQueries, pair_cannot_produce};
use crate::types::constraints::support::Support;
use crate::types::constraints::type_analysis::{
    BoundSearch, ConstraintDepthCacheEffects, ConstraintTypeEffects, ConstraintTypeWork,
    cached_constraint_bound_depth_with, constraint_as_concrete_with,
};
use crate::types::constraints::variables::{Constraint, TypeVarEquivalenceBound};
use crate::types::constraints::{
    ConstraintId, ConstraintSet, ConstraintSetBuilder, InteriorNode, InteriorNodeData,
    IsNeverSatisfiedVisitor, NodeId, OwnedConstraintSet, PathVisitor, SingleConjunctionScan,
    SourceOrderId, SourceOrderScan, TypeVarId, UniqueConstraintScan, extend_dependent_support_with,
    independent_pair_skip_with,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mapping::runtime::{MaterializationConfiguration, MaterializationQueries};
use crate::types::protocol_class::ProtocolInterfaceView;
use crate::types::relation::runtime::constraint_set::{
    self, ConstraintSetObservation, NoOwnedRelationQueries, OwnedRelationQueryAccess,
};
use crate::types::relation::runtime::protocol::{NoProtocolQueries, ProtocolQueryAccess};
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallRelationOwners,
};
use crate::types::{
    BoundTypeVarInstance, MaterializationKind, MemberLookupPolicy, MemberLookupResult,
    ProtocolInstanceType, Type,
};

use crate::{Db, FxIndexSet, Program};

mod tests;
mod type_search;

/// Query routes used by the original sequent and satisfaction reducers.
pub(in crate::types::constraints) trait ConstraintQueryAccess<'run, 'db: 'run>:
    Clone + 'run
{
    const CONCRETE_AVAILABLE: bool = false;

    fn single<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        constraint: Constraint<'db>,
    ) -> impl Future<Output = RunResult<&'db SequentMap<'db>>> + 'call
    where
        'run: 'call;

    fn pair<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> impl Future<Output = RunResult<&'db SequentMap<'db>>> + 'call
    where
        'run: 'call;

    fn materialize<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _program: Program<'db>,
        _ty: Type<'db>,
        _kind: MaterializationKind,
    ) -> impl Future<Output = RunResult<Type<'db>>> + 'call
    where
        'run: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(RunError::Contract(
                        "materialization capability is unavailable",
                    ))
                })
                .await)
        }
    }

    fn assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _program: Program<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(RunError::Contract(
                        "scalar assignability capability is unavailable",
                    ))
                })
                .await)
        }
    }

    fn equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _program: Program<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(RunError::Contract(
                        "scalar equivalence capability is unavailable",
                    ))
                })
                .await)
        }
    }

    fn owned_assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _program: Program<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = RunResult<Cow<'db, OwnedConstraintSet<'db>>>> + 'call
    where
        'run: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(RunError::Contract(
                        "owned assignability capability is unavailable",
                    ))
                })
                .await)
        }
    }

    fn owned_equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _program: Program<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = RunResult<Cow<'db, OwnedConstraintSet<'db>>>> + 'call
    where
        'run: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(RunError::Contract(
                        "owned equivalence capability is unavailable",
                    ))
                })
                .await)
        }
    }
}

pub(in crate::types::constraints) async fn static_eligible<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
) -> RunResult<bool> {
    type_search::static_eligible(db, endpoint, ty).await
}

/// Providers borrow these pools and route tokens until the registered run has drained.
pub(in crate::types::constraints) struct ConcreteSolverQueries<
    'run,
    'db: 'run,
    S,
    P,
    C,
    Q = NoProtocolQueries,
    O = NoOwnedRelationQueries,
> where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    C: MaterializationConfiguration,
    Q: ProtocolQueryAccess<'run, 'db>,
    O: OwnedRelationQueryAccess<'run, 'db>,
{
    pub(in crate::types::constraints) sequents: SequentQueries<'run, 'db, S, P>,
    pub(in crate::types::constraints) materialization: &'run MaterializationQueries<'run, 'db, C>,
    pub(in crate::types::constraints) environments: &'run CallEnvironments<'db>,
    pub(in crate::types::constraints) builders: &'run CallBuilders<'db>,
    pub(in crate::types::constraints) owners: &'run CallRelationOwners<'run, 'run, 'db>,
    pub(in crate::types::constraints) protocols: Q,
    pub(in crate::types::constraints) owned_relations: O,
    pub(in crate::types::constraints) relation_observer:
        Option<&'run dyn Fn(ConstraintSetObservation<'_, 'db>)>,
}

impl<'run, 'db: 'run, S, P, C, Q, O> Clone for ConcreteSolverQueries<'run, 'db, S, P, C, Q, O>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    C: MaterializationConfiguration,
    Q: ProtocolQueryAccess<'run, 'db>,
    O: OwnedRelationQueryAccess<'run, 'db>,
{
    fn clone(&self) -> Self {
        Self {
            sequents: self.sequents.clone(),
            materialization: self.materialization,
            environments: self.environments,
            builders: self.builders,
            owners: self.owners,
            protocols: self.protocols.clone(),
            owned_relations: self.owned_relations.clone(),
            relation_observer: self.relation_observer,
        }
    }
}

impl<'run, 'db: 'run, S, P, C, Q, O> ConstraintQueryAccess<'run, 'db>
    for ConcreteSolverQueries<'run, 'db, S, P, C, Q, O>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    C: MaterializationConfiguration,
    Q: ProtocolQueryAccess<'run, 'db>,
    O: OwnedRelationQueryAccess<'run, 'db>,
{
    const CONCRETE_AVAILABLE: bool = true;

    async fn single<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        constraint: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>>
    where
        'run: 'call,
    {
        self.sequents.single(endpoint, program, constraint).await
    }

    async fn pair<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>>
    where
        'run: 'call,
    {
        self.sequents.pair(endpoint, program, left, right).await
    }

    async fn materialize<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        self.materialization
            .materialization(endpoint, db, ty, program, kind)
            .await
    }

    async fn assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        constraint_set::assignable_observed(
            db,
            endpoint,
            program,
            left,
            right,
            self.environments,
            self.builders,
            self.owners,
            self.clone(),
            self.relation_observer,
        )
        .await
    }

    async fn owned_assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>>
    where
        'run: 'call,
    {
        constraint_set::owned_assignable_with_queries_observed(
            db,
            endpoint,
            program,
            left,
            right,
            self.owned_relations.clone(),
            self.relation_observer,
        )
        .await
    }

    async fn equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        constraint_set::equivalent_observed(
            db,
            endpoint,
            program,
            left,
            right,
            self.environments,
            self.builders,
            self.owners,
            self.clone(),
            self.owned_relations.clone(),
            self.relation_observer,
        )
        .await
    }

    async fn owned_equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>>
    where
        'run: 'call,
    {
        constraint_set::owned_equivalent_observed(
            db,
            endpoint,
            program,
            left,
            right,
            self.owned_relations.clone(),
            self.relation_observer,
        )
        .await
    }
}

impl<'run, 'db: 'run, S, P, C, Q, O> ProtocolQueryAccess<'run, 'db>
    for ConcreteSolverQueries<'run, 'db, S, P, C, Q, O>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    C: MaterializationConfiguration,
    Q: ProtocolQueryAccess<'run, 'db>,
    O: OwnedRelationQueryAccess<'run, 'db>,
{
    const AVAILABLE: bool = Q::AVAILABLE;
    const MEMBERS_AVAILABLE: bool = Q::MEMBERS_AVAILABLE;
    const SOLVER_AVAILABLE: bool = true;

    async fn interface<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>>
    where
        'run: 'call,
    {
        self.protocols.interface(endpoint, protocol).await
    }

    async fn member_lookup<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        self.protocols
            .member_lookup(endpoint, program, ty, name, policy)
            .await
    }

    async fn object_equivalence<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        self.protocols.object_equivalence(endpoint, protocol).await
    }

    async fn constraint_satisfaction<'call, 'c>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        constraints: ConstraintSet<'db, 'c>,
        always: bool,
    ) -> RunResult<bool>
    where
        'run: 'call,
        'c: 'call,
    {
        let kind = if always {
            SatisfactionKind::Always
        } else {
            SatisfactionKind::Never
        };
        satisfy(db, endpoint, program, constraints, kind, self).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedSatisfactionOperation {
    CompactedBuilder,
    RichTracing,
    GroupedImport,
    BoundMaterialization,
    BoundDepth,
}

/// The observations borrow actual solver state; they cannot supply semantic answers.
enum Observation<'a, 'db> {
    Path(PathTrace, &'a PathAssignments),
    Single(Constraint<'db>, &'db SequentMap<'db>),
    Pair(Constraint<'db>, Constraint<'db>, &'db SequentMap<'db>),
    Imported(Constraint<'db>, ConstraintId),
    Or(NodeId, NodeId, NodeId),
    BeforeNeverCacheInsert(NodeId),
    BeforeConstraintImport(Constraint<'db>),
    BeforeDepth(Type<'db>),
    AfterDepth(Type<'db>, (u16, u16)),
    BeforeDepthCacheInsert(ConstraintId, (u16, u16)),
    BoundSearch(Type<'db>, BoundSearch, bool),
}

fn runtime_incomplete(db: &dyn Db, reason: Incomplete) -> RunError {
    let reason = expansion_probe::refuse(db, reason);
    RunError::Refused(match reason {
        Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
        Incomplete::RequestedAllocation => salsa::attempt_probe::Incomplete::RequestedAllocation,
        _ => salsa::attempt_probe::Incomplete::Interrupted,
    })
}

fn runtime_error(db: &dyn Db, error: TddError<RunError>) -> RunError {
    match error {
        TddError::Refused(error) => error,
        TddError::CapacityExhausted => {
            runtime_incomplete(db, Incomplete::ConstraintCapacityExhausted)
        }
    }
}

pub(in crate::types::constraints) async fn satisfy<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    constraints: ConstraintSet<'db, 'c>,
    kind: SatisfactionKind,
    queries: &'call Q,
) -> RunResult<bool>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    satisfy_observed(db, endpoint, program, constraints, kind, queries, None).await
}

async fn satisfy_observed<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    constraints: ConstraintSet<'db, 'c>,
    kind: SatisfactionKind,
    queries: &'call Q,
    observer: Option<&'call dyn Fn(Observation<'_, 'db>)>,
) -> RunResult<bool>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    let mut effects = RuntimeSatisfaction {
        db,
        endpoint,
        program,
        builder: constraints.builder,
        fields: salsa::FieldReads::new(db),
        queries,
        observer,
    };
    endpoint.local_call(|| {
        endpoint.admit_work(1)?;
        if effects.builder.storage.borrow().compacted.is_some() {
            return Err(effects.unsupported(UnsupportedSatisfactionOperation::CompactedBuilder));
        }
        if tracing::enabled!(target: "ty_python_semantic::types::constraints::PathAssignment", tracing::Level::TRACE)
            || tracing::enabled!(target: "ty_python_semantic::types::constraints::SequentMap", tracing::Level::TRACE)
        {
            return Err(effects.unsupported(UnsupportedSatisfactionOperation::RichTracing));
        }
        Ok(())
    }).await;
    let result = node_satisfaction_with(
        constraints.node,
        constraints.source_order,
        kind,
        &mut effects,
    )
    .await?;
    endpoint.local_call(|| endpoint.admit_work(1)).await;
    Ok(result)
}

struct RuntimeSatisfaction<'call, 'run, 'db: 'run, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    builder: &'c ConstraintSetBuilder<'db>,
    fields: salsa::FieldReads<'db>,
    queries: &'call Q,
    observer: Option<&'call dyn Fn(Observation<'_, 'db>)>,
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    fn incomplete(&self, reason: Incomplete) -> RunError {
        runtime_incomplete(self.db, reason)
    }
    fn error(&self, error: TddError<RunError>) -> RunError {
        runtime_error(self.db, error)
    }
    fn unsupported(&self, operation: UnsupportedSatisfactionOperation) -> RunError {
        self.incomplete(Incomplete::UnsupportedSatisfactionOperation(operation))
    }
    async fn refuse<T>(&self, operation: UnsupportedSatisfactionOperation) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Err(self.unsupported(operation))
            })
            .await)
    }
    fn observe(&self, observation: Observation<'_, 'db>) {
        if let Some(observer) = self.observer {
            observer(observation);
        }
    }
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> PathEffects<'db>
    for RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    type Error = RunError;

    async fn checkpoint(&mut self, work: PathWork) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                admit_path_work(
                    work,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
    async fn interior_data(&mut self, node: NodeId) -> RunResult<InteriorNodeData> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(self.builder.storage.borrow().interior_node_data(node))
            })
            .await)
    }
    async fn constraint_data(&mut self, id: ConstraintId) -> RunResult<Constraint<'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(self.builder.storage.borrow().constraint_data(id))
            })
            .await)
    }
    async fn single_sequents(
        &mut self,
        constraint: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>> {
        let map = self
            .queries
            .single(self.endpoint, self.program, constraint)
            .await?;
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.observe(Observation::Single(constraint, map));
                Ok(())
            })
            .await;
        Ok(map)
    }
    async fn pair_sequents(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>> {
        let map = self
            .queries
            .pair(self.endpoint, self.program, left, right)
            .await?;
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.observe(Observation::Pair(left, right, map));
                Ok(())
            })
            .await;
        Ok(map)
    }
    async fn pair_cannot_produce(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> RunResult<bool> {
        pair_cannot_produce(
            self.db,
            self.endpoint,
            self.program,
            left,
            right,
            self.queries,
        )
        .await
    }
    async fn independent_pair_skip(
        &mut self,
        existing: ConstraintId,
        current: ConstraintId,
        independent: &FxHashSet<TypeVarId>,
    ) -> RunResult<bool> {
        Ok(self
            .endpoint
            .local_call(|| {
                independent_pair_skip_with(
                    &self.builder.storage.borrow(),
                    existing,
                    current,
                    independent,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await)
    }
    async fn group_imports_left_first(
        &mut self,
        equivalence: TypeVarEquivalenceBound<'db>,
    ) -> RunResult<bool> {
        let _ = equivalence;
        self.refuse(UnsupportedSatisfactionOperation::GroupedImport)
            .await
    }
    async fn intern_constraint(&mut self, constraint: Constraint<'db>) -> RunResult<ConstraintId> {
        let mut support = Support::default();
        if self.observer.is_some() {
            self.endpoint
                .local_call(|| {
                    self.observe(Observation::BeforeConstraintImport(constraint));
                    Ok(())
                })
                .await;
        }
        let id = type_search::intern_constraint(
            self.db,
            self.endpoint,
            self.builder,
            constraint,
            &mut support,
        )
        .await?;
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                // A cache hit leaves this temporary support with the import future.
                drop(std::mem::take(&mut support));
                self.observe(Observation::Imported(constraint, id));
                Ok(())
            })
            .await;
        Ok(id)
    }
    async fn constraint_depth(&mut self, id: ConstraintId) -> RunResult<(u16, u16)> {
        cached_constraint_bound_depth_with(id, self).await
    }
    async fn reflexive_constraint(&mut self, constraint: Constraint<'db>) -> RunResult<bool> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(constraint.is_reflexive_typevar_relation_with_fields(self.fields))
            })
            .await)
    }
    async fn trace_path(&mut self, event: PathTrace, path: &PathAssignments) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.observe(Observation::Path(event, path));
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn reserve_path(
        &mut self,
        path: &mut PathAssignments,
        request: PathReserve,
    ) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                reserve_path_with(
                    path,
                    request,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
    async fn reserve_replay(&mut self, ids: &mut Vec<ConstraintId>) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                reserve_path_replay_with(
                    ids,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> SatisfactionEffects<'db>
    for RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    async fn never_cache_get(&mut self, node: NodeId) -> RunResult<Option<bool>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(self
                    .builder
                    .storage
                    .borrow()
                    .never_satisfied_cache
                    .get(&node)
                    .copied())
            })
            .await)
    }
    async fn never_cache_insert(&mut self, node: NodeId, value: bool) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.observe(Observation::BeforeNeverCacheInsert(node));
                self.endpoint.admit_work(1)?;
                let mut storage = self.builder.storage.borrow_mut();
                let admission = EndpointAdmission(self.endpoint);
                let mut control = ExecutionControl::new(&admission);
                if !storage.never_satisfied_cache.contains_key(&node) {
                    reserve_map(
                        &mut storage.never_satisfied_cache,
                        TableKind::NeverCache,
                        &mut control,
                    )
                    .map_err(|error| self.error(error))?;
                }
                control.admit(TddWork::Commit)?;
                storage.never_satisfied_cache.insert(node, value);
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn collect_unique_constraints(
        &mut self,
        node: NodeId,
    ) -> RunResult<SmallVec<[ConstraintId; 8]>> {
        let mut scan = Some(UniqueConstraintScan::new(node));
        let mut result = SmallVec::new();
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    scan.as_mut()
                        .ok_or(RunError::Contract("unique constraint scan already retired"))?
                        .advance_with(
                            &self.builder.storage.borrow(),
                            &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                        )
                        .map_err(|error| self.error(error))
                })
                .await;
            match progress {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(Some(id)) => {
                    self.endpoint
                        .local_call(|| {
                            reserve_smallvec(
                                &mut result,
                                1,
                                AllocationKind::UniqueConstraintOutput,
                                &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                            )
                            .map_err(|error| self.error(error))?;
                            result.push(id);
                            Ok(())
                        })
                        .await;
                }
                ControlFlow::Break(None) => {
                    self.endpoint
                        .local_call(|| {
                            self.endpoint.admit_work(1)?;
                            drop(scan.take());
                            Ok(())
                        })
                        .await;
                    return Ok(result);
                }
            }
        }
    }
    async fn collect_source_order(
        &mut self,
        source_order: Option<SourceOrderId>,
    ) -> RunResult<FxIndexSet<ConstraintId>> {
        let mut scan = Some(SourceOrderScan::new(source_order));
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    scan.as_mut()
                        .ok_or(RunError::Contract("source order scan already retired"))?
                        .advance_with(
                            &self.builder.storage.borrow(),
                            &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                        )
                        .map_err(|error| self.error(error))
                })
                .await;
            if progress.is_break() {
                return Ok(self
                    .endpoint
                    .local_call(|| {
                        self.endpoint.admit_work(1)?;
                        let scan = scan
                            .take()
                            .ok_or(RunError::Contract("source order scan already retired"))?;
                        Ok(scan.result)
                    })
                    .await);
            }
        }
    }
    async fn is_single_conjunction(&mut self, node: NodeId) -> RunResult<bool> {
        let mut scan = SingleConjunctionScan::new(node);
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    scan.advance_with(
                        &self.builder.storage.borrow(),
                        &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                    )
                    .map_err(|error| self.error(error))
                })
                .await;
            if let ControlFlow::Break(result) = progress {
                return Ok(result);
            }
        }
    }
    async fn as_concrete(
        &mut self,
        constraint: Constraint<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        constraint_as_concrete_with(constraint, self).await
    }
    async fn existing_typevar_id(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarId> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                let identity = typevar.identity_with_fields(self.fields);
                self.builder
                    .storage
                    .borrow()
                    .typevar_cache
                    .get(&identity)
                    .copied()
                    .ok_or(RunError::Contract(
                        "constraint typevar is absent from its builder",
                    ))
            })
            .await)
    }
    async fn extend_dependent_support(
        &mut self,
        constraint: ConstraintId,
        dependent: &mut FxHashSet<TypeVarId>,
    ) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                extend_dependent_support_with(
                    &self.builder.storage.borrow(),
                    constraint,
                    dependent,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
    async fn reserve_typevars(
        &mut self,
        set: &mut FxHashSet<TypeVarId>,
        kind: PathTypevarSet,
    ) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                reserve_path_typevars_with(
                    set,
                    kind,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> PathVisitEffects<'db, IsNeverSatisfiedVisitor>
    for RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    async fn visit_node(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.visit_node())
            })
            .await)
    }
    async fn visit_satisfied(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        path: &PathAssignments,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.visit_satisfied(self.db, &mut self.builder.storage.borrow_mut(), path))
            })
            .await)
    }
    async fn visit_unsatisfied(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        path: &PathAssignments,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(
                    visitor.visit_unsatisfied(
                        self.db,
                        &mut self.builder.storage.borrow_mut(),
                        path,
                    ),
                )
            })
            .await)
    }
    async fn visit_impossible(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        path: &PathAssignments,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.visit_impossible(self.db, &mut self.builder.storage.borrow_mut(), path))
            })
            .await)
    }
    async fn enter_interior(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        interior: InteriorNode,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.enter_interior(
                    self.db,
                    &mut self.builder.storage.borrow_mut(),
                    interior,
                ))
            })
            .await)
    }
    async fn visit_edge(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        interior_value: &(),
        subtree: (),
        path: &PathAssignments,
        new_range: Range<usize>,
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.visit_edge(
                    self.db,
                    &mut self.builder.storage.borrow_mut(),
                    interior_value,
                    subtree,
                    path,
                    new_range,
                ))
            })
            .await)
    }
    async fn leave_interior(
        &mut self,
        visitor: &mut IsNeverSatisfiedVisitor,
        interior_value: &(),
        if_true: (),
        if_uncertain: (),
        if_false: (),
    ) -> RunResult<ControlFlow<()>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(visitor.leave_interior(
                    self.db,
                    &mut self.builder.storage.borrow_mut(),
                    interior_value,
                    if_true,
                    if_uncertain,
                    if_false,
                ))
            })
            .await)
    }
    async fn or_nodes(&mut self, left: NodeId, right: NodeId) -> RunResult<NodeId> {
        let mut cursor = Some(TddApply::new(Operation::Or(left, right)));
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    cursor
                        .as_mut()
                        .ok_or(RunError::Contract("node OR cursor already retired"))?
                        .advance_with(
                            &mut self.builder.storage.borrow_mut(),
                            &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                        )
                        .map_err(|error| self.error(error))
                })
                .await;
            if let ControlFlow::Break(result) = progress {
                self.endpoint
                    .local_call(|| {
                        self.endpoint.admit_work(1)?;
                        drop(cursor.take());
                        self.observe(Observation::Or(left, right, result));
                        Ok(())
                    })
                    .await;
                return Ok(result);
            }
        }
    }
    async fn reserve_frames(
        &mut self,
        frames: &mut Vec<PathVisitFrame<IsNeverSatisfiedVisitor>>,
    ) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                reserve_path_frames_with(
                    frames,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| self.error(error))
            })
            .await;
        Ok(())
    }
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> ConstraintTypeEffects<'db>
    for RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    type Error = RunError;
    async fn checkpoint(&mut self, work: ConstraintTypeWork) -> RunResult<()> {
        let _ = work;
        self.endpoint
            .local_call(|| self.endpoint.admit_work(1))
            .await;
        Ok(())
    }
    async fn search_bound(&mut self, bound: Type<'db>, search: BoundSearch) -> RunResult<bool> {
        let result = type_search::search_bound(self.db, self.endpoint, bound, search).await?;
        if self.observer.is_some() {
            self.endpoint
                .local_call(|| {
                    self.observe(Observation::BoundSearch(bound, search, result));
                    Ok(())
                })
                .await;
        }
        Ok(result)
    }
    async fn materialize_bound(
        &mut self,
        bound: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        if !Q::CONCRETE_AVAILABLE {
            return self
                .refuse(UnsupportedSatisfactionOperation::BoundMaterialization)
                .await;
        }
        self.queries
            .materialize(self.endpoint, self.db, self.program, bound, kind)
            .await
    }
    async fn type_depth(&mut self, bound: Type<'db>) -> RunResult<(u16, u16)> {
        if self.observer.is_some() {
            self.endpoint
                .local_call(|| {
                    self.observe(Observation::BeforeDepth(bound));
                    Ok(())
                })
                .await;
        }
        let depth = type_search::type_depth(self.db, self.endpoint, bound).await?;
        if self.observer.is_some() {
            self.endpoint
                .local_call(|| {
                    self.observe(Observation::AfterDepth(bound, depth));
                    Ok(())
                })
                .await;
        }
        Ok(depth)
    }
}

impl<'call, 'run: 'call, 'db: 'run, 'c: 'call, Q> ConstraintDepthCacheEffects<'db>
    for RuntimeSatisfaction<'call, 'run, 'db, 'c, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    async fn depth_cache_get(&mut self, id: ConstraintId) -> RunResult<Option<(u16, u16)>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(self
                    .builder
                    .storage
                    .borrow()
                    .constraint_bound_depth_cache
                    .get(&id)
                    .copied())
            })
            .await)
    }
    async fn depth_constraint(&mut self, id: ConstraintId) -> RunResult<Constraint<'db>> {
        self.constraint_data(id).await
    }
    async fn depth_cache_publish(&mut self, id: ConstraintId, depth: (u16, u16)) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.observe(Observation::BeforeDepthCacheInsert(id, depth));
                self.endpoint.admit_work(1)?;
                let mut storage = self.builder.storage.borrow_mut();
                let admission = EndpointAdmission(self.endpoint);
                let mut control = ExecutionControl::new(&admission);
                if !storage.constraint_bound_depth_cache.contains_key(&id) {
                    reserve_map(
                        &mut storage.constraint_bound_depth_cache,
                        TableKind::DepthCache,
                        &mut control,
                    )
                    .map_err(|error| self.error(error))?;
                }
                control.admit(TddWork::Commit)?;
                storage.constraint_bound_depth_cache.insert(id, depth);
                Ok(())
            })
            .await;
        Ok(())
    }
}
