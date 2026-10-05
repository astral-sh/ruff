//! Controlled comparisons retain the ordinary visitor, tuple walk, and result union.

use std::borrow::Cow;

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use ty_python_core::Truthiness;

use super::{SourceAccess, SourceEffects, SourceOperation};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::comparison_guard::{self, Stage, Storage};
use crate::Db;
use crate::types::bool::BoolError;
use crate::types::constraints::control::GrowthPlan;
use crate::types::cyclic::identity::{
    IDENTITY_MODE_ADMISSION_WORK, IDENTITY_MODE_WORK, identity_mode_admission_bytes,
    identity_mode_bytes,
};
use crate::types::context::InferContext;
use crate::types::cyclic::{
    CycleDetectorLookup, CycleDetectorVisit, CycleGuardControl, HasIdentity, RelationGuardError,
    RelationGuardWork, cycle_cache_scan_slots,
};
use crate::types::equality::{ComparisonSoundnessPolicy, TupleEqualityEvaluator};
use crate::types::infer::comparisons::source::{
    self, ComparisonBranch, ComparisonEffects, ComparisonFacts, ComparisonResult,
    TypeComparisonOperation, compare_inner_with, compare_tuple_with, compare_with,
};
use crate::types::infer::comparisons::{
    BinaryComparisonVisitor, MembershipOperator, NonIdentityOperator, RichCompareOperator,
};
use crate::types::literal::{BytesLiteralType, IntLiteralType, StringLiteralType};
use crate::types::set_theoretic::builder::controlled_union::{
    UnionFacts, add_in_place_with, try_build_with,
};
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::typevar::BoundTypeVarInstance;
use crate::types::{
    DynamicType, IntersectionType, KnownClass, SubclassOfInner, Type, UnionBuilder,
};

type ComparisonKey<'db> = (Type<'db>, NonIdentityOperator, Type<'db>);

#[derive(Debug)]
enum GuardFailure {
    Runtime(RunError),
    Identity,
}

impl From<RunError> for GuardFailure {
    fn from(error: RunError) -> Self {
        Self::Runtime(error)
    }
}

/// Identifies which existing collection can grow during the current visitor operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardStorage {
    Active,
    Cache,
}

struct ComparisonGuard<'endpoint, 'run, 'db: 'run> {
    endpoint: &'endpoint TaskEndpoint<'run, 'db>,
    storage: GuardStorage,
}

// A fixed key has two finite Type payloads and an operator. Six tag/scalar operations per
// Type plus four tuple/operator operations bound hashing or equality after fixed_hash succeeds.
const KEY_WORK: usize = 16;

// Quotation is itself admitted before any checked arithmetic. Its scalar, event and result
// representations are compile-time layouts, so refusing this precharge does not evaluate a
// variable quote. Payload admissions below fund the subsequent detector operations separately.
const GUARD_QUOTE_BYTES: usize = size_of::<usize>() * 40
    + size_of::<Option<usize>>() * 40
    + size_of::<RelationGuardWork>() * 2
    + size_of::<ComparisonGuard<'static, 'static, 'static>>() * 4
    + size_of::<Result<(), RelationGuardError<GuardFailure>>>() * 4
    + size_of::<Option<(usize, usize)>>() * 4;

const GUARD_TRANSFER_BYTES: usize = BinaryComparisonVisitor::admitted_transient_bytes()
    + size_of::<ComparisonKey<'static>>() * 4
    + size_of::<<ComparisonKey<'static> as HasIdentity<'static>>::Id>() * 4
    + size_of::<ComparisonResult<'static>>() * 4;

impl ComparisonGuard<'_, '_, '_> {
    /// Quotes relocation and eventual retirement of the active stack or completed-result cache.
    /// Returns `(work, bytes)`, or `None` if quotation arithmetic overflows. The following Resource
    /// event separately admits the plan's logical allocation request.
    fn relocation_quote(&self, plan: GrowthPlan) -> Option<(usize, usize)> {
        match self.storage {
            GuardStorage::Active => {
                let units = plan
                    .relocation_units
                    .checked_mul(2)?
                    .checked_add(plan.requested_capacity)?
                    .checked_add(16)?;
                let bytes = plan
                    .relocation_units
                    .checked_mul(BinaryComparisonVisitor::admitted_active_entry_bytes())?
                    .checked_mul(2)?
                    .checked_add(plan.requested_payload_bytes)?;
                Some((units, bytes))
            }
            GuardStorage::Cache => {
                // The insert-only table's replacement capacity is bounded by twice its request.
                // Count collision-dependent rehash probes as well as slot relocation/retirement.
                let capacity = plan.requested_capacity.checked_mul(2)?;
                let slots = cycle_cache_scan_slots::<GuardFailure>(Some(capacity)).ok()?;
                let rehash = slots
                    .checked_mul(KEY_WORK + 2)?
                    .checked_add(KEY_WORK)?
                    .checked_mul(plan.requested_capacity)?;
                let units = plan
                    .relocation_units
                    .checked_mul(2)?
                    .checked_add(rehash)?
                    .checked_add(slots)?
                    .checked_add(32)?;
                let entry_bytes = size_of::<(ComparisonKey<'_>, ComparisonResult<'_>)>();
                let backing_bytes = slots.checked_mul(entry_bytes.checked_add(1)?)?;
                let rehash_bytes = plan
                    .requested_capacity
                    .checked_mul(slots)?
                    .checked_mul(size_of::<ComparisonKey<'_>>() * 2)?;
                let bytes = backing_bytes
                    .checked_mul(2)?
                    .checked_sub(plan.requested_payload_bytes)?
                    .checked_add(plan.relocation_units.checked_mul(entry_bytes)?.checked_mul(2)?)?
                    .checked_add(rehash_bytes)?;
                Some((units, bytes))
            }
        }
    }
}

fn fixed_hash(ty: Type<'_>) -> bool {
    !matches!(ty, Type::Dynamic(DynamicType::Todo(_)))
        && !matches!(ty, Type::SubclassOf(subclass) if matches!(subclass.subclass_of(), SubclassOfInner::Dynamic(DynamicType::Todo(_))))
}

fn identity_needs_fields(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::FunctionLiteral(_)
            | Type::NewTypeInstance(_)
            | Type::ProtocolInstance(_)
            | Type::TypeAlias(_)
            | Type::TypedDict(_)
            | Type::Recursive(_)
    )
}

impl<'db> CycleGuardControl<'db, ComparisonKey<'db>> for ComparisonGuard<'_, '_, 'db> {
    type Error = GuardFailure;

    fn admit(&mut self, work: RelationGuardWork) -> Result<(), GuardFailure> {
        #[cfg(test)]
        if let RelationGuardWork::Relocate { .. } = work {
            comparison_guard::relocation_started(match self.storage {
                GuardStorage::Active => Storage::Active,
                GuardStorage::Cache => Storage::Cache,
            });
        }
        // At most 32 checked arithmetic/result steps (two operations each), plus 16 fixed
        // dispatch/forwarding steps. This pays for a quote even if its payload is refused.
        self.endpoint.admit_work(80)?;
        self.endpoint.admit(ExecutionWork::Resource {
            requested_bytes: GUARD_QUOTE_BYTES,
        })?;
        self.endpoint.check_completion()?;
        let snapshots = match work {
            RelationGuardWork::KeyCheck => 0,
            RelationGuardWork::Candidate | RelationGuardWork::Identity => 2,
            RelationGuardWork::CacheAccess { .. }
            | RelationGuardWork::ExactScan { .. }
            | RelationGuardWork::CandidateScan { .. }
            | RelationGuardWork::ActivePush
            | RelationGuardWork::Finish
            | RelationGuardWork::CacheKeyScan { .. }
            | RelationGuardWork::Relocate { .. }
            | RelationGuardWork::Resource { .. } => 1,
        };
        let quote = match work {
            RelationGuardWork::CacheAccess { capacity, probes } => {
                cycle_cache_scan_slots::<GuardFailure>(capacity)
                    .ok()
                    .and_then(|slots| {
                        let units = slots
                            .checked_mul(KEY_WORK + 2)?
                            .checked_add(KEY_WORK)?
                            .checked_mul(probes)?;
                        let bytes = slots
                            .checked_mul(probes)?
                            .checked_mul(size_of::<ComparisonKey<'db>>() * 2)?;
                        Some((units, bytes))
                    })
            }
            RelationGuardWork::ExactScan { len } => {
                self.endpoint.admit_work(const { IDENTITY_MODE_ADMISSION_WORK + IDENTITY_MODE_WORK })?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: const {
                        identity_mode_admission_bytes::<Result<(), GuardFailure>>()
                            + identity_mode_bytes::<<ComparisonKey<'db> as HasIdentity<'db>>::Id>()
                    },
                })?;
                len.checked_mul(KEY_WORK + 2)
                    .and_then(|units| units.checked_add(8))
                    .zip(len.checked_mul(size_of::<ComparisonKey<'db>>() * 2))
            }
            RelationGuardWork::CandidateScan { len } => len
                .checked_mul(12)
                .and_then(|units| units.checked_add(8))
                .zip(len.checked_mul(BinaryComparisonVisitor::admitted_scan_transfer_bytes())),
            // Two finite candidate classifications (32 each), plus the existing preliminary
            // equality/refusal checks and tuple short-circuit bookkeeping (32).
            RelationGuardWork::Candidate => Some((96, 0)),
            RelationGuardWork::Identity => Some((64, 0)),
            RelationGuardWork::KeyCheck => Some((24, 0)),
            // Insertion also prepays removing the entry if the suspended comparison is dropped.
            RelationGuardWork::ActivePush => Some((48, 0)),
            RelationGuardWork::Finish => Some((128, 0)),
            RelationGuardWork::CacheKeyScan { capacity } => {
                cycle_cache_scan_slots::<GuardFailure>(capacity)
                    .ok()
                    .and_then(|slots| {
                        slots.checked_mul(12).zip(slots.checked_mul(size_of::<ComparisonKey<'db>>()))
                    })
            }
            RelationGuardWork::Relocate { plan } => self.relocation_quote(plan),
            RelationGuardWork::Resource { requested_bytes } => Some((0, requested_bytes)),
        }
        .ok_or(RunError::Contract("comparison visitor quotation overflow"))?;
        // Candidate and identity classification each have a second validation after returning.
        // KeyCheck calls the control directly; the other events validate through admit_visit.
        let units = quote
            .0
            .checked_add(snapshots * 48)
            .ok_or(RunError::Contract("comparison visitor work overflow"))?;
        let requested_bytes = quote
            .1
            .checked_add(GUARD_TRANSFER_BYTES)
            .and_then(|bytes| {
                bytes.checked_add(if snapshots == 2 {
                    BinaryComparisonVisitor::admitted_transient_bytes()
                } else {
                    0
                })
            })
            .ok_or(RunError::Contract("comparison visitor byte quotation overflow"))?;
        self.endpoint.admit_work(units)?;
        self.endpoint
            .admit(ExecutionWork::Resource { requested_bytes })?;
        self.endpoint.check_completion()?;
        #[cfg(test)]
        if let RelationGuardWork::Relocate { .. } = work {
            comparison_guard::relocation_accepted(match self.storage {
                GuardStorage::Active => Storage::Active,
                GuardStorage::Cache => Storage::Cache,
            });
        }
        Ok(())
    }

    fn key_has_fixed_cost(key: &ComparisonKey<'db>) -> bool {
        fixed_hash(key.0) && fixed_hash(key.2)
    }

    fn candidate(
        &mut self,
        db: &'db dyn Db,
        item: &ComparisonKey<'db>,
        active: &ComparisonKey<'db>,
    ) -> Result<bool, GuardFailure> {
        if (item.0 != active.0 && identity_needs_fields(item.0))
            || (item.0 == active.0
                && item.1 == active.1
                && item.2 != active.2
                && identity_needs_fields(item.2))
        {
            return Err(GuardFailure::Identity);
        }
        Ok(item.may_share_identity(db, active))
    }

    fn identity(
        &mut self,
        db: &'db dyn Db,
        item: &ComparisonKey<'db>,
    ) -> Result<<ComparisonKey<'db> as HasIdentity<'db>>::Id, GuardFailure> {
        if identity_needs_fields(item.0) || identity_needs_fields(item.2) {
            return Err(GuardFailure::Identity);
        }
        Ok(item.to_identity(db))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn compare_types(
        &self,
        context: &InferContext<'db, '_>,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
        _range: TextRange,
    ) -> RunResult<ComparisonResult<'db>> {
        let visitor = self
            // Two empty collections/RefCells, fallback/tag initialization and destruction,
            // and the three-reference effects adapter require no heap allocation.
            .local_with_fixed_transfers(
                32,
                size_of::<&Self>() * 6
                    + size_of::<ComparisonResult<'db>>() * 2
                    + size_of::<Type<'db>>() * 2,
                || BinaryComparisonVisitor::new(Ok(Type::bool_literal(true))),
            )
            .await?;
        self.type_parameter_future(|| async {
            let effects = ComparisonSourceEffects {
                source: self,
                context,
                visitor: &visitor,
            };
            compare_with(left, op, right, ComparisonFacts, &effects).await
        })
        .await?
        .await
    }
}

struct ComparisonSourceEffects<'effects, 'access, 'run, 'db: 'run, 'context, 'ast, 'visitor, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    context: &'context InferContext<'db, 'ast>,
    visitor: &'visitor BinaryComparisonVisitor<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    ComparisonSourceEffects<'_, '_, 'run, 'db, '_, '_, '_, A>
{
    async fn unavailable<T>(&self, operation: TypeComparisonOperation) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::TypeComparison(operation))
            .await
    }

    async fn guard_result<T>(
        &self,
        result: Result<T, RelationGuardError<GuardFailure>>,
    ) -> RunResult<T> {
        match result {
            Ok(result) => Ok(result),
            Err(RelationGuardError::Refused(GuardFailure::Runtime(error))) => Err(error),
            Err(
                RelationGuardError::Refused(GuardFailure::Identity)
                | RelationGuardError::UnsupportedKey,
            ) => {
                self.unavailable(TypeComparisonOperation::VisitorIdentity)
                    .await
            }
            Err(RelationGuardError::CapacityExhausted) => {
                Err(RunError::Contract("comparison visitor capacity overflow"))
            }
            Err(RelationGuardError::Changed) => Err(RunError::Contract(
                "comparison visitor changed during admission",
            )),
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ComparisonEffects<'db>
    for ComparisonSourceEffects<'_, '_, 'run, 'db, '_, '_, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(24).await
    }
    async fn identity(
        &self,
        _left: Type<'db>,
        _op: ast::CmpOp,
        _right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(TypeComparisonOperation::Identity).await
    }
    async fn recurse(
        &self,
        left: Type<'db>,
        op: NonIdentityOperator,
        right: Type<'db>,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| compare_inner_with(left, op, right, ComparisonFacts, self))
            .await?
            .await
    }
    async fn policy(&self) -> RunResult<ComparisonSoundnessPolicy> {
        let settings = self
            .source
            .access
            .analysis_settings(self.context.file())
            .await?;
        self.source
            .local(4, 0, || {
                ComparisonSoundnessPolicy::from_analysis_settings(settings)
            })
            .await
    }
    async fn tuple_spec(&self, ty: Type<'db>) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        self.source
            .tuple_spec(self.context.program_environment(), ty)
            .await
    }
    async fn tuple_comparison(
        &self,
        left: Type<'db>,
        op: RichCompareOperator,
        right: Type<'db>,
        left_spec: &TupleSpec<'db>,
        right_spec: &TupleSpec<'db>,
    ) -> RunResult<ComparisonResult<'db>> {
        #[cfg(test)]
        comparison_guard::observe(self.source.db(), self.visitor, Stage::BeforeVisit);
        let endpoint = self.source.access.endpoint();
        let lookup = self
            .source
            .local_with_fixed_transfers(32, BinaryComparisonVisitor::admitted_transient_bytes(), || {
                self.visitor.lookup_visit_admitted(
                    self.source.db(),
                    (left, NonIdentityOperator::Rich(op), right),
                    &mut ComparisonGuard {
                        endpoint,
                        storage: GuardStorage::Active,
                    },
                )
            })
            .await?;
        let lookup = self.guard_result(lookup).await?;
        let mut scope = match lookup {
            CycleDetectorLookup::Cached(cached) => return Ok(cached.into_result()),
            CycleDetectorLookup::Visit(CycleDetectorVisit::Ready(result)) => return Ok(result),
            CycleDetectorLookup::Visit(CycleDetectorVisit::Cycle(_)) => {
                return Ok(Ok(Type::bool_literal(true)));
            }
            CycleDetectorLookup::Visit(CycleDetectorVisit::Pending(scope)) => scope,
        };
        #[cfg(test)]
        comparison_guard::observe(self.source.db(), self.visitor, Stage::Pending);
        let result = self
            .source
            .type_parameter_future(|| {
                compare_tuple_with(left_spec, op, right_spec, ComparisonFacts, self)
            })
            .await?
            .await?;
        #[cfg(test)]
        comparison_guard::observe(self.source.db(), self.visitor, Stage::BeforeFinish);
        let prepared = self
            .source
            .local_with_fixed_transfers(32, BinaryComparisonVisitor::admitted_transient_bytes(), || {
                scope.prepare_finish_admitted(
                    &result,
                    &mut ComparisonGuard {
                        endpoint,
                        storage: GuardStorage::Cache,
                    },
                )
            })
            .await?;
        let prepared = self.guard_result(prepared).await?;
        #[cfg(test)]
        comparison_guard::observe(self.source.db(), self.visitor, Stage::Prepared);
        let result = self.source
            .local_with_fixed_transfers(8, 0, || {
                scope.commit_prepared_admitted(prepared, result, |left, right| left == right)
            })
            .await?
            .map_err(|_| RunError::Contract("comparison visitor changed before completion"));
        #[cfg(test)]
        if result.is_ok() {
            comparison_guard::observe(self.source.db(), self.visitor, Stage::Committed);
        }
        result
    }
    async fn membership(
        &self,
        _left: Type<'db>,
        _op: MembershipOperator,
        _right: &FixedLengthTuple<Type<'db>>,
        _policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Type<'db>> {
        self.unavailable(TypeComparisonOperation::Membership).await
    }
    async fn equality(
        &self,
        left: Type<'db>,
        op: RichCompareOperator,
        right: Type<'db>,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Truthiness> {
        match op {
            RichCompareOperator::Eq => {
                self.source
                    .equality_truthiness(self.context.program_environment(), left, right, policy)
                    .await
            }
            RichCompareOperator::Ne => {
                self.source
                    .inequality_truthiness(self.context.program_environment(), left, right, policy)
                    .await
            }
            _ => self.source.local(1, 0, || Truthiness::Ambiguous).await,
        }
    }
    async fn from_truthiness(&self, truthiness: Truthiness) -> RunResult<Type<'db>> {
        match truthiness {
            Truthiness::AlwaysTrue => self.source.local(1, 0, || Type::bool_literal(true)).await,
            Truthiness::AlwaysFalse => self.source.local(1, 0, || Type::bool_literal(false)).await,
            Truthiness::Ambiguous => self.boolean_type().await,
        }
    }
    async fn intersection_has_typevar(
        &self,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(TypeComparisonOperation::IntersectionTypeVars)
            .await
    }
    async fn same_typevar(
        &self,
        _left: BoundTypeVarInstance<'db>,
        _right: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.unavailable(TypeComparisonOperation::TypeVarIdentity)
            .await
    }
    async fn deferred(
        &self,
        branch: ComparisonBranch<'db>,
        _left: Type<'db>,
        _op: NonIdentityOperator,
        _right: Type<'db>,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        let operation = match branch {
            ComparisonBranch::EnumComplementLeft(_) | ComparisonBranch::EnumComplementRight(_) => {
                TypeComparisonOperation::EnumComplement
            }
            ComparisonBranch::UnionLeft(_) | ComparisonBranch::UnionRight(_) => {
                TypeComparisonOperation::Union
            }
            ComparisonBranch::IntersectionExpandLeft(_)
            | ComparisonBranch::IntersectionExpandRight(_)
            | ComparisonBranch::IntersectionLeft(_)
            | ComparisonBranch::IntersectionRight(_) => TypeComparisonOperation::Intersection,
            ComparisonBranch::AliasLeft | ComparisonBranch::AliasRight => {
                TypeComparisonOperation::Alias
            }
            ComparisonBranch::NewTypeLeft(_) | ComparisonBranch::NewTypeRight(_) => {
                TypeComparisonOperation::NewType
            }
            ComparisonBranch::SameTypeVar(_) | ComparisonBranch::TypeVar(_) => {
                TypeComparisonOperation::TypeVar
            }
            ComparisonBranch::ConstraintSets(_, _) => TypeComparisonOperation::ConstraintSets,
        };
        self.unavailable(operation).await
    }
    async fn integer(
        &self,
        left: IntLiteralType,
        op: NonIdentityOperator,
        right: IntLiteralType,
        left_type: Type<'db>,
        right_type: Type<'db>,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .local(6, 0, || {
                source::integer_comparison(left, op, right, left_type, right_type)
            })
            .await
    }
    async fn string(
        &self,
        _left: StringLiteralType<'db>,
        _op: NonIdentityOperator,
        _right: StringLiteralType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(TypeComparisonOperation::StringLiteral)
            .await
    }
    async fn bytes(
        &self,
        _left: BytesLiteralType<'db>,
        _op: NonIdentityOperator,
        _right: BytesLiteralType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(TypeComparisonOperation::BytesLiteral)
            .await
    }
    async fn dunder(
        &self,
        _left: Type<'db>,
        _op: NonIdentityOperator,
        _right: Type<'db>,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(TypeComparisonOperation::Dunder).await
    }
    async fn new_union(&self) -> RunResult<UnionBuilder<'db>> {
        self.source
            // Six fields, including the fixed environment clone and empty Vec; element storage
            // and its retirement are admitted by the existing union insertion/finalization path.
            .local_with_fixed_transfers(16, 0, || {
                UnionBuilder::new(self.source.db(), self.context.program_environment())
            })
            .await
    }
    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        add_in_place_with(builder, ty, UnionFacts, self.source).await
    }
    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        Ok(try_build_with(builder, UnionFacts, self.source)
            .await?
            .unwrap_or(Type::Never))
    }
    async fn new_equality(
        &self,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<TupleEqualityEvaluator<'db>> {
        self.source
            .new_tuple_equality_evaluator(self.context.program_environment(), policy)
            .await
    }
    async fn element_equality(
        &self,
        evaluator: &mut TupleEqualityEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Result<Truthiness, BoolError<'db>>> {
        let result = self
            .source
            .tuple_element_equality(evaluator, left, right)
            .await?;
        #[cfg(test)]
        super::observations::observe(
            self.source.db(),
            super::observations::Event::ComparisonRetained,
        );
        Ok(result)
    }
    async fn retire_equality(&self, evaluator: TupleEqualityEvaluator<'db>) -> RunResult<()> {
        self.source.retire_tuple_equality_evaluator(evaluator).await
    }
    async fn report_equality(&self, _error: &BoolError<'db>) -> RunResult<()> {
        self.unavailable(TypeComparisonOperation::EqualityDiagnostic)
            .await
    }
    async fn pairs<'tuple>(
        &self,
        left: &'tuple FixedLengthTuple<Type<'db>>,
        right: &'tuple FixedLengthTuple<Type<'db>>,
    ) -> RunResult<source::FixedTuplePairs<'tuple, 'db>> {
        self.source
            .local_with_fixed_transfers(
                16,
                0,
                || source::tuple_pairs(left, right),
            )
            .await
    }
    async fn next_pair(
        &self,
        pairs: &mut source::FixedTuplePairs<'_, 'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        self.source
            .local_with_fixed_transfers(12, 0, || pairs.next())
            .await
    }
    async fn variable_tuple(
        &self,
        _left: &TupleSpec<'db>,
        _op: RichCompareOperator,
        _right: &TupleSpec<'db>,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(TypeComparisonOperation::VariableTuple)
            .await
    }
    async fn boolean_type(&self) -> RunResult<Type<'db>> {
        self.source
            .access
            .known_class_instance(self.source.program, KnownClass::Bool)
            .await
    }
}
