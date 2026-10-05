//! Callable protocol queries and their original comparison owners.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::ops::ControlFlow;

use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, FixedQueryKeys, NativeValueOperation, NativeValueQuote,
    RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};

use super::{PairWork, RuntimePairs, UnsupportedPairOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::call::Bindings;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::instance::ProtocolInterfaceSource;
use crate::types::instance::protocol_object::{
    ProtocolObjectEffects, ProtocolObjectWork, protocol_object_compare_with,
    protocol_object_equivalence_with,
};
use crate::types::instance::protocol_relation::{
    ProtocolRelationEffects, ProtocolRelationWork, check_type_satisfies_protocol_with,
};
use crate::types::member_lookup::runtime::{MemberQueryAccess, NoMemberQueries};
use crate::types::protocol_class::member_presence::{
    ProtocolMembersDefinedEffects, ProtocolMembersDefinedWork, protocol_members_defined_with,
};
use crate::types::protocol_class::outer_step::InterfaceMembers;
use crate::types::protocol_class::{
    ProtocolClass, ProtocolInterface, ProtocolInterfaceView, ProtocolMember,
    StructuralMemberPriority,
};
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallRelationOwners,
};
use crate::types::relation::{
    RelationFieldReads, TypeRelation, TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ClassType, ErrorContext, MemberLookupPolicy, MemberLookupResult, NominalInstanceType,
    ProtocolInstanceType, StaticClassLiteral, Type,
};
use crate::{Db, Program, ProgramEnvironment};

pub(in crate::types) trait ProtocolQueryAccess<'run, 'db: 'run>:
    Clone + 'run
{
    const AVAILABLE: bool;
    const MEMBERS_AVAILABLE: bool = false;
    const SOLVER_AVAILABLE: bool = false;

    fn constraint_satisfaction<'call, 'c>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        _program: Program<'db>,
        _constraints: ConstraintSet<'db, 'c>,
        _always: bool,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
        'c: 'call,
    {
        async move {
            Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Err(unsupported(
                        db,
                        UnsupportedPairOperation::ConstraintSatisfaction,
                    ))
                })
                .await)
        }
    }

    fn interface<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = RunResult<ProtocolInterfaceView<'db>>> + 'call
    where
        'run: 'call;

    fn member_lookup<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> impl Future<Output = RunResult<MemberLookupResult<'db>>> + 'call
    where
        'run: 'call;

    fn object_equivalence<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call;
}

#[derive(Clone, Copy)]
pub(in crate::types) struct NoProtocolQueries;

impl<'run, 'db: 'run> ProtocolQueryAccess<'run, 'db> for NoProtocolQueries {
    const AVAILABLE: bool = false;

    async fn interface<'call>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>>
    where
        'run: 'call,
    {
        Err(RunError::Contract(
            "protocol query capability is unavailable",
        ))
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
        NoMemberQueries
            .member(endpoint, program, ty, name, policy)
            .await
    }

    async fn object_equivalence<'call>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::Contract(
            "protocol query capability is unavailable",
        ))
    }
}

pub(in crate::types) struct ProtocolQueries<
    'run,
    'db: 'run,
    CO: InternedQueryConfiguration,
    CI: Configuration,
    M = NoMemberQueries,
> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) object: CallableRoute<'run, 'db, CO>,
    pub(in crate::types) interface: CallableRoute<'run, 'db, CI>,
    pub(in crate::types) object_keys: &'run FixedQueryKeys<'db, CO>,
    pub(in crate::types) members: M,
}

impl<CO: InternedQueryConfiguration, CI: Configuration, M: Clone> Clone
    for ProtocolQueries<'_, '_, CO, CI, M>
{
    fn clone(&self) -> Self {
        Self {
            db: self.db,
            object: self.object.clone(),
            interface: self.interface.clone(),
            object_keys: self.object_keys,
            members: self.members.clone(),
        }
    }
}

fn unsupported(db: &dyn Db, operation: UnsupportedPairOperation) -> RunError {
    let reason = expansion_probe::refuse(db, Incomplete::UnsupportedPairOperation(operation));
    RunError::Refused(match reason {
        Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
        Incomplete::RequestedAllocation => salsa::attempt_probe::Incomplete::RequestedAllocation,
        _ => salsa::attempt_probe::Incomplete::Interrupted,
    })
}

impl<'run, 'db: 'run, CO, CI, M: MemberQueryAccess<'run, 'db>> ProtocolQueryAccess<'run, 'db>
    for ProtocolQueries<'run, 'db, CO, CI, M>
where
    CO: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (ProtocolInstanceType<'a>, ()),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>,
    CI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassType<'a>,
            Output<'a> = ProtocolInterface<'a>,
        >,
{
    const AVAILABLE: bool = true;
    const MEMBERS_AVAILABLE: bool = M::AVAILABLE;

    async fn interface<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>>
    where
        'run: 'call,
    {
        let source = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                salsa::attempt_probe::charge(self.db, 0).map_err(RunError::Refused)?;
                match RelationFieldReads::new(self.db).protocol_interface_source(protocol) {
                    ProtocolInterfaceSource::Materialized { origin, kind } => {
                        let _ = (origin, kind);
                        Err(unsupported(self.db, UnsupportedPairOperation::Protocol))
                    }
                    source => Ok(source),
                }
            })
            .await;
        match source {
            ProtocolInterfaceSource::Class(origin) => Ok(endpoint
                .child_call(|| async {
                    let interface = *endpoint
                        .fetch_ref(&self.interface, (*origin).as_id())?
                        .await?;
                    Ok(ProtocolInterfaceView::new(interface, None))
                })
                .await),
            ProtocolInterfaceSource::Synthesized(interface) => {
                Ok(ProtocolInterfaceView::new(interface, None))
            }
            ProtocolInterfaceSource::Materialized { .. } => Err(RunError::Contract(
                "materialized interface passed source admission",
            )),
        }
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
        self.members
            .member(endpoint, program, ty, name, policy)
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
        let id = endpoint
            .intern_query_key(self.object_keys, (protocol, ()))
            .await;
        Ok(endpoint
            .child_call(|| async { Ok(*endpoint.fetch_ref(&self.object, id)?.await?) })
            .await)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ProtocolRuntimeSite {
    Object,
    Direct,
    Pair,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ProtocolRuntimeRecord {
    pub(in crate::types) site: ProtocolRuntimeSite,
    pub(in crate::types) owners: [*const (); 6],
}

#[derive(Default)]
pub(in crate::types) struct ProtocolRuntimeObservations {
    pub(in crate::types) records: RefCell<Vec<ProtocolRuntimeRecord>>,
    pub(in crate::types) object_bodies: Cell<usize>,
    pub(in crate::types) preflights: Cell<usize>,
    pub(in crate::types) member_advances: Cell<usize>,
    pub(in crate::types) terminal_checks: Cell<usize>,
    pub(in crate::types) comparison_scopes: Cell<usize>,
    pub(in crate::types) comparison_drops: Cell<usize>,
}

impl ProtocolRuntimeObservations {
    fn comparison_scope(&self) -> ComparisonScope<'_> {
        self.comparison_scopes.set(self.comparison_scopes.get() + 1);
        ComparisonScope(self)
    }

    pub(super) fn record(
        &self,
        site: ProtocolRuntimeSite,
        checker: &TypeRelationChecker<'_, '_, '_>,
    ) {
        self.records.borrow_mut().push(ProtocolRuntimeRecord {
            site,
            owners: [
                std::ptr::from_ref(checker.env).cast(),
                std::ptr::from_ref(checker.constraints).cast(),
                std::ptr::from_ref(checker.relation_visitor).cast(),
                std::ptr::from_ref(checker.disjointness_visitor).cast(),
                std::ptr::from_ref(checker.signature_relation_visitor).cast(),
                std::ptr::from_ref(checker.materialization_visitor).cast(),
            ],
        });
    }
}

struct ComparisonScope<'a>(&'a ProtocolRuntimeObservations);

impl Drop for ComparisonScope<'_> {
    fn drop(&mut self) {
        self.0
            .comparison_scopes
            .set(self.0.comparison_scopes.get() - 1);
        self.0
            .comparison_drops
            .set(self.0.comparison_drops.get() + 1);
    }
}

pub(in crate::types) async fn check_protocol_pair_for_test<
    'run,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    checker: TypeRelationChecker<'run, 'run, 'db>,
    source: Type<'db>,
    target: Type<'db>,
    queries: Q,
    observations: Option<&'run ProtocolRuntimeObservations>,
) -> RunResult<ConstraintSet<'db, 'run>> {
    let mut effects = RuntimePairs::with_queries(db, endpoint, checker.constraints, queries);
    effects.protocol_observations = observations;
    PairEffects::check_type_pair(&effects, &checker, source, target).await
}

pub(in crate::types) async fn check_protocol_presence_for_test<
    'run,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    checker: TypeRelationChecker<'run, 'run, 'db>,
    source: Type<'db>,
    target: ProtocolInstanceType<'db>,
    queries: Q,
    observations: Option<&'run ProtocolRuntimeObservations>,
) -> RunResult<bool> {
    let mut effects = RuntimePairs::with_queries(db, endpoint, checker.constraints, queries);
    effects.protocol_observations = observations;
    ProtocolRelationEffects::has_all_protocol_members_defined(&effects, &checker, source, target)
        .await
}

pub(in crate::types) struct ProtocolObjectProvider<'run, 'db: 'run, Q> {
    pub(in crate::types) queries: Q,
    pub(in crate::types) environments: &'run CallEnvironments<'db>,
    pub(in crate::types) builders: &'run CallBuilders<'db>,
    pub(in crate::types) owners: &'run CallRelationOwners<'run, 'run, 'db>,
    pub(in crate::types) observations: Option<&'run ProtocolRuntimeObservations>,
}

impl<'run, 'db: 'run, CO, Q> CallableRouteProvider<'run, 'db, CO>
    for ProtocolObjectProvider<'run, 'db, Q>
where
    CO: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = (ProtocolInstanceType<'a>, ()),
            Output<'a> = bool,
        >,
    Q: ProtocolQueryAccess<'run, 'db>,
{
    async fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, CO>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // ProtocolInstanceType derives Clone over its finite Protocol enum and PhantomData.
            // Every Protocol variant contains only tags and generated handles; () has no fields.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 4,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "protocol object input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { .. } => 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        (protocol, ()): (ProtocolInstanceType<'db>, ()),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        if let Some(observations) = self.observations {
            observations
                .object_bodies
                .set(observations.object_bodies.get() + 1);
        }
        protocol_object_equivalence_with(
            RelationFieldReads::new(db),
            protocol,
            &ObjectEffects {
                db,
                endpoint: &endpoint,
                provider: self,
            },
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: (ProtocolInstanceType<'db>, ()),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(true)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call bool,
        value: bool,
        _input: (ProtocolInstanceType<'db>, ()),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(value)
    }
}

struct ObjectEffects<'call, 'run, 'db: 'run, Q> {
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    provider: &'call ProtocolObjectProvider<'run, 'db, Q>,
}

impl<'run, 'db: 'run, Q: ProtocolQueryAccess<'run, 'db>> ProtocolObjectEffects<'db>
    for ObjectEffects<'_, 'run, 'db, Q>
{
    type Error = RunError;

    async fn checkpoint(&self, work: ProtocolObjectWork) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                let units = match work {
                    ProtocolObjectWork::Entry | ProtocolObjectWork::Complete => Some(1),
                    ProtocolObjectWork::MemberName { bytes } => bytes.checked_add(1),
                }
                .ok_or(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance,
                ))?;
                self.endpoint.admit_work(units)?;
                salsa::attempt_probe::charge(self.db, 0).map_err(RunError::Refused)
            })
            .await;
        Ok(())
    }

    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>> {
        let interface = self
            .provider
            .queries
            .interface(self.endpoint, protocol)
            .await?;
        self.endpoint
            .local_call(|| {
                let count =
                    RelationFieldReads::new(self.db).protocol_interface_member_count(interface);
                let units = count
                    .checked_add(1)
                    .and_then(|count| count.checked_mul(2))
                    .ok_or(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance,
                    ))?;
                self.endpoint.admit_work(units)
            })
            .await;
        Ok(interface)
    }

    async fn compare_object(
        &self,
        program: Program<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        let environments: &'run CallEnvironments<'db> = self.provider.environments;
        let builders: &'run CallBuilders<'db> = self.provider.builders;
        let owner_pool: &'run CallRelationOwners<'run, 'run, 'db> = self.provider.owners;
        let env: &'run ProgramEnvironment<'db> =
            environments.allocate(self.endpoint, program).await;
        let builder: &'run ConstraintSetBuilder<'db> = builders.allocate(self.endpoint).await;
        let owners = owner_pool.allocate(self.endpoint, env, builder).await;
        let checker = owners.subtyping(TypeVarSet::None);
        if let Some(observations) = self.provider.observations {
            observations.record(ProtocolRuntimeSite::Object, &checker);
        }
        let _comparison_scope = self
            .provider
            .observations
            .map(ProtocolRuntimeObservations::comparison_scope);
        let mut effects = RuntimePairs::with_queries(
            self.db,
            self.endpoint.clone(),
            builder,
            self.provider.queries.clone(),
        );
        effects.protocol_observations = self.provider.observations;
        protocol_object_compare_with(&checker, protocol, &effects).await
    }
}

impl<'run, 'db: 'run, 'c: 'run, Q: ProtocolQueryAccess<'run, 'db>> RuntimePairs<'run, 'db, 'c, Q> {
    pub(super) async fn request_direct<'a: 'run>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let checker = checker.clone();
        let queries = self.queries.clone();
        let endpoint = self.endpoint.clone();
        let db = self.db;
        let builder = self.builder;
        let observations = self.observations;
        let protocol_observations = self.protocol_observations;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .demand(move || async move {
                        let mut effects =
                            RuntimePairs::with_queries(db, endpoint, builder, queries);
                        effects.observations = observations;
                        effects.protocol_observations = protocol_observations;
                        effects
                            .endpoint
                            .local_call(|| {
                                effects.endpoint.admit_work(1)?;
                                effects.verify_database()?;
                                effects.verify_builder(checker.constraints)?;
                                if !matches!(
                                    checker.relation,
                                    TypeRelation::Assignability | TypeRelation::Subtyping
                                ) || checker.typevar_evaluation != TypeVarEvaluation::Eager
                                    || checker.observations.is_some()
                                    || checker.is_context_collection_enabled()
                                {
                                    return Err(
                                        effects.unsupported(UnsupportedPairOperation::CheckerMode)
                                    );
                                }
                                if let Some(observations) = protocol_observations {
                                    observations.record(ProtocolRuntimeSite::Direct, &checker);
                                }
                                Ok(())
                            })
                            .await;
                        check_type_satisfies_protocol_with(
                            RelationFieldReads::new(db),
                            &checker,
                            ty,
                            protocol,
                            &effects,
                        )
                        .await
                    })?
                    .await
            })
            .await)
    }

    pub(super) async fn terminal_satisfaction<'a>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        always: bool,
    ) -> RunResult<bool> {
        let terminal = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                self.verify_database()?;
                self.verify_builder(checker.constraints)?;
                constraints.verify_builder(self.builder);
                if let Some(observations) = self.protocol_observations {
                    observations
                        .terminal_checks
                        .set(observations.terminal_checks.get() + 1);
                }
                if constraints.is_trivially_never_satisfied() {
                    return Ok(ControlFlow::Break(!always));
                }
                if constraints.is_trivially_always_satisfied() {
                    return Ok(ControlFlow::Break(always));
                }
                if !Q::SOLVER_AVAILABLE {
                    return Err(self.unsupported(UnsupportedPairOperation::ConstraintSatisfaction));
                }
                Ok(ControlFlow::Continue(checker.env.program(self.db)))
            })
            .await;
        match terminal {
            ControlFlow::Break(result) => Ok(result),
            ControlFlow::Continue(program) => {
                self.queries
                    .constraint_satisfaction(&self.endpoint, self.db, program, constraints, always)
                    .await
            }
        }
    }

    async fn advance_protocol_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> RunResult<Option<ProtocolMember<'db, 'db>>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.verify_database()?;
                if let Some(observations) = self.protocol_observations {
                    observations
                        .member_advances
                        .set(observations.member_advances.get() + 1);
                }
                Ok(members.next())
            })
            .await)
    }
}

impl<'run, 'db: 'run, 'c: 'run, Q: ProtocolQueryAccess<'run, 'db>>
    ProtocolMembersDefinedEffects<'db> for RuntimePairs<'run, 'db, 'c, Q>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ProtocolMembersDefinedWork) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.verify_database()
            })
            .await;
        Ok(())
    }
    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>> {
        self.queries.interface(&self.endpoint, protocol).await
    }
    async fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> RunResult<Option<ProtocolMember<'db, 'db>>> {
        self.advance_protocol_member(members).await
    }
    async fn non_object_member_count(
        &self,
        _interface: ProtocolInterface<'db>,
    ) -> RunResult<usize> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn includes_member_or_object_fallback(
        &self,
        _source: ProtocolInterfaceView<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &'db str,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn restricted_member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        if !Q::MEMBERS_AVAILABLE {
            return self.refuse(UnsupportedPairOperation::Protocol).await;
        }
        let result = self
            .queries
            .member_lookup(
                &self.endpoint,
                env.program(self.db),
                ty,
                name,
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(result
                    .unwrap_or_else(|error| error.fallback_member(self.db))
                    .member(self.db))
            })
            .await)
    }
    async fn member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        if !Q::MEMBERS_AVAILABLE {
            return self.refuse(UnsupportedPairOperation::Protocol).await;
        }
        let result = self
            .queries
            .member_lookup(
                &self.endpoint,
                env.program(self.db),
                ty,
                name,
                MemberLookupPolicy::default(),
            )
            .await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(result
                    .unwrap_or_else(|error| error.fallback_member(self.db))
                    .member(self.db))
            })
            .await)
    }
}

impl<'run, 'a: 'run, 'db: 'run, 'c: 'run, Q: ProtocolQueryAccess<'run, 'db>>
    ProtocolRelationEffects<'a, 'c, 'db> for RuntimePairs<'run, 'db, 'c, Q>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ProtocolRelationWork) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.verify_database()
            })
            .await;
        Ok(())
    }
    async fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::check_type_pair(self, checker, source, target).await
    }
    async fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.request_direct(checker, ty, protocol).await
    }
    async fn check_protocol_interface(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source_type: Type<'db>,
        _source: ProtocolInterfaceView<'db>,
        _target: ProtocolInterfaceView<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn check_protocol_member<'member>(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
        _member: ProtocolMember<'member, 'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn check_meta_protocol_members(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _instance_ty: Type<'db>,
        _meta_ty: Type<'db>,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::MetaProtocol).await
    }
    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.terminal_satisfaction(checker, constraints, false)
            .await
    }
    async fn is_always_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.terminal_satisfaction(checker, constraints, true).await
    }
    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<ProtocolInterfaceView<'db>> {
        self.queries.interface(&self.endpoint, protocol).await
    }
    async fn has_all_protocol_members_defined(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        self.endpoint
            .local_call(|| {
                self.verify_database()?;
                self.verify_builder(checker.constraints)?;
                if let Some(observations) = self.protocol_observations {
                    observations
                        .preflights
                        .set(observations.preflights.get() + 1);
                }
                Ok(())
            })
            .await;
        protocol_members_defined_with(
            RelationFieldReads::new(self.db),
            checker.env,
            ty,
            protocol,
            self,
        )
        .await
    }
    async fn nominal_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _nominal: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.refuse(UnsupportedPairOperation::NominalInstance).await
    }
    async fn materialization_changes_requirements(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _protocol: ProtocolInstanceType<'db>,
        _required: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn identity_specialization(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.refuse(UnsupportedPairOperation::ClassDefaultSpecialization)
            .await
    }
    async fn into_protocol_class(
        &self,
        _class: ClassType<'db>,
    ) -> RunResult<Option<ProtocolClass<'db>>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn non_recursive_protocol_interface(
        &self,
        _interface: ProtocolInterface<'db>,
        _protocol: ProtocolClass<'db>,
        _receiver: Type<'db>,
    ) -> RunResult<ProtocolInterface<'db>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn argument_has_unmentioned_typevar(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _argument: Type<'db>,
        _constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn member_has_explicit_receiver_annotation<'member>(
        &self,
        _member: ProtocolMember<'member, 'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn structural_member_priority<'member>(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _member: ProtocolMember<'member, 'db>,
    ) -> RunResult<StructuralMemberPriority> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn to_class_type(&self, _ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        self.refuse(UnsupportedPairOperation::MetaProtocol).await
    }
    async fn bindings(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.refuse(UnsupportedPairOperation::MetaProtocol).await
    }
    async fn bindings_return_type(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::MetaProtocol).await
    }
    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::combine_constraints(self, builder, kind, left, right).await
    }
    async fn union_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        result: &mut ConstraintSet<'db, 'c>,
        other: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let combined = PairEffects::combine_constraints(
            self,
            builder,
            ConstraintFoldKind::Any,
            *result,
            other,
        )
        .await?;
        Ok(self
            .endpoint
            .local_call(|| {
                *result = combined;
                Ok(combined)
            })
            .await)
    }
    async fn imply_constraints(
        &self,
        _builder: &'c ConstraintSetBuilder<'db>,
        _antecedent: ConstraintSet<'db, 'c>,
        _consequent: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::ConstraintSatisfaction)
            .await
    }
    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        PairEffects::push_constraints(self, fold, next).await
    }
    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::finish_constraints(self, fold).await
    }
    async fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> RunResult<Option<ProtocolMember<'db, 'db>>> {
        self.advance_protocol_member(members).await
    }
    async fn reserve_member_priorities(
        &self,
        _capacity: usize,
    ) -> RunResult<Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn push_member_priority(
        &self,
        _members: &mut Vec<(StructuralMemberPriority, ProtocolMember<'db, 'db>)>,
        _priority: StructuralMemberPriority,
        _member: ProtocolMember<'db, 'db>,
    ) -> RunResult<()> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn sort_member_priorities(
        &self,
        _members: &mut [(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
    ) -> RunResult<()> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn advance_prioritized_member(
        &self,
        _members: &[(StructuralMemberPriority, ProtocolMember<'db, 'db>)],
        _index: &mut usize,
    ) -> RunResult<ProtocolMember<'db, 'db>> {
        self.refuse(UnsupportedPairOperation::Protocol).await
    }
    async fn report_error(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _error: ErrorContext<'db>,
    ) -> RunResult<()> {
        self.refuse(UnsupportedPairOperation::CheckerMode).await
    }
}
