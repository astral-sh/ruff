//! Constraint-set relation wrappers using the original checker and call resources.

use std::borrow::Cow;
use std::future::Future;
use std::rc::Rc;

use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, InternedValues, NativeValueOperation, NativeValueQuote,
    PassiveMemoSchema, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;

use super::protocol::ProtocolQueryAccess;
use super::{PairWork, RuntimePairs, UnsupportedPairOperation};
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet, UnsupportedSequentOperation,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::cyclic::relation_key_has_fixed_cost;
pub(in crate::types) use crate::types::relation::OwnedRelationKind;
use crate::types::relation::pair_effects::{AsyncConstraintSet, PairEffects};
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallMappingVisitors, CallRelationOwners,
};
use crate::types::relation::{
    ConstraintSetRelationFacts, DirectionalEquivalenceEffects, EquivalenceChecker,
    EquivalenceWrapperEffects, OwnedConstraintSetEffects, OwnedRelationProducerEffects,
    RelationOwners, ScalarConstraintSetEffects, TypeRelation, TypeRelationChecker,
    TypeVarEvaluation, constraint_set_assignable_owned_with, constraint_set_assignable_with,
    constraint_set_equivalent_owned_with, constraint_set_equivalent_with,
    directional_equivalence_with, owned_relation_constraints_with,
    trivially_constraint_set_assignable_with,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{IntersectionType, Type, TypePair, UnionType};
use crate::{Db, Program, ProgramEnvironment};

/// These observations report actual owners and results without supplying relation answers.
pub(in crate::types) enum ConstraintSetObservation<'c, 'db> {
    Checker {
        resources: [*const (); 6],
        relation: TypeRelation,
        evaluation: TypeVarEvaluation,
        inferable_none: bool,
        given_never: bool,
    },
    PairResult(ConstraintSet<'db, 'c>),
    AlwaysResult {
        constraints: ConstraintSet<'db, 'c>,
        result: bool,
    },
    OwnedResult(&'c Cow<'db, OwnedConstraintSet<'db>>),
}

pub(in crate::types) async fn satisfy_constraints<
    'call,
    'run: 'call,
    'a,
    'c: 'run,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    checker: &'call TypeRelationChecker<'a, 'c, 'db>,
    constraints: ConstraintSet<'db, 'c>,
    always: bool,
    queries: Q,
) -> RunResult<bool> {
    RuntimePairs::with_queries(db, endpoint.clone(), checker.constraints, queries)
        .terminal_satisfaction(checker, constraints, always)
        .await
}

pub(in crate::types) async fn assignable<
    'call,
    'run: 'call,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    environments: &'run CallEnvironments<'db>,
    builders: &'run CallBuilders<'db>,
    owners: &'run CallRelationOwners<'run, 'run, 'db>,
    queries: Q,
) -> RunResult<bool> {
    assignable_observed(
        db,
        endpoint,
        program,
        left,
        right,
        environments,
        builders,
        owners,
        queries,
        None,
    )
    .await
}

pub(in crate::types) async fn assignable_observed<
    'call,
    'run: 'call,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    environments: &'run CallEnvironments<'db>,
    builders: &'run CallBuilders<'db>,
    owners: &'run CallRelationOwners<'run, 'run, 'db>,
    queries: Q,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
) -> RunResult<bool> {
    let env: &'run ProgramEnvironment<'db> = environments.allocate(endpoint, program).await;
    let builder: &'run ConstraintSetBuilder<'db> = builders.allocate(endpoint).await;
    let owners = owners.allocate(endpoint, env, builder).await;
    let checker = owners.constraint_set_assignability();
    let effects = ScalarEffects {
        pairs: RuntimePairs::with_queries(db, endpoint.clone(), builder, queries),
        observer,
    };
    constraint_set_assignable_with(&checker, left, right, &effects).await
}

struct ScalarEffects<'call, 'run, 'db: 'run, Q> {
    pairs: RuntimePairs<'run, 'db, 'run, Q>,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
}

impl<'run, 'db: 'run, Q: ProtocolQueryAccess<'run, 'db>> ScalarConstraintSetEffects<'run, 'run, 'db>
    for ScalarEffects<'_, 'run, 'db, Q>
{
    type Error = RunError;

    async fn pair(
        &self,
        checker: &TypeRelationChecker<'run, 'run, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'run>> {
        self.pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(PairWork::HelperStep.units())?;
                self.pairs.verify_database()?;
                self.pairs.verify_builder(checker.constraints)?;
                if let Some(observer) = self.observer {
                    observer(ConstraintSetObservation::Checker {
                        resources: [
                            std::ptr::from_ref(checker.env).cast(),
                            std::ptr::from_ref(checker.constraints).cast(),
                            std::ptr::from_ref(checker.relation_visitor).cast(),
                            std::ptr::from_ref(checker.disjointness_visitor).cast(),
                            std::ptr::from_ref(checker.signature_relation_visitor).cast(),
                            std::ptr::from_ref(checker.materialization_visitor).cast(),
                        ],
                        relation: checker.relation,
                        evaluation: checker.typevar_evaluation,
                        inferable_none: checker.inferable == TypeVarSet::None,
                        given_never: checker.given.is_trivially_never_satisfied(),
                    });
                }
                Ok(())
            })
            .await;
        self.pairs.check_type_pair(checker, source, target).await
    }

    async fn always(
        &self,
        checker: &TypeRelationChecker<'run, 'run, 'db>,
        constraints: ConstraintSet<'db, 'run>,
    ) -> RunResult<bool> {
        self.pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(PairWork::HelperStep.units())?;
                self.pairs.verify_builder(checker.constraints)?;
                constraints.verify_builder(checker.constraints);
                if let Some(observer) = self.observer {
                    observer(ConstraintSetObservation::PairResult(constraints));
                }
                Ok(())
            })
            .await;
        let result = self
            .pairs
            .terminal_satisfaction(checker, constraints, true)
            .await?;
        self.pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(PairWork::Complete.units())?;
                if let Some(observer) = self.observer {
                    observer(ConstraintSetObservation::AlwaysResult {
                        constraints,
                        result,
                    });
                }
                Ok(())
            })
            .await;
        Ok(result)
    }
}

pub(in crate::types) async fn owned_assignable<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
    owned_assignable_observed(db, endpoint, program, left, right, None).await
}

pub(in crate::types) async fn owned_assignable_observed<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
    owned_assignable_with_queries_observed(
        db,
        endpoint,
        program,
        left,
        right,
        NoOwnedRelationQueries,
        observer,
    )
    .await
}

pub(in crate::types) async fn owned_assignable_with_queries_observed<
    'call,
    'run: 'call,
    'db: 'run,
    O: OwnedRelationQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    owned: O,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
    let effects = OwnedEffects {
        db,
        endpoint,
        program,
        owned,
    };
    let result =
        constraint_set_assignable_owned_with(left, right, &effects, ConstraintSetRelationFacts)
            .await?;
    endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::Complete.units())?;
            if let Some(observer) = observer {
                observer(ConstraintSetObservation::OwnedResult(&result));
            }
            Ok(())
        })
        .await;
    Ok(result)
}

struct OwnedEffects<'call, 'run, 'db, O> {
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    owned: O,
}

impl<O> OwnedEffects<'_, '_, '_, O> {
    async fn refuse<T>(&self) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                let reason = expansion_probe::refuse(
                    self.db,
                    Incomplete::UnsupportedSequentOperation(
                        UnsupportedSequentOperation::OwnedAssignable,
                    ),
                );
                Err(RunError::Refused(match reason {
                    Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                    Incomplete::RequestedAllocation => {
                        salsa::attempt_probe::Incomplete::RequestedAllocation
                    }
                    _ => salsa::attempt_probe::Incomplete::Interrupted,
                }))
            })
            .await)
    }
}

impl<'run, 'db: 'run, O: OwnedRelationQueryAccess<'run, 'db>> OwnedConstraintSetEffects<'db>
    for OwnedEffects<'_, 'run, 'db, O>
{
    type Error = RunError;

    async fn trivially_assignable(&self, source: Type<'db>, target: Type<'db>) -> RunResult<bool> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::Entry.units())?;
                salsa::attempt_probe::charge(self.db, 0).map_err(RunError::Refused)?;
                if !relation_key_has_fixed_cost((
                    source,
                    target,
                    TypeRelation::Assignability,
                    TypeVarEvaluation::Lazy,
                )) {
                    let reason = expansion_probe::refuse(
                        self.db,
                        Incomplete::UnsupportedPairOperation(UnsupportedPairOperation::GuardKey),
                    );
                    return Err(RunError::Refused(match reason {
                        Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                        Incomplete::RequestedAllocation => {
                            salsa::attempt_probe::Incomplete::RequestedAllocation
                        }
                        _ => salsa::attempt_probe::Incomplete::Interrupted,
                    }));
                }
                Ok(())
            })
            .await;
        trivially_constraint_set_assignable_with(source, target, self, ConstraintSetRelationFacts)
            .await
    }

    async fn union_contains(&self, _union: UnionType<'db>, _source: Type<'db>) -> RunResult<bool> {
        self.refuse().await
    }

    async fn intersection_contains(
        &self,
        _intersection: IntersectionType<'db>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse().await
    }

    async fn cached_owned_assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>> {
        self.owned
            .owned_assignable(self.endpoint, self.db, self.program, source, target)
            .await
    }
}

pub(in crate::types) trait OwnedRelationQueryAccess<'run, 'db: 'run>:
    Clone + 'run
{
    fn owned_assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = RunResult<&'db OwnedConstraintSet<'db>>> + 'call
    where
        'run: 'call;

    fn owned_equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = RunResult<&'db OwnedConstraintSet<'db>>> + 'call
    where
        'run: 'call;
}

#[derive(Clone, Copy)]
pub(in crate::types) struct NoOwnedRelationQueries;

async fn unavailable_owned<'call, 'run: 'call, 'db: 'run, T>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    operation: UnsupportedSequentOperation,
) -> RunResult<T> {
    Ok(endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::HelperStep.units())?;
            let reason =
                expansion_probe::refuse(db, Incomplete::UnsupportedSequentOperation(operation));
            Err(RunError::Refused(match reason {
                Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                Incomplete::RequestedAllocation => {
                    salsa::attempt_probe::Incomplete::RequestedAllocation
                }
                _ => salsa::attempt_probe::Incomplete::Interrupted,
            }))
        })
        .await)
}

impl<'run, 'db: 'run> OwnedRelationQueryAccess<'run, 'db> for NoOwnedRelationQueries {
    async fn owned_assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        _program: Program<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        unavailable_owned(db, endpoint, UnsupportedSequentOperation::OwnedAssignable).await
    }
    async fn owned_equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        _program: Program<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        unavailable_owned(db, endpoint, UnsupportedSequentOperation::OwnedEquivalent).await
    }
}

pub(in crate::types) struct OwnedRelationQueries<
    'run,
    'db: 'run,
    A: Configuration,
    E: Configuration,
    M,
> {
    pub(in crate::types) assignability: CallableRoute<'run, 'db, A>,
    pub(in crate::types) equivalence: CallableRoute<'run, 'db, E>,
    pub(in crate::types) keys: &'run InternedValues<'db, TypePair<'static>, M>,
}

impl<A: Configuration, E: Configuration, M> Clone for OwnedRelationQueries<'_, '_, A, E, M> {
    fn clone(&self) -> Self {
        Self {
            assignability: self.assignability.clone(),
            equivalence: self.equivalence.clone(),
            keys: self.keys,
        }
    }
}

impl<'run, 'db: 'run, A, E, M> OwnedRelationQueryAccess<'run, 'db>
    for OwnedRelationQueries<'run, 'db, A, E, M>
where
    A: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
    E: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
    M: PassiveMemoSchema<'db, TypePair<'static>> + 'run,
{
    async fn owned_assignable<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let key = endpoint
            .intern_value(self.keys, (program, source, target))
            .await;
        Ok(endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.assignability, key.as_id())?.await })
            .await)
    }
    async fn owned_equivalent<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let key = endpoint
            .intern_value(self.keys, (program, source, target))
            .await;
        Ok(endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.equivalence, key.as_id())?.await })
            .await)
    }
}

pub(in crate::types) enum OwnedRelationObservation<'event, 'run, 'db> {
    Constructed {
        kind: OwnedRelationKind,
        builder: &'run ConstraintSetBuilder<'db>,
        resources: [*const (); 6],
    },
    Direction {
        source: Type<'db>,
        target: Type<'db>,
        resources: [*const (); 6],
        relation: TypeRelation,
        evaluation: TypeVarEvaluation,
        materialization_guard: Option<*const ()>,
    },
    PairResult(ConstraintSet<'db, 'run>),
    Packaged(&'event OwnedConstraintSet<'db>),
    Initial,
    Recovery,
}

pub(in crate::types) struct OwnedRelationProvider<'run, 'db: 'run, Q> {
    pub(in crate::types) kind: OwnedRelationKind,
    pub(in crate::types) queries: Q,
    pub(in crate::types) environments: &'run CallEnvironments<'db>,
    pub(in crate::types) builders: &'run CallBuilders<'db>,
    pub(in crate::types) owners: &'run CallRelationOwners<'run, 'run, 'db>,
    pub(in crate::types) mappings: &'run CallMappingVisitors<'run, 'db>,
    pub(in crate::types) observer: Option<&'run dyn Fn(OwnedRelationObservation<'_, 'run, 'db>)>,
}

impl<'run, 'db: 'run, C, Q> CallableRouteProvider<'run, 'db, C>
    for OwnedRelationProvider<'run, 'db, Q>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
    Q: ProtocolQueryAccess<'run, 'db>,
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
        let work = match operation {
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => 1,
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                return Err(RunError::Contract(
                    "owned relation input requires a generated TypePair handle",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                owned_constraint_comparison_work(&endpoint, left, right).await?
            }
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
        input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let (program, source, target) = endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::Entry.units())?;
                salsa::attempt_probe::charge(db, 0).map_err(RunError::Refused)?;
                let fields = input.read_fields(salsa::FieldReads::new(db));
                Ok((*fields.program(), *fields.first(), *fields.second()))
            })
            .await;
        let env: &'run ProgramEnvironment<'db> =
            self.environments.allocate(&endpoint, program).await;
        let builder: &'run ConstraintSetBuilder<'db> = self.builders.allocate(&endpoint).await;
        let owners = self.owners.allocate(&endpoint, env, builder).await;
        let effects = ProducerEffects {
            pairs: RuntimePairs::with_queries(db, endpoint.clone(), builder, self.queries.clone()),
            owners,
            mappings: self.mappings,
            observer: self.observer,
        };
        endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::HelperStep.units())?;
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::Constructed {
                        kind: self.kind,
                        builder,
                        resources: checker_resources(&owners.constraint_set_assignability()),
                    });
                }
                Ok(())
            })
            .await;
        let result = owned_relation_constraints_with(self.kind, source, target, &effects).await?;
        endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::HelperStep.units())?;
                result.verify_builder(builder);
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::PairResult(result));
                }
                Ok(())
            })
            .await;
        // The pool retains the private builder until driver drain. Packaging a terminal borrows
        // its result and leaves that storage in place for outstanding children and guards.
        let owned = package_owned_result(db, &endpoint, builder, result).await?;
        endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::Complete.units())?;
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::Packaged(&owned));
                }
                Ok(())
            })
            .await;
        Ok(owned)
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::HelperStep.units())?;
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::Initial);
                }
                Ok(())
            })
            .await;
        Ok(OwnedConstraintSet::always())
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call OwnedConstraintSet<'db>,
        value: OwnedConstraintSet<'db>,
        _input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(PairWork::HelperStep.units())?;
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::Recovery);
                }
                Ok(())
            })
            .await;
        Ok(value)
    }
}

async fn owned_constraint_comparison_work<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    left: &OwnedConstraintSet<'db>,
    right: &OwnedConstraintSet<'db>,
) -> RunResult<usize> {
    let mut work = 0usize;
    for constraints in [left, right] {
        work = endpoint
            .local_call(|| {
                endpoint.admit_work(16)?;
                endpoint.check_completion()?;
                // Retirement reads fixed container metadata, including an upper bound on
                // support words. Eight units per entry cover native equality's scalar fields.
                constraints
                    .retirement_work()
                    .and_then(|entries| entries.checked_mul(8))
                    .and_then(|entries| work.checked_add(entries))
                    .ok_or(RunError::Contract(
                        "owned constraint comparison work overflow",
                    ))
            })
            .await;
        let mut pairs = constraints.native_comparison_type_pairs();
        while pairs.len() != 0 {
            let count = pairs.len().min(64);
            work = endpoint
                .local_call(|| {
                    endpoint.admit_work(count * 2)?;
                    endpoint.check_completion()?;
                    pairs.by_ref().take(count).try_fold(work, |work, pair| {
                        pair.into_iter().try_fold(work, |work, ty| {
                            ty.inline_payload_bytes()
                                .checked_add(1)
                                .and_then(|field| work.checked_add(field))
                                .ok_or(RunError::Contract(
                                    "owned constraint comparison work overflow",
                                ))
                        })
                    })
                })
                .await;
            endpoint.checkpoint()?.await?;
        }
    }
    Ok(work)
}

pub(super) async fn package_owned_result<'call, 'run: 'call, 'db: 'run, 'c: 'call>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    builder: &'c ConstraintSetBuilder<'db>,
    result: ConstraintSet<'db, 'c>,
) -> RunResult<OwnedConstraintSet<'db>> {
    Ok(endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::Complete.units())?;
            result.verify_builder(builder);
            result.to_owned_terminal().ok_or_else(|| {
                let reason = expansion_probe::refuse(
                    db,
                    Incomplete::UnsupportedPairOperation(UnsupportedPairOperation::OwnedCompaction),
                );
                RunError::Refused(match reason {
                    Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                    Incomplete::RequestedAllocation => {
                        salsa::attempt_probe::Incomplete::RequestedAllocation
                    }
                    _ => salsa::attempt_probe::Incomplete::Interrupted,
                })
            })
        })
        .await)
}

fn checker_resources(checker: &TypeRelationChecker<'_, '_, '_>) -> [*const (); 6] {
    [
        std::ptr::from_ref(checker.env).cast(),
        std::ptr::from_ref(checker.constraints).cast(),
        std::ptr::from_ref(checker.relation_visitor).cast(),
        std::ptr::from_ref(checker.disjointness_visitor).cast(),
        std::ptr::from_ref(checker.signature_relation_visitor).cast(),
        std::ptr::from_ref(checker.materialization_visitor).cast(),
    ]
}

struct ProducerEffects<'run, 'db: 'run, Q> {
    pairs: RuntimePairs<'run, 'db, 'run, Q>,
    owners: &'run RelationOwners<'run, 'run, 'db>,
    mappings: &'run CallMappingVisitors<'run, 'db>,
    observer: Option<&'run dyn Fn(OwnedRelationObservation<'_, 'run, 'db>)>,
}

impl<'run, 'db: 'run, Q: ProtocolQueryAccess<'run, 'db>> ProducerEffects<'run, 'db, Q> {
    async fn observe_direction(
        &self,
        checker: &TypeRelationChecker<'run, 'run, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<()> {
        self.pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(PairWork::HelperStep.units())?;
                self.pairs.verify_database()?;
                self.pairs.verify_builder(checker.constraints)?;
                if let Some(observer) = self.observer {
                    observer(OwnedRelationObservation::Direction {
                        source,
                        target,
                        resources: checker_resources(checker),
                        relation: checker.relation,
                        evaluation: checker.typevar_evaluation,
                        materialization_guard: checker
                            .materialization_visitor
                            .materialization_equivalence
                            .get()
                            .map(|guard| Rc::as_ptr(guard).cast()),
                    });
                }
                Ok(())
            })
            .await;
        Ok(())
    }
}

impl<'run, 'db: 'run, Q: ProtocolQueryAccess<'run, 'db>> OwnedRelationProducerEffects<'run, 'db>
    for ProducerEffects<'run, 'db, Q>
{
    type Error = RunError;
    async fn assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'run>> {
        let checker: TypeRelationChecker<'run, 'run, 'db> =
            self.owners.constraint_set_assignability();
        self.observe_direction(&checker, source, target).await?;
        self.pairs.check_type_pair(&checker, source, target).await
    }
    async fn equivalent(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'run>> {
        let checker: EquivalenceChecker<'run, 'run, 'db> = self.owners.constraint_set_equivalence();
        directional_equivalence_with(&checker, source, target, self).await
    }
}

impl<'run, 'db: 'run, Q: ProtocolQueryAccess<'run, 'db>>
    DirectionalEquivalenceEffects<'run, 'run, 'db> for ProducerEffects<'run, 'db, Q>
{
    type Error = RunError;
    async fn direction(
        &self,
        checker: &EquivalenceChecker<'run, 'run, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'run>> {
        let visitor = self
            .mappings
            .allocate_materialization_root(&self.pairs.endpoint, checker.materialization_visitor)
            .await;
        let relation = checker.as_relation_checker(visitor);
        self.observe_direction(&relation, source, target).await?;
        self.pairs.check_type_pair(&relation, source, target).await
    }
    async fn is_never(
        &self,
        checker: &EquivalenceChecker<'run, 'run, 'db>,
        value: ConstraintSet<'db, 'run>,
    ) -> RunResult<bool> {
        self.pairs
            .step(|| {
                value.verify_builder(checker.constraints);
                value.is_trivially_never_satisfied()
            })
            .await
    }
    async fn conjoin(
        &self,
        checker: &EquivalenceChecker<'run, 'run, 'db>,
        left: ConstraintSet<'db, 'run>,
        right: ConstraintSet<'db, 'run>,
    ) -> RunResult<ConstraintSet<'db, 'run>> {
        left.and_with(checker.constraints, || async { Ok(right) }, &self.pairs)
            .await
    }
}

async fn admit_equivalence<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    source: Type<'db>,
    target: Type<'db>,
) -> RunResult<()> {
    endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::Entry.units())?;
            salsa::attempt_probe::charge(db, 0).map_err(RunError::Refused)?;
            if !relation_key_has_fixed_cost((
                source,
                target,
                TypeRelation::Redundancy { pure: true },
                TypeVarEvaluation::Lazy,
            )) {
                let reason = expansion_probe::refuse(
                    db,
                    Incomplete::UnsupportedPairOperation(UnsupportedPairOperation::GuardKey),
                );
                return Err(RunError::Refused(match reason {
                    Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                    Incomplete::RequestedAllocation => {
                        salsa::attempt_probe::Incomplete::RequestedAllocation
                    }
                    _ => salsa::attempt_probe::Incomplete::Interrupted,
                }));
            }
            Ok(())
        })
        .await;
    Ok(())
}

pub(in crate::types) async fn owned_equivalent_observed<
    'call,
    'run: 'call,
    'db: 'run,
    O: OwnedRelationQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    owned: O,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
    admit_equivalence(db, endpoint, left, right).await?;
    let effects = EquivalenceWrapper {
        db,
        endpoint,
        program,
        owned,
        observer,
        scalar: None::<ScalarEquivalence<'run, 'db, super::protocol::NoProtocolQueries>>,
    };
    let result =
        constraint_set_equivalent_owned_with(left, right, &effects, ConstraintSetRelationFacts)
            .await?;
    endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::Complete.units())?;
            if let Some(observer) = observer {
                observer(ConstraintSetObservation::OwnedResult(&result));
            }
            Ok(())
        })
        .await;
    Ok(result)
}

pub(in crate::types) async fn equivalent_observed<
    'call,
    'run: 'call,
    'db: 'run,
    Q: ProtocolQueryAccess<'run, 'db>,
    O: OwnedRelationQueryAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    left: Type<'db>,
    right: Type<'db>,
    environments: &'run CallEnvironments<'db>,
    builders: &'run CallBuilders<'db>,
    owners: &'run CallRelationOwners<'run, 'run, 'db>,
    protocols: Q,
    owned: O,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
) -> RunResult<bool> {
    admit_equivalence(db, endpoint, left, right).await?;
    let effects = EquivalenceWrapper {
        db,
        endpoint,
        program,
        owned,
        observer,
        scalar: Some(ScalarEquivalence {
            environments,
            builders,
            owners,
            protocols,
        }),
    };
    let result =
        constraint_set_equivalent_with(left, right, &effects, ConstraintSetRelationFacts).await?;
    endpoint
        .local_call(|| {
            endpoint.admit_work(PairWork::Complete.units())?;
            Ok(())
        })
        .await;
    Ok(result)
}

struct ScalarEquivalence<'run, 'db: 'run, Q> {
    environments: &'run CallEnvironments<'db>,
    builders: &'run CallBuilders<'db>,
    owners: &'run CallRelationOwners<'run, 'run, 'db>,
    protocols: Q,
}

struct EquivalenceWrapper<'call, 'run, 'db: 'run, O, Q> {
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    program: Program<'db>,
    owned: O,
    scalar: Option<ScalarEquivalence<'run, 'db, Q>>,
    observer: Option<&'call dyn Fn(ConstraintSetObservation<'_, 'db>)>,
}

impl<'run, 'db: 'run, O: OwnedRelationQueryAccess<'run, 'db>, Q: ProtocolQueryAccess<'run, 'db>>
    EquivalenceWrapperEffects<'db> for EquivalenceWrapper<'_, 'run, 'db, O, Q>
{
    type Error = RunError;
    async fn cached_owned_equivalent(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>> {
        self.owned
            .owned_equivalent(self.endpoint, self.db, self.program, source, target)
            .await
    }
    async fn owned_always(&self, value: &'db OwnedConstraintSet<'db>) -> RunResult<bool> {
        let Some(scalar) = &self.scalar else {
            return unavailable_owned(
                self.db,
                self.endpoint,
                UnsupportedSequentOperation::Equivalent,
            )
            .await;
        };
        let env: &'run ProgramEnvironment<'db> = scalar
            .environments
            .allocate(self.endpoint, self.program)
            .await;
        let view = scalar
            .builders
            .allocate_owned_query(self.endpoint, value)
            .await;
        let (builder, constraints) = view.parts();
        let owners = scalar.owners.allocate(self.endpoint, env, builder).await;
        let checker = owners.constraint_set_assignability();
        let effects = ScalarEffects {
            pairs: RuntimePairs::with_queries(
                self.db,
                self.endpoint.clone(),
                builder,
                scalar.protocols.clone(),
            ),
            observer: self.observer,
        };
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                effects.pairs.verify_database()?;
                effects.pairs.verify_builder(builder)?;
                if let Some(observer) = self.observer {
                    observer(ConstraintSetObservation::Checker {
                        resources: checker_resources(&checker),
                        relation: checker.relation,
                        evaluation: checker.typevar_evaluation,
                        inferable_none: checker.inferable == TypeVarSet::None,
                        given_never: checker.given.is_trivially_never_satisfied(),
                    });
                }
                Ok(())
            })
            .await;
        effects.always(&checker, constraints).await
    }
}
