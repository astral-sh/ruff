//! Canonical sequent producers with retained semantic query capabilities.

use std::borrow::Cow;

use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, ExecutionWork, NativeValueOperation, NativeValueQuote,
    QueryKeys, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};

use super::effects::{
    ConjunctionStep, DomainEndpoint, OwnedConjunctionCursor, SequentBuffer, SequentEffects,
    SequentFields, SequentWork,
};
use super::profile::{PairSequentProfile, SingleSequentProfile};
use super::{
    Sequent, SequentGroup, SequentMap, pair_cannot_produce_with, pair_sequents_with,
    single_sequents_with,
};
use crate::types::constraints::OwnedConstraintSet;
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{TddError, sequence_growth};
use crate::types::constraints::runtime::EndpointAdmission;
use crate::types::constraints::runtime::satisfaction::{ConstraintQueryAccess, static_eligible};
use crate::types::constraints::variables::Constraint;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarDomain};
use crate::types::{BoundTypeVarInstance, MaterializationKind, Type, TypeVarVariance};
use crate::{Db, Program};

mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedSequentOperation {
    ParameterSignatureEndpoint,
    Materialize,
    StaticEligibility,
    Variance,
    Substitute,
    Assignable,
    Equivalent,
    OwnedAssignable,
    OwnedEquivalent,
    TriviallyDisjoint,
    Union,
    Intersection,
}

struct RuntimeSequentEffects<'call, 'run, 'db, Q> {
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    queries: &'call Q,
    fields: SequentFields<'db>,
}
impl<'call, 'run, 'db, Q> RuntimeSequentEffects<'call, 'run, 'db, Q> {
    fn new(
        db: &'db dyn Db,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        queries: &'call Q,
    ) -> Self {
        Self {
            db,
            endpoint,
            program,
            queries,
            fields: SequentFields::new(db),
        }
    }
    fn error(&self, error: TddError<RunError>) -> RunError {
        match error {
            TddError::Refused(error) => error,
            TddError::CapacityExhausted => self.incomplete(Incomplete::ConstraintCapacityExhausted),
        }
    }
    fn incomplete(&self, reason: Incomplete) -> RunError {
        let reason = expansion_probe::refuse(self.db, reason);
        RunError::Refused(match reason {
            Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
            Incomplete::RequestedAllocation => salsa::attempt_probe::Incomplete::RequestedAllocation,
            _ => salsa::attempt_probe::Incomplete::Interrupted,
        })
    }
    async fn refuse<T>(&self, operation: UnsupportedSequentOperation) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Err(self.incomplete(Incomplete::UnsupportedSequentOperation(operation)))
            })
            .await)
    }
    fn checked(&self, value: Option<usize>) -> RunResult<usize> {
        value.ok_or_else(|| self.incomplete(Incomplete::ConstraintCapacityExhausted))
    }
    fn payload<T>(&self, count: usize) -> RunResult<()> {
        let bytes = self.checked(count.checked_mul(size_of::<T>()))?;
        if bytes > isize::MAX as usize {
            return Err(self.incomplete(Incomplete::ConstraintCapacityExhausted));
        }
        if bytes != 0 {
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: bytes,
            })?;
        }
        Ok(())
    }
    fn reserve<T>(&self, values: &mut Vec<T>) -> RunResult<()> {
        self.endpoint.admit_work(1)?;
        if values.len() == values.capacity() {
            let required = self.checked(values.len().checked_add(1))?;
            let plan = sequence_growth::<T, RunError>(values.capacity(), required)
                .map_err(|error| self.error(error))?;
            self.endpoint.admit_work(plan.relocation_units)?;
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: plan.requested_payload_bytes,
            })?;
            values.reserve_exact(plan.requested_capacity - values.len());
        }
        Ok(())
    }
}

impl<'run, 'db: 'run, Q> SequentEffects<'db> for RuntimeSequentEffects<'_, 'run, 'db, Q>
where
    Q: ConstraintQueryAccess<'run, 'db>,
{
    type Error = RunError;
    fn fields(&self) -> SequentFields<'db> {
        self.fields
    }
    async fn checkpoint(&mut self, work: SequentWork) -> Result<(), Self::Error> {
        let _ = work;
        self.endpoint
            .local_call(|| self.endpoint.admit_work(1))
            .await;
        Ok(())
    }
    async fn domain_endpoint(
        &mut self,
        domain: TypeVarDomain,
        end: DomainEndpoint,
    ) -> Result<Type<'db>, Self::Error> {
        match domain {
            TypeVarDomain::ParameterSignature => {
                self.refuse(UnsupportedSequentOperation::ParameterSignatureEndpoint)
                    .await
            }
            TypeVarDomain::Type | TypeVarDomain::TypeTuple => Ok(self
                .endpoint
                .local_call(|| {
                    self.endpoint.admit_work(1)?;
                    Ok(match end {
                        DomainEndpoint::Bottom => Type::Never,
                        DomainEndpoint::Top => Type::object(),
                    })
                })
                .await),
        }
    }
    async fn materialize(
        &mut self,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self.refuse(UnsupportedSequentOperation::Materialize).await;
        }
        self.queries
            .materialize(self.endpoint, self.db, self.program, ty, kind)
            .await
    }
    async fn static_eligible(&mut self, ty: Type<'db>) -> Result<bool, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self
                .refuse(UnsupportedSequentOperation::StaticEligibility)
                .await;
        }
        static_eligible(self.db, self.endpoint, ty).await
    }
    async fn variance(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarIdentity<'db>,
    ) -> Result<TypeVarVariance, Self::Error> {
        let _ = (ty, variable);
        self.refuse(UnsupportedSequentOperation::Variance).await
    }
    async fn substitute(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let _ = (ty, variable, replacement);
        self.refuse(UnsupportedSequentOperation::Substitute).await
    }
    async fn assignable(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self.refuse(UnsupportedSequentOperation::Assignable).await;
        }
        self.queries
            .assignable(self.endpoint, self.db, self.program, left, right)
            .await
    }
    async fn equivalent(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self.refuse(UnsupportedSequentOperation::Equivalent).await;
        }
        self.queries
            .equivalent(self.endpoint, self.db, self.program, left, right)
            .await
    }
    async fn owned_assignable(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self
                .refuse(UnsupportedSequentOperation::OwnedAssignable)
                .await;
        }
        self.queries
            .owned_assignable(self.endpoint, self.db, self.program, left, right)
            .await
    }
    async fn owned_equivalent(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error> {
        if !Q::CONCRETE_AVAILABLE {
            return self
                .refuse(UnsupportedSequentOperation::OwnedEquivalent)
                .await;
        }
        self.queries
            .owned_equivalent(self.endpoint, self.db, self.program, left, right)
            .await
    }
    async fn trivially_disjoint(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Self::Error> {
        let _ = (left, right);
        self.refuse(UnsupportedSequentOperation::TriviallyDisjoint)
            .await
    }
    async fn union(&mut self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error> {
        let _ = (left, right);
        self.refuse(UnsupportedSequentOperation::Union).await
    }
    async fn intersection(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let _ = (left, right);
        self.refuse(UnsupportedSequentOperation::Intersection).await
    }
    async fn emit(
        &mut self,
        map: &mut SequentMap<'db>,
        sequent: Sequent<Constraint<'db>>,
    ) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                self.reserve(&mut map.pending)?;
                map.pending.push(sequent);
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn prepare_extract(&mut self, map: &SequentMap<'db>) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                self.endpoint
                    .admit_work(self.checked(map.pending.len().checked_add(1))?)?;
                self.payload::<Sequent<Constraint<'db>>>(map.pending.len())
            })
            .await;
        Ok(())
    }
    async fn reserve_group(&mut self, map: &mut SequentMap<'db>) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| self.reserve(&mut map.sequents))
            .await;
        Ok(())
    }
    async fn prepare_shrink(
        &mut self,
        map: &SequentMap<'db>,
        buffer: SequentBuffer,
    ) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                let (len, capacity) = match buffer {
                    SequentBuffer::Groups => (map.sequents.len(), map.sequents.capacity()),
                    SequentBuffer::Pending => (map.pending.len(), map.pending.capacity()),
                };
                self.endpoint
                    .admit_work(self.checked(len.checked_add(1))?)?;
                if capacity != len {
                    match buffer {
                        SequentBuffer::Groups => self.payload::<SequentGroup<'db>>(len)?,
                        SequentBuffer::Pending => self.payload::<Sequent<Constraint<'db>>>(len)?,
                    }
                }
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn conjunction_step(
        &mut self,
        cursor: &mut OwnedConjunctionCursor<'db>,
    ) -> Result<ConjunctionStep<'db>, Self::Error> {
        Ok(self
            .endpoint
            .local_call(|| {
                let admission = EndpointAdmission(self.endpoint);
                cursor
                    .advance_with(&mut ExecutionControl::new(&admission))
                    .map_err(|error| self.error(error))
            })
            .await)
    }
}

fn checked_native_work(work: Option<usize>) -> RunResult<usize> {
    work.ok_or(RunError::Contract(
        "sequent native value quotation overflow",
    ))
}

async fn quote_sequent_slice<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    sequents: &[Sequent<Constraint<'db>>],
) -> RunResult<usize> {
    let mut cursor = sequents.iter();
    let mut work = 1usize;
    loop {
        let next = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                let Some(sequent) = cursor.next() else {
                    return Ok(None);
                };
                endpoint.admit_work(64)?;
                let constraints = match sequent {
                    Sequent::SingleTautology { ante } => [Some(ante), None, None],
                    Sequent::PairImpossibility { ante1, ante2 } => [Some(ante1), Some(ante2), None],
                    Sequent::TripleImpossibility {
                        ante1,
                        ante2,
                        ante3,
                    } => [Some(ante1), Some(ante2), Some(ante3)],
                    Sequent::SingleImplication { ante, post, .. } => [Some(ante), Some(post), None],
                    Sequent::PairImplication {
                        ante1, ante2, post, ..
                    } => [Some(ante1), Some(ante2), Some(post)],
                };
                let mut entry_work = 1usize;
                for constraint in constraints.into_iter().flatten() {
                    // Constraint equality includes provenance, typevar identities and the
                    // bound's finite Type representation. Todo labels also compare their bytes.
                    entry_work = checked_native_work(entry_work.checked_add(16))?;
                    for ty in constraint.type_pair() {
                        entry_work =
                            checked_native_work(entry_work.checked_add(ty.inline_payload_bytes()))?;
                    }
                }
                Ok(Some(entry_work))
            })
            .await;
        let Some(entry_work) = next else {
            return Ok(work);
        };
        work = checked_native_work(work.checked_add(entry_work))?;
    }
}

async fn quote_sequent_comparison<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    left: &SequentMap<'db>,
    right: &SequentMap<'db>,
) -> RunResult<usize> {
    let mut work = 2usize;
    for map in [left, right] {
        let mut groups = map.sequents.iter();
        loop {
            let next = endpoint
                .local_call(|| {
                    endpoint.admit_work(8)?;
                    endpoint.check_completion()?;
                    Ok(groups.next().map(|group| match group {
                        SequentGroup::Ungrouped(sequents) => (sequents.as_ref(), &[][..]),
                        SequentGroup::Grouped {
                            leftwards,
                            rightwards,
                            ..
                        } => (leftwards.as_ref(), rightwards.as_ref()),
                    }))
                })
                .await;
            let Some((leftwards, rightwards)) = next else {
                break;
            };
            // Derived equality includes the group tag and the grouped equivalence's
            // provenance and two interned typevar identities, as well as both slices.
            work = checked_native_work(work.checked_add(8))?;
            for sequents in [leftwards, rightwards] {
                work = checked_native_work(
                    work.checked_add(quote_sequent_slice(endpoint, sequents).await?),
                )?;
            }
        }
        // Pending sequents are part of the derived equality even when a producer normally
        // empties them before publication. Both actual operands must be fully covered.
        work = checked_native_work(
            work.checked_add(quote_sequent_slice(endpoint, &map.pending).await?),
        )?;
    }
    Ok(work)
}

async fn quote_sequent_native_value<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
    input_work: usize,
) -> RunResult<NativeValueQuote>
where
    C: for<'a> Configuration<Output<'a> = SequentMap<'a>>,
{
    let work = match operation {
        NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => input_work,
        NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
            return Err(RunError::Contract(
                "sequent input profile requires a retained argument tuple",
            ));
        }
        NativeValueOperation::Comparison { left, right } => {
            quote_sequent_comparison(&endpoint, left, right).await?
        }
    };
    Ok(NativeValueQuote {
        work,
        requested_bytes: 0,
        cleanup_work: 0,
    })
}

pub(in crate::types::constraints) struct SingleSequentProvider<Q> {
    pub(in crate::types::constraints) queries: Q,
}
impl<'run, 'db: 'run, C, Q> CallableRouteProvider<'run, 'db, C> for SingleSequentProvider<Q>
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    Q: ConstraintQueryAccess<'run, 'db>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        // Tuple Clone copies Program's generated identity and Constraint's derived fields.
        // Type's derived Clone copies its handles and borrowed Todo label, including a label
        // inside SubclassOf; it neither scans those bytes nor allocates ownership to clean up.
        quote_sequent_native_value(endpoint, operation, 17).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        let (program, constraint) = input;
        let mut effects = RuntimeSequentEffects::new(db, &endpoint, program, &self.queries);
        single_sequents_with(constraint, &mut effects).await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(SequentMap::default())
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call SequentMap<'db>,
        value: SequentMap<'db>,
        _input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(value)
    }
}

pub(in crate::types::constraints) struct PairSequentProvider<Q> {
    pub(in crate::types::constraints) queries: Q,
}
impl<'run, 'db: 'run, C, Q> CallableRouteProvider<'run, 'db, C> for PairSequentProvider<Q>
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    Q: ConstraintQueryAccess<'run, 'db>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        // This tuple clones the same Program handle and two fixed-size Constraints.
        quote_sequent_native_value(endpoint, operation, 33).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        let (program, left, right) = input;
        let mut effects = RuntimeSequentEffects::new(db, &endpoint, program, &self.queries);
        pair_sequents_with(left, right, &mut effects).await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(SequentMap::default())
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call SequentMap<'db>,
        value: SequentMap<'db>,
        _input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(value)
    }
}

pub(in crate::types::constraints) struct SequentQueries<'run, 'db, S, P>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    pub(in crate::types::constraints) single_route: CallableRoute<'run, 'db, S>,
    pub(in crate::types::constraints) pair_route: CallableRoute<'run, 'db, P>,
    pub(in crate::types::constraints) single_keys: &'run QueryKeys<'db, S, SingleSequentProfile>,
    pub(in crate::types::constraints) pair_keys: &'run QueryKeys<'db, P, PairSequentProfile>,
}
impl<'run, 'db: 'run, S, P> SequentQueries<'run, 'db, S, P>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    pub(in crate::types::constraints) async fn single<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        constraint: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>>
    where
        'run: 'call,
    {
        let id = endpoint
            .intern_query_key(self.single_keys, (program, constraint))
            .await;
        Ok(endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.single_route, id)?.await })
            .await)
    }
    pub(in crate::types::constraints) async fn pair<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>>
    where
        'run: 'call,
    {
        let id = endpoint
            .intern_query_key(self.pair_keys, (program, left, right))
            .await;
        Ok(endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.pair_route, id)?.await })
            .await)
    }
}

impl<'run, 'db: 'run, S, P> Clone for SequentQueries<'run, 'db, S, P>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    fn clone(&self) -> Self {
        Self {
            single_route: self.single_route.clone(),
            pair_route: self.pair_route.clone(),
            single_keys: self.single_keys,
            pair_keys: self.pair_keys,
        }
    }
}

impl<'run, 'db: 'run, S, P> ConstraintQueryAccess<'run, 'db> for SequentQueries<'run, 'db, S, P>
where
    S: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    P: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    async fn single<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        constraint: Constraint<'db>,
    ) -> RunResult<&'db SequentMap<'db>>
    where
        'run: 'call,
    {
        SequentQueries::single(self, endpoint, program, constraint).await
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
        SequentQueries::pair(self, endpoint, program, left, right).await
    }
}

pub(in crate::types::constraints) async fn pair_cannot_produce<
    'call,
    'run: 'call,
    'db: 'run,
    Q: ConstraintQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Constraint<'db>,
    right: Constraint<'db>,
    queries: &'call Q,
) -> RunResult<bool> {
    pair_cannot_produce_with(
        left,
        right,
        &mut RuntimeSequentEffects::new(db, endpoint, program, queries),
    )
    .await
}
