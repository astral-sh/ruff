//! Real pair tasks with explicit operation boundaries in the canonical relation dispatcher.

use std::future::Future;
use std::ops::ControlFlow;

use salsa::execution_probe::{BorrowOrCopy, ExecutionWork, RunError, RunResult, TaskEndpoint};

use self::protocol::{
    NoProtocolQueries, ProtocolQueryAccess, ProtocolRuntimeObservations, ProtocolRuntimeSite,
};

use super::dependencies::OrdinaryDependencies;
use super::guard::{
    PendingCachedRelation, RelationGuardEffects, RelationGuardStep, RelationScope,
    with_relation_guard,
};
use super::pair::PairEvaluation;
use super::pair_effects::PairEffects;
use super::{EquivalenceChecker, TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::Db;
use crate::types::constraints::runtime::RuntimeStructural;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::cyclic::{
    PreparedRelationFinish, RelationGuardControl, RelationGuardError,
    RelationGuardWork, RelationIdentity, RelationKey, cycle_cache_scan_slots,
    relation_key_has_fixed_cost,
};
use crate::types::cyclic::identity::{
    IDENTITY_MODE_ADMISSION_WORK, IDENTITY_MODE_WORK, admit_candidate_step, field_free_candidate,
    identity_mode_admission_bytes, identity_mode_bytes,
};
use crate::types::enums::EnumComplementType;
use crate::types::instance::{
    ExplicitAnyInstanceClass, NominalClassFacts, NominalKnownClassEffects, nominal_known_class_with,
};
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
    SentinelInstance,
};
use crate::types::literal::{BytesLiteralType, EnumLiteralType, StringLiteralType};
use crate::types::type_alias::TypeAliasType;
use crate::types::typevar::{BoundTypeVarInstance, TypeVarDomain};
use crate::types::{
    BoundMethodType, BoundSuperType, CallableSignature, CallableType, ClassLiteral, ClassType,
    FunctionType, GenericAlias, IntersectionType, KnownBoundMethodType, KnownClass,
    KnownInstanceType, NewType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType,
    RecursiveType, SpecialFormType, StaticClassLiteral, SubclassOfInner, SubclassOfType, Type,
    TypeFormType, TypeGuardType, TypeIsType, TypeVarBoundOrConstraints, TypedDictType, UnionType,
};

pub(in crate::types) mod constraint_set;
pub(in crate::types) mod protocol;
mod tests;

pub(in crate::types) use super::source_operations::RelationOperation as UnsupportedPairOperation;

#[derive(Clone, Copy)]
enum PairWork {
    Entry,
    Dispatch,
    Complete,
    HelperStep,
}

impl PairWork {
    fn units(self) -> usize {
        match self {
            Self::Entry | Self::Dispatch | Self::Complete | Self::HelperStep => 1,
        }
    }
}

pub(super) struct RuntimePairs<'run, 'db: 'run, 'c, Q = NoProtocolQueries> {
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    builder: &'c ConstraintSetBuilder<'db>,
    structural: RuntimeStructural<'run, 'db>,
    observations: Option<&'run tests::Observations>,
    queries: Q,
    protocol_observations: Option<&'run ProtocolRuntimeObservations>,
}

impl<'run, 'db: 'run, 'c> RuntimePairs<'run, 'db, 'c> {
    fn new(
        db: &'db dyn Db,
        endpoint: TaskEndpoint<'run, 'db>,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> Self {
        Self::with_queries(db, endpoint, builder, NoProtocolQueries)
    }
}

impl<'run, 'db: 'run, 'c, Q: ProtocolQueryAccess<'run, 'db>> RuntimePairs<'run, 'db, 'c, Q> {
    fn with_queries(
        db: &'db dyn Db,
        endpoint: TaskEndpoint<'run, 'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        queries: Q,
    ) -> Self {
        Self {
            db,
            structural: RuntimeStructural::new(db, endpoint.clone()),
            endpoint,
            builder,
            observations: None,
            queries,
            protocol_observations: None,
        }
    }

    fn verify_database(&self) -> RunResult<()> {
        salsa::attempt_probe::charge(self.db, 0).map_err(RunError::Refused)
    }

    fn verify_builder(&self, builder: &ConstraintSetBuilder<'db>) -> RunResult<()> {
        if std::ptr::eq(self.builder, builder) {
            Ok(())
        } else {
            Err(RunError::Contract(
                "pair effect received a foreign constraint builder",
            ))
        }
    }

    fn unsupported(&self, reason: UnsupportedPairOperation) -> RunError {
        let reason = expansion_probe::refuse(self.db, Incomplete::UnsupportedPairOperation(reason));
        RunError::Refused(match reason {
            Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
            Incomplete::RequestedAllocation => {
                salsa::attempt_probe::Incomplete::RequestedAllocation
            }
            _ => salsa::attempt_probe::Incomplete::Interrupted,
        })
    }

    fn guard_error(&self, error: RelationGuardError<RunError>) -> RunError {
        match error {
            RelationGuardError::Refused(error) => error,
            RelationGuardError::Changed => {
                RunError::Contract("relation guard storage changed during admission")
            }
            RelationGuardError::UnsupportedKey => {
                self.unsupported(UnsupportedPairOperation::GuardKey)
            }
            RelationGuardError::CapacityExhausted => {
                let reason =
                    expansion_probe::refuse(self.db, Incomplete::ConstraintCapacityExhausted);
                RunError::Refused(match reason {
                    Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                    Incomplete::RequestedAllocation => {
                        salsa::attempt_probe::Incomplete::RequestedAllocation
                    }
                    _ => salsa::attempt_probe::Incomplete::Interrupted,
                })
            }
        }
    }

    async fn refuse<T>(&self, reason: UnsupportedPairOperation) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                {
                    let _operation = self
                        .observations
                        .map(|observations| observations.mark_operation(reason));
                    self.endpoint.admit_work(PairWork::HelperStep.units())?;
                }
                self.verify_database()?;
                Err(self.unsupported(reason))
            })
            .await)
    }
}

impl<'run, 'db: 'run, 'c, Q: ProtocolQueryAccess<'run, 'db>> NominalKnownClassEffects<'db>
    for RuntimePairs<'run, 'db, 'c, Q>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                self.verify_database()
            })
            .await;
        Ok(())
    }

    async fn explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> RunResult<ClassType<'db>> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .class(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn generic_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        Ok(self
            .endpoint
            .read_field(
                alias
                    .field_requests(self.endpoint.field_request_context())
                    .origin(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn static_known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &BorrowOrCopy,
            )
            .await)
    }
}

struct RuntimeGuardControl<'effect, 'run, 'db: 'run, 'c, Q>(
    &'effect RuntimePairs<'run, 'db, 'c, Q>,
);

impl<'run, 'db: 'run, 'c, Q: ProtocolQueryAccess<'run, 'db>> RelationGuardControl<'db>
    for RuntimeGuardControl<'_, 'run, 'db, 'c, Q>
{
    type Error = RunError;

    fn admit(&mut self, work: RelationGuardWork) -> RunResult<()> {
        let units = match work {
            RelationGuardWork::CacheAccess { probes, .. } => probes,
            RelationGuardWork::ExactScan { len } => {
                self.0.endpoint.admit_work(const { IDENTITY_MODE_ADMISSION_WORK + IDENTITY_MODE_WORK })?;
                self.0.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: const {
                        identity_mode_admission_bytes::<RunResult<()>>()
                            + identity_mode_bytes::<RelationIdentity<'db>>()
                    },
                })?;
                len
            }
            RelationGuardWork::CandidateScan { len } => len,
            RelationGuardWork::Candidate => {
                return admit_candidate_step::<RelationKey<'db>, RunResult<bool>>(&self.0.endpoint);
            }
            RelationGuardWork::Identity
            | RelationGuardWork::ActivePush
            | RelationGuardWork::Finish
            | RelationGuardWork::KeyCheck => 1,
            RelationGuardWork::CacheKeyScan { capacity } => cycle_cache_scan_slots::<RunError>(capacity)
                .map_err(|error| self.0.guard_error(error.into()))?,
            RelationGuardWork::Relocate { plan } => plan.relocation_units,
            RelationGuardWork::Resource { requested_bytes } => {
                return self
                    .0
                    .endpoint
                    .admit(ExecutionWork::Resource { requested_bytes });
            }
        };
        self.0.endpoint.admit_work(units)
    }

    fn candidate(
        &mut self,
        _db: &'db dyn Db,
        item: RelationKey<'db>,
        active: RelationKey<'db>,
    ) -> RunResult<bool> {
        Ok(field_free_candidate(&self.0.endpoint, &item.0, &active.0)?
            .ok_or_else(|| self.0.unsupported(UnsupportedPairOperation::GuardIdentity))?
            && field_free_candidate(&self.0.endpoint, &item.1, &active.1)?
                .ok_or_else(|| self.0.unsupported(UnsupportedPairOperation::GuardIdentity))?
            && item.2 == active.2
            && item.3 == active.3)
    }

    fn identity(
        &mut self,
        _db: &'db dyn Db,
        _item: RelationKey<'db>,
    ) -> RunResult<RelationIdentity<'db>> {
        Err(self.0.unsupported(UnsupportedPairOperation::GuardIdentity))
    }
}

impl<'run, 'a: 'run, 'db: 'run + 'c, 'c: 'run + 'a, Q: ProtocolQueryAccess<'run, 'db>>
    RelationGuardEffects<'a, 'c, 'db> for RuntimePairs<'run, 'db, 'c, Q>
{
    type Error = RunError;
    type Prepared = PreparedRelationFinish<'a, 'db, 'c>;

    async fn start(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<RelationGuardStep<'a, 'c, 'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                self.verify_database()?;
                self.verify_builder(checker.constraints)?;
                RelationGuardStep::start_with(
                    self.db,
                    checker,
                    source,
                    target,
                    &mut RuntimeGuardControl(self),
                )
                .map_err(|error| self.guard_error(error))
            })
            .await)
    }

    async fn complete(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                result.verify_builder(self.builder);
                Ok(result)
            })
            .await)
    }

    async fn child<F>(&self, work: impl FnOnce() -> F) -> RunResult<ConstraintSet<'db, 'c>>
    where
        F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
    {
        Ok(self.endpoint.child_call(work).await)
    }

    async fn prepare_finish(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        scope: &RelationScope<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<Self::Prepared> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.verify_database()?;
                self.verify_builder(checker.constraints)?;
                result.verify_builder(self.builder);
                scope
                    .prepare_finish_with(&result, &mut RuntimeGuardControl(self))
                    .map_err(|error| self.guard_error(error))
            })
            .await)
    }

    async fn commit_finish(
        &self,
        mut scope: RelationScope<'a, 'c, 'db>,
        prepared: Self::Prepared,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        match scope.commit_prepared(prepared, result) {
            Ok(result) => Ok(result),
            Err(_) => Ok(self
                .endpoint
                .local_call(|| {
                    Err(RunError::Contract(
                        "relation guard finish preparation became stale",
                    ))
                })
                .await),
        }
    }

    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        PairEffects::is_never_satisfied(self, checker, constraints).await
    }

    async fn resume(
        &self,
        pending: PendingCachedRelation<'a, 'c, 'db>,
        is_never_satisfied: bool,
    ) -> RunResult<RelationGuardStep<'a, 'c, 'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                pending
                    .resume_with(self.db, is_never_satisfied, &mut RuntimeGuardControl(self))
                    .map_err(|error| self.guard_error(error))
            })
            .await)
    }

    async fn recursive_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::recursive_type_pair_fallback(self, checker, source, target).await
    }
}

async fn evaluate_pair<'run, 'a: 'run, 'db: 'run, 'c: 'run, Q: ProtocolQueryAccess<'run, 'db>>(
    effects: RuntimePairs<'run, 'db, 'c, Q>,
    checker: TypeRelationChecker<'a, 'c, 'db>,
    source: Type<'db>,
    target: Type<'db>,
) -> RunResult<ConstraintSet<'db, 'c>> {
    let db = effects.db;
    let pair = effects
        .endpoint
        .local_call(|| {
            effects.endpoint.admit_work(PairWork::Entry.units())?;
            effects.verify_database()?;
            effects.verify_builder(checker.constraints)?;
            if !matches!(
                (checker.relation, checker.typevar_evaluation),
                (
                    TypeRelation::Assignability | TypeRelation::Subtyping,
                    TypeVarEvaluation::Eager
                ) | (
                    TypeRelation::Assignability | TypeRelation::Redundancy { pure: true },
                    TypeVarEvaluation::Lazy
                )
            ) || checker.observations.is_some()
                || checker.is_context_collection_enabled()
            {
                return Err(effects.unsupported(UnsupportedPairOperation::CheckerMode));
            }
            if matches!(source, Type::RecursiveVar(_)) || matches!(target, Type::RecursiveVar(_)) {
                return Err(effects.unsupported(UnsupportedPairOperation::Operands));
            }
            if !relation_key_has_fixed_cost((
                source,
                target,
                checker.relation,
                checker.typevar_evaluation,
            )) {
                return Err(effects.unsupported(UnsupportedPairOperation::GuardKey));
            }
            if let Some(observations) = effects.observations {
                observations.record(&checker);
            }
            if let Some(observations) = effects.protocol_observations {
                observations.record(ProtocolRuntimeSite::Pair, &checker);
            }
            Ok(
                PairEvaluation::start(db, &checker, source, target, &OrdinaryDependencies)
                    .unwrap_or_else(|never| match never {}),
            )
        })
        .await;
    effects
        .endpoint
        .local_call(|| effects.endpoint.admit_work(PairWork::Dispatch.units()))
        .await;
    let result = effects
        .endpoint
        .child_call(|| checker.check_type_pair_inner_with(source, target, &effects))
        .await;
    let result = effects
        .endpoint
        .local_call(|| {
            effects.endpoint.admit_work(PairWork::Complete.units())?;
            Ok(pair
                .finish_borrowed(db, result, &OrdinaryDependencies)
                .unwrap_or_else(|never| match never {}))
        })
        .await;
    drop(pair);
    Ok(result)
}

impl<'run, 'a: 'run, 'db: 'run, 'c: 'run, Q: ProtocolQueryAccess<'run, 'db>>
    PairEffects<'a, 'c, 'db> for RuntimePairs<'run, 'db, 'c, Q>
{
    type Error = RunError;

    async fn type_form_argument(&self, value: TypeFormType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .type_argument(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn field_default(&self, value: FieldInstance<'db>) -> RunResult<Option<Type<'db>>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .default_type(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn field_converter(
        &self,
        value: FieldInstance<'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .converter(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn method_wrapper_kind(&self, value: MethodWrapper<'db>) -> RunResult<MethodWrapperKind> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .kind(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn method_wrapper_type(&self, value: MethodWrapper<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .wrapped(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn partial_wrapped(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> RunResult<InternedType<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .wrapped(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn partial_callable(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> RunResult<CallableType<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .partial(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn interned_type(&self, value: InternedType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .inner(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn type_is_argument(&self, value: TypeIsType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .type_argument(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn type_guard_return(&self, value: TypeGuardType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .return_type(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn step<T>(&self, operation: impl FnOnce() -> T) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                self.verify_database()?;
                Ok(operation())
            })
            .await)
    }

    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.structural
            .combine_with_entry(builder, kind, left, right, || {
                self.verify_database()?;
                self.verify_builder(builder)
            })
            .await
    }

    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        self.structural
            .push_with_entry(fold, next, |fold| {
                self.verify_database()?;
                self.verify_builder(fold.builder())
            })
            .await
    }

    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.structural
            .finish_with_entry(fold, |fold| {
                self.verify_database()?;
                self.verify_builder(fold.builder())
            })
            .await
    }

    async fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let db = self.db;
        let checker = checker.clone();
        let builder = self.builder;
        let endpoint = self.endpoint.clone();
        let observations = self.observations;
        let protocol_observations = self.protocol_observations;
        let queries = self.queries.clone();
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .demand(move || {
                        let mut effects =
                            RuntimePairs::with_queries(db, endpoint, builder, queries);
                        effects.observations = observations;
                        effects.protocol_observations = protocol_observations;
                        evaluate_pair(effects, checker, source, target)
                    })?
                    .await
            })
            .await)
    }

    async fn guard<F>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> F,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
    {
        with_relation_guard(checker, source, target, work, self).await
    }

    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.terminal_satisfaction(checker, constraints, false)
            .await
    }

    async fn check_typevar_subclass_relation_to_target(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: SubclassOfType<'db>,
        _target: Type<'db>,
    ) -> RunResult<Option<ConstraintSet<'db, 'c>>> {
        self.refuse(UnsupportedPairOperation::TypeVarSubclass).await
    }

    async fn check_newtype_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: NewType<'db>,
        _target: NewType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::NewType).await
    }

    async fn check_source_union(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: UnionType<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::SourceUnion).await
    }

    async fn check_target_union(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: UnionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::TargetUnion).await
    }

    async fn check_target_intersection(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: IntersectionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::TargetIntersection)
            .await
    }

    async fn check_source_intersection(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: IntersectionType<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::SourceIntersection)
            .await
    }

    async fn check_source_typevar_bounds(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: TypeVarBoundOrConstraints<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::TypeVarBounds).await
    }

    async fn check_function_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: FunctionType<'db>,
        _target: FunctionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Function).await
    }

    async fn check_bound_method_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: BoundMethodType<'db>,
        _target: BoundMethodType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::BoundMethod).await
    }

    async fn check_known_bound_method_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: KnownBoundMethodType<'db>,
        _target: KnownBoundMethodType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::KnownBoundMethod)
            .await
    }

    async fn check_callable_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: CallableType<'db>,
        _target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Callable).await
    }

    async fn check_callable_signature_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: &CallableSignature<'db>,
        _target: &CallableSignature<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::CallableSignature)
            .await
    }

    async fn check_callable_source(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::CallableSource).await
    }

    async fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        if Q::AVAILABLE {
            self.request_direct(checker, source, target).await
        } else {
            self.refuse(UnsupportedPairOperation::Protocol).await
        }
    }

    async fn check_meta_type_satisfies_protocol(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: ProtocolInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::MetaProtocol).await
    }

    async fn check_typeddict_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: TypedDictType<'db>,
        _target: TypedDictType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::TypedDict).await
    }

    async fn check_typeddict_fallback(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: TypedDictType<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::TypedDictFallback)
            .await
    }

    async fn check_class_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: ClassType<'db>,
        _target: ClassType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Class).await
    }

    async fn check_subclassof_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: SubclassOfType<'db>,
        _target: SubclassOfType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Subclass).await
    }

    async fn check_nominal_instance_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: NominalInstanceType<'db>,
        _target: NominalInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::NominalInstance).await
    }

    async fn check_property_instance_pair(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: PropertyInstanceType<'db>,
        _target: PropertyInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::Property).await
    }

    async fn when_recursive_types_relate_by_arguments(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: RecursiveType<'db>,
        _target: RecursiveType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::RecursiveArguments)
            .await
    }

    async fn check_bound_super_pair(
        &self,
        _checker: &EquivalenceChecker<'a, 'c, 'db>,
        _source: BoundSuperType<'db>,
        _target: BoundSuperType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::BoundSuper).await
    }

    async fn recursive_type_pair_fallback(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::RecursiveFallback)
            .await
    }

    async fn implied_typevar_relation(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::ImpliedTypevarRelation)
            .await
    }

    async fn lazy_typevar_upper_constraint(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::LazyTypevarUpperConstraint)
            .await
    }

    async fn lazy_typevar_lower_constraint(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
        _source: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::LazyTypevarLowerConstraint)
            .await
    }

    async fn protocol_is_equivalent_to_object(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        if Q::AVAILABLE {
            self.queries
                .object_equivalence(&self.endpoint, protocol)
                .await
        } else {
            self.refuse(UnsupportedPairOperation::ProtocolObjectEquivalence)
                .await
        }
    }

    async fn same_typevar_occurrence(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: BoundTypeVarInstance<'db>,
        _target: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::SameTypevarOccurrence)
            .await
    }

    async fn unfold_recursive(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _recursive: RecursiveType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::UnfoldRecursive).await
    }

    async fn alias_value(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _alias: TypeAliasType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::AliasValue).await
    }

    async fn union_has_aliases(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _union: UnionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::UnionHasAliases).await
    }

    async fn expand_union_aliases(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _union: UnionType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::ExpandUnionAliases)
            .await
    }

    async fn subclass_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::SubclassInstance)
            .await
    }

    async fn nominal_has_known_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> RunResult<bool> {
        let known = nominal_known_class_with(instance, NominalClassFacts, self).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(PairWork::HelperStep.units())?;
                self.verify_database()?;
                Ok(known == Some(class))
            })
            .await)
    }

    async fn class_default_specialization(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.refuse(UnsupportedPairOperation::ClassDefaultSpecialization)
            .await
    }

    async fn class_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::ClassInstance).await
    }

    async fn known_instance_type_form_argument(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _instance: KnownInstanceType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::KnownInstanceTypeFormArgument)
            .await
    }

    async fn special_form_type_form_argument(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _form: SpecialFormType,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::SpecialFormTypeFormArgument)
            .await
    }

    async fn enum_remaining_literals(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _complement: EnumComplementType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::EnumRemainingLiterals)
            .await
    }

    async fn enum_intersection(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _complement: EnumComplementType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::EnumIntersection)
            .await
    }

    async fn same_sentinel(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: SentinelInstance<'db>,
        _target: SentinelInstance<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::SameSentinel).await
    }

    async fn wrapper_matches_nominal(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _wrapper: MethodWrapper<'db>,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::WrapperMatchesNominal)
            .await
    }

    async fn lookup_wrapped_function(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _target: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::LookupWrappedFunction)
            .await
    }

    async fn nominal_class_is_known(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _instance: NominalInstanceType<'db>,
        _class: KnownClass,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::NominalClassIsKnown)
            .await
    }

    async fn specialize_partial_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::SpecializePartialInstance)
            .await
    }

    async fn union_contains_dynamic(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _union: UnionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::UnionContainsDynamic)
            .await
    }

    async fn intersection_contains_nondivergent_dynamic(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IntersectionContainsNondivergentDynamic)
            .await
    }

    async fn union_contains_type(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _union: UnionType<'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::UnionContainsType)
            .await
    }

    async fn intersection_positive_contains(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _intersection: IntersectionType<'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IntersectionPositiveContains)
            .await
    }

    async fn intersection_contains_dynamic(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IntersectionContainsDynamic)
            .await
    }

    async fn intersection_negative_contains(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _intersection: IntersectionType<'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IntersectionNegativeContains)
            .await
    }

    async fn instance_approximation(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::InstanceApproximation)
            .await
    }

    async fn typevar_is_inferable(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::TypevarIsInferable)
            .await
    }

    async fn typevar_is_typevartuple(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::TypevarIsTypevartuple)
            .await
    }

    async fn is_exact_tuple_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IsExactTupleInstance)
            .await
    }

    async fn is_variadic_exact_tuple_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::IsVariadicExactTupleInstance)
            .await
    }

    async fn unpacked_typevartuple(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::UnpackedTypevartuple)
            .await
    }

    async fn typevar_domain(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarDomain> {
        self.refuse(UnsupportedPairOperation::TypevarDomain).await
    }

    async fn callable_is_gradual_paramspec_value(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::CallableIsGradualParamspecValue)
            .await
    }

    async fn callable_is_top_paramspec_value(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::CallableIsTopParamspecValue)
            .await
    }

    async fn callable_is_bottom_paramspec_value(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::CallableIsBottomParamspecValue)
            .await
    }

    async fn typevar_constraints(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<&'db [Type<'db>]>> {
        self.refuse(UnsupportedPairOperation::TypevarConstraints)
            .await
    }

    async fn typevar_upper_bound(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::TypevarUpperBound)
            .await
    }

    async fn typevar_bound_or_constraints(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.refuse(UnsupportedPairOperation::TypevarBoundOrConstraints)
            .await
    }

    async fn newtype_concrete_base(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _newtype: NewType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::NewtypeConcreteBase)
            .await
    }

    async fn type_is_always_falsy(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::TypeIsAlwaysFalsy)
            .await
    }

    async fn type_is_always_truthy(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedPairOperation::TypeIsAlwaysTruthy)
            .await
    }

    async fn callable_signatures(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.refuse(UnsupportedPairOperation::CallableSignatures)
            .await
    }

    async fn function_callable_signatures(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.refuse(UnsupportedPairOperation::FunctionCallableSignatures)
            .await
    }

    async fn known_class_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::KnownClassInstance)
            .await
    }

    async fn check_string_literal_nominal(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _value: StringLiteralType<'db>,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::StringLiteralNominal)
            .await
    }

    async fn check_bytes_literal_nominal(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _value: BytesLiteralType<'db>,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::BytesLiteralNominal)
            .await
    }

    async fn check_enum_instance_literal(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
        _literal: EnumLiteralType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.refuse(UnsupportedPairOperation::EnumInstanceLiteral)
            .await
    }

    async fn literal_fallback_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _source: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(UnsupportedPairOperation::LiteralFallbackInstance)
            .await
    }

    async fn callable_runtime_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.refuse(UnsupportedPairOperation::CallableRuntimeClass)
            .await
    }

    async fn subclass_inner_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.refuse(UnsupportedPairOperation::SubclassInnerClass)
            .await
    }

    async fn class_literal_metaclass_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::ClassLiteralMetaclassInstance)
            .await
    }

    async fn class_metaclass_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::ClassMetaclassInstance)
            .await
    }

    async fn subclass_metaclass_instance(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::SubclassMetaclassInstance)
            .await
    }

    async fn special_form_instance_fallback(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _form: SpecialFormType,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::SpecialFormInstanceFallback)
            .await
    }

    async fn known_instance_fallback(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _instance: KnownInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::KnownInstanceFallback)
            .await
    }

    async fn property_instance_fallback(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _property: PropertyInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedPairOperation::PropertyInstanceFallback)
            .await
    }
}
