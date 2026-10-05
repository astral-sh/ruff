//! Equality reuses the ordinary evaluator and admits its active-set storage before mutation.

use std::borrow::Cow;

use salsa::execution_probe::{RunError, RunResult};

use super::storage::{StorageQuote, slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::place::PlaceAndQualifiers;
use crate::types::bool::BoolError;
use crate::types::equality::nominal_source::NominalTuplePairs;
use crate::types::equality::source::{self, EqualityEffects, EqualityFacts, EqualityOperation};
use crate::types::equality::{
    ComparisonBranch, ComparisonEvaluator, ComparisonKey, ComparisonOperator, ComparisonResult,
    ComparisonSoundnessPolicy, KnownComparisonSemantics, TupleEqualityEvaluator,
};
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::literal::{BytesLiteralType, StringLiteralType};
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::type_alias::AliasResolutionStep;
use crate::types::{
    ClassLiteral, KnownClass, LiteralValueTypeKind, NominalInstanceType, Truthiness, Type,
};

use crate::types::{DynamicType, SubclassOfInner};

mod enums;
mod nominal;

/// Admitted insertion resources and the bounds retained with the mutated active set.
#[derive(Clone, Copy, Debug)]
struct ActiveSetInsertion {
    work: usize,
    requested_bytes: usize,
    backing: usize,
    key_work: usize,
}

/// Returns the extra text traversal required by a stored dynamic type's hash or equality.
const fn equality_dynamic_text_work(dynamic: DynamicType<'_>) -> usize {
    match dynamic {
        DynamicType::Todo(todo) => {
            #[cfg(debug_assertions)]
            {
                todo.0.len()
            }
            #[cfg(not(debug_assertions))]
            {
                let _ = todo;
                0
            }
        }
        DynamicType::Any
        | DynamicType::Unknown
        | DynamicType::UnknownGeneric(_)
        | DynamicType::UnspecializedTypeVar
        | DynamicType::UnknownLambdaParameter
        | DynamicType::InvalidConcatenateUnknown
        | DynamicType::AmbiguousOverload => 0,
    }
}

/// Bounds hashing or equality of one stored type, including debug-only TODO messages.
const fn equality_type_key_work(ty: Type<'_>) -> Option<usize> {
    let string_work = match ty {
        Type::Dynamic(dynamic) => equality_dynamic_text_work(dynamic),
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Dynamic(dynamic) => equality_dynamic_text_work(dynamic),
            SubclassOfInner::Class(_)
            | SubclassOfInner::Protocol(_)
            | SubclassOfInner::TypeVar(_) => 0,
        },
        Type::Divergent(_)
        | Type::Recursive(_)
        | Type::RecursiveVar(_)
        | Type::Never
        | Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::DataclassDecorator(_)
        | Type::DataclassTransformer(_)
        | Type::Callable(_)
        | Type::ModuleLiteral(_)
        | Type::ClassLiteral(_)
        | Type::GenericAlias(_)
        | Type::NominalInstance(_)
        | Type::ProtocolInstance(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::SlotDescriptor(_)
        | Type::Union(_)
        | Type::Intersection(_)
        | Type::EnumComplement(_)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::LiteralValue(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_)
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypeForm(_)
        | Type::TypedDict(_)
        | Type::TypeAlias(_)
        | Type::NewTypeInstance(_) => 0,
    };
    // The listed representations need at most six tag/scalar visits. A debug TODO
    // additionally hashes or compares its text. New variants must reconsider this bound;
    // representation widths contribute bytes only.
    6usize.checked_add(string_work)
}

/// Quotes one set insertion, collision-dependent rehashing and retained backing disposal.
/// `key_work` bounds every key retained in this owner, including the incoming key.
/// `previous` is the owner's retained backing bound. The second return value is the
/// promised backing bound to retain after insertion, distinct from requested bytes or entry capacity.
fn equality_table_quote<K>(
    len: usize,
    capacity: usize,
    previous: usize,
    key_work: usize,
) -> RunResult<(StorageQuote, usize)> {
    let quote = || {
        let old = slots(capacity)?.max(previous);
        let (mut quote, backing) = table_merge::<K>(len, capacity, 1, previous)?;
        let grows = quote.bytes != 0;
        let comparisons = old.checked_add(1)?;
        let query = comparisons
            .checked_mul(key_work.checked_add(2)?)?
            .checked_add(key_work)?;
        quote.work = quote.work.checked_add(query)?.checked_add(16)?;
        quote.bytes = quote
            .bytes
            .checked_add(comparisons.checked_mul(size_of::<&K>().checked_mul(2)?)?)?;
        if grows {
            let rehash_comparisons = len.checked_mul(backing)?;
            let rehash = rehash_comparisons
                .checked_mul(key_work.checked_add(2)?)?
                .checked_add(len.checked_mul(key_work)?)?;
            quote.work = quote.work.checked_add(rehash)?.checked_add(backing)?;
            quote.bytes = quote
                .bytes
                .checked_add(len.checked_mul(size_of::<K>().checked_mul(2)?)?)?
                .checked_add(rehash_comparisons.checked_mul(size_of::<&K>().checked_mul(2)?)?)?;
        }
        Some((quote, backing))
    };
    quote().ok_or(RunError::Contract(
        "equality active-set insertion quotation overflow",
    ))
}

/// Quotes removal from a retained set without changing its backing or key bound.
/// Returns `(logical work, requested bytes)`, or a contract error on arithmetic overflow.
fn equality_table_remove_quote<K>(backing: usize, key_work: usize) -> RunResult<(usize, usize)> {
    let quote = || {
        let comparisons = backing.checked_add(1)?;
        let work = comparisons
            .checked_mul(key_work.checked_add(2)?)?
            .checked_add(key_work)?
            .checked_add(4)?;
        let bytes = comparisons.checked_mul(size_of::<&K>().checked_mul(2)?)?;
        Some((work, bytes))
    };
    quote().ok_or(RunError::Contract(
        "equality active-set removal quotation overflow",
    ))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn new_tuple_equality_evaluator(
        &self,
        env: &ProgramEnvironment<'db>,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<TupleEqualityEvaluator<'db>> {
        // Construction copies the environment's Cell and initializes an empty set; no backing
        // is allocated until insertion. The fixed quote retains the complete evaluator owner.
        self.local_with_fixed_transfers(12, size_of::<TupleEqualityEvaluator<'db>>() * 2, || {
            TupleEqualityEvaluator::new(self.db(), env, policy)
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn tuple_element_equality(
        &self,
        evaluator: &mut TupleEqualityEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Result<Truthiness, BoolError<'db>>> {
        let effects = self
            .local_with_fixed_transfers(2, 0, || EqualitySourceEffects { source: self })
            .await?;
        self.type_parameter_future(|| {
            source::tuple_element_with(evaluator, left, right, EqualityFacts, &effects)
        })
        .await?
        .await
    }

    pub(in crate::types::infer::builder) async fn equality_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Truthiness> {
        self.comparison_truthiness(env, left, right, ComparisonOperator::Equality, policy)
            .await
    }

    pub(in crate::types::infer::builder) async fn inequality_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Truthiness> {
        self.comparison_truthiness(env, left, right, ComparisonOperator::Inequality, policy)
            .await
    }

    async fn comparison_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Truthiness> {
        let mut evaluator = self.new_tuple_equality_evaluator(env, policy).await?;
        let effects = self
            .local_with_fixed_transfers(2, 0, || EqualitySourceEffects { source: self })
            .await?;
        let result = self
            .type_parameter_future(|| {
                source::comparison_truthiness_with(
                    &mut evaluator.evaluator,
                    left,
                    right,
                    operator,
                    &effects,
                )
            })
            .await?
            .await?;
        self.retire_tuple_equality_evaluator(evaluator).await?;
        Ok(result)
    }

    pub(in crate::types::infer::builder) async fn retire_tuple_equality_evaluator(
        &self,
        evaluator: TupleEqualityEvaluator<'db>,
    ) -> RunResult<()> {
        // Each growth pays for releasing its backing, including refusal during a later element.
        self.local_with_fixed_transfers(1, 0, || drop(evaluator))
            .await
    }
}

struct EqualitySourceEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EqualitySourceEffects<'_, '_, 'run, 'db, A> {
    async fn unavailable<T>(&self, operation: EqualityOperation) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::Equality(operation))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EqualityEffects<'db>
    for EqualitySourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn clone_environment(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
    ) -> RunResult<ProgramEnvironment<'db>> {
        self.source
            .local_with_fixed_transfers(3, size_of::<ProgramEnvironment<'db>>(), || {
                evaluator.env.clone()
            })
            .await
    }

    async fn alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        match self
            .source
            .local(2, 0, || ty.alias_resolution_step())
            .await?
        {
            AliasResolutionStep::Resolved(ty) => Ok(ty),
            AliasResolutionStep::Alias(_)
            | AliasResolutionStep::Recursive(_)
            | AliasResolutionStep::UnboundRecursiveVariable => {
                self.unavailable(EqualityOperation::AliasResolution).await
            }
        }
    }

    async fn insert_active(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        key: ComparisonKey<'db>,
    ) -> RunResult<bool> {
        let quote = self
            .source
            .local_with_fixed_transfers(
                128,
                size_of::<usize>() * 56 + size_of::<Option<usize>>() * 56,
                || {
                    let (left, right) = key.operands();
                    let key_work = equality_type_key_work(left)
                        .and_then(|left| left.checked_add(equality_type_key_work(right)?))
                        .and_then(|work| work.checked_add(6))
                        .ok_or(RunError::Contract("equality key quotation overflow"))?
                        .max(evaluator.source_active_key_work);
                    equality_table_quote::<ComparisonKey<'db>>(
                        evaluator.active.len(),
                        evaluator.active.capacity(),
                        evaluator.source_active_backing,
                        key_work,
                    )
                    .map(|(quote, backing)| ActiveSetInsertion {
                        work: quote.work,
                        requested_bytes: quote.bytes,
                        backing,
                        key_work,
                    })
                },
            )
            .await?;
        let insertion = quote?;
        self.source
            .local_with_fixed_transfers(insertion.work, insertion.requested_bytes, || {
                let inserted = evaluator.active.insert(key);
                evaluator.source_active_backing = slots(evaluator.active.capacity())
                    .map(|observed| evaluator.source_active_backing.max(observed))
                    .unwrap_or(insertion.backing);
                evaluator.source_active_key_work = insertion.key_work;
                inserted
            })
            .await
    }

    async fn remove_active(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        key: ComparisonKey<'db>,
    ) -> RunResult<()> {
        let quote = self
            .source
            .local_with_fixed_transfers(
                32,
                size_of::<usize>() * 16 + size_of::<Option<usize>>() * 16,
                || {
                    equality_table_remove_quote::<ComparisonKey<'db>>(
                        evaluator.source_active_backing,
                        evaluator.source_active_key_work,
                    )
                },
            )
            .await?;
        self.source
            .local_quoted_with_fixed_transfers(quote, || {
                evaluator.active.remove(&key);
            })
            .await
    }

    async fn evaluate(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| {
                source::evaluate_with(
                    evaluator,
                    left,
                    right,
                    branch,
                    operator,
                    EqualityFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn evaluate_once(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| {
                source::evaluate_once_with(evaluator, left, right, branch, operator, self)
            })
            .await?
            .await
    }

    async fn enum_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.source
            .equality_enum_comparison(evaluator, left, right, branch, operator)
            .await
    }

    async fn dynamic_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.source
            .type_parameter_future(|| {
                source::dynamic_with(
                    evaluator,
                    left,
                    right,
                    branch,
                    operator,
                    EqualityFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn dynamic_constraint(
        &self,
        _evaluator: &mut ComparisonEvaluator<'db>,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.unavailable(EqualityOperation::DynamicComparison).await
    }

    async fn finite_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.source
            .type_parameter_future(|| {
                source::finite_with(evaluator, left, right, branch, operator, self)
            })
            .await?
            .await
    }

    async fn finite_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        self.source
            .type_parameter_future(|| source::finite_alternatives_with(env, ty, operator, self))
            .await?
            .await
    }

    async fn finite_alternatives_other(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        self.source
            .type_parameter_future(|| {
                source::finite_alternatives_other_with(env, ty, operator, EqualityFacts, self)
            })
            .await?
            .await
    }

    async fn union_left(
        &self,
        _evaluator: &mut ComparisonEvaluator<'db>,
        _alternatives: Vec<Type<'db>>,
        _right: Type<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(EqualityOperation::FiniteComparison).await
    }

    async fn union_right(
        &self,
        _evaluator: &mut ComparisonEvaluator<'db>,
        _left: Type<'db>,
        _alternatives: Vec<Type<'db>>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(EqualityOperation::FiniteComparison).await
    }

    async fn structural_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| {
                source::structural_with(
                    evaluator,
                    left,
                    right,
                    branch,
                    operator,
                    EqualityFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn structural_other(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| {
                source::structural_other_with(
                    evaluator,
                    env,
                    left,
                    right,
                    branch,
                    operator,
                    EqualityFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn literal_equality(
        &self,
        env: &ProgramEnvironment<'db>,
        left: LiteralValueTypeKind<'db>,
        right: LiteralValueTypeKind<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<bool>> {
        self.source
            .type_parameter_future(|| {
                source::literal_equality_with(env, left, right, operator, EqualityFacts, self)
            })
            .await?
            .await
    }

    async fn literal_equality_other(
        &self,
        _env: &ProgramEnvironment<'db>,
        _left: LiteralValueTypeKind<'db>,
        _right: LiteralValueTypeKind<'db>,
        _operator: ComparisonOperator,
    ) -> RunResult<Option<bool>> {
        self.unavailable(EqualityOperation::LiteralEquality).await
    }

    async fn string_equality(
        &self,
        left: StringLiteralType<'db>,
        right: StringLiteralType<'db>,
    ) -> RunResult<bool> {
        let (left, right) = self
            .source
            .local(2, 0, || {
                (left.value(self.source.db()), right.value(self.source.db()))
            })
            .await?;
        let work = left
            .len()
            .min(right.len())
            .checked_add(1)
            .ok_or(RunError::Contract("string equality quotation overflow"))?;
        self.source.local(work, 0, || left == right).await
    }

    async fn bytes_equality(
        &self,
        left: BytesLiteralType<'db>,
        right: BytesLiteralType<'db>,
    ) -> RunResult<bool> {
        let (left, right) = self
            .source
            .local(2, 0, || {
                (left.value(self.source.db()), right.value(self.source.db()))
            })
            .await?;
        let work = left
            .len()
            .min(right.len())
            .checked_add(1)
            .ok_or(RunError::Contract("bytes equality quotation overflow"))?;
        self.source.local(work, 0, || left == right).await
    }

    async fn narrow_literals(
        &self,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
        _left_literal: LiteralValueTypeKind<'db>,
        _right_literal: LiteralValueTypeKind<'db>,
        _positive: bool,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(EqualityOperation::LiteralNarrowing).await
    }

    async fn singleton(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| source::singleton_with(evaluator, ty, EqualityFacts, self))
            .await?
            .await
    }

    async fn singleton_other(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::singleton_type(self, &evaluator.env, ty).await
    }

    async fn comparison_semantics(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        self.source
            .type_parameter_future(|| {
                source::comparison_semantics_with(evaluator, ty, operator, EqualityFacts, self)
            })
            .await?
            .await
    }

    async fn comparison_semantics_other(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        EqualitySourceEffects::known_semantics(
            self,
            &evaluator.env,
            ty,
            operator,
            evaluator.soundness_policy,
        )
        .await
    }

    async fn tuple_equality(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Truthiness> {
        self.source
            .type_parameter_future(|| {
                source::tuple_equality_with(evaluator, left, right, EqualityFacts, self)
            })
            .await?
            .await
    }

    async fn dunder_equality(
        &self,
        _evaluator: &ComparisonEvaluator<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(EqualityOperation::DunderCall).await
    }

    async fn try_bool(
        &self,
        _evaluator: &ComparisonEvaluator<'db>,
        _ty: Type<'db>,
    ) -> RunResult<Result<Truthiness, BoolError<'db>>> {
        self.unavailable(EqualityOperation::DunderTruthiness).await
    }

    async fn nominal_checkpoint(&self) -> RunResult<()> {
        EqualitySourceEffects::nominal_checkpoint(self).await
    }

    async fn known_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        EqualitySourceEffects::known_semantics(self, env, ty, operator, policy).await
    }

    async fn instance_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        EqualitySourceEffects::instance_semantics(self, env, ty, operator).await
    }

    async fn known_semantics_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        EqualitySourceEffects::known_semantics_specialized(self, env, ty, operator, policy).await
    }

    async fn nominal_is_final(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::nominal_is_final(self, env, instance).await
    }

    async fn nominal_has_known_class(
        &self,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> RunResult<bool> {
        EqualitySourceEffects::nominal_has_known_class(self, instance, class).await
    }

    async fn nominal_class_available(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::nominal_class_available(self, env, ty).await
    }

    async fn equality_meta_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        EqualitySourceEffects::equality_meta_type(self, env, ty).await
    }

    async fn equality_dunder(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        name: &'static str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        EqualitySourceEffects::equality_dunder(self, env, ty, name).await
    }

    async fn equality_known_class(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        EqualitySourceEffects::equality_known_class(self, env, class).await
    }

    async fn same_member_implementation(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::same_member_implementation(self, left, right).await
    }

    async fn function_identity(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<FunctionLiteral<'db>> {
        EqualitySourceEffects::function_identity(self, function).await
    }

    async fn next_builtin_semantics(
        &self,
        index: &mut usize,
    ) -> RunResult<Option<(KnownClass, KnownComparisonSemantics)>> {
        EqualitySourceEffects::next_builtin_semantics(self, index).await
    }

    async fn identity_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<bool> {
        EqualitySourceEffects::identity_semantics(self, env, ty, operator).await
    }

    async fn metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        EqualitySourceEffects::metaclass_instance(self, env, class).await
    }

    async fn singleton_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::singleton_type(self, env, ty).await
    }

    async fn singleton_nominal(&self, instance: NominalInstanceType<'db>) -> RunResult<bool> {
        EqualitySourceEffects::singleton_nominal(self, instance).await
    }

    async fn singleton_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::singleton_specialized(self, env, ty).await
    }

    async fn finite_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        EqualitySourceEffects::finite_specialized(self, env, ty, operator).await
    }

    async fn boolean_alternatives(&self) -> RunResult<Vec<Type<'db>>> {
        EqualitySourceEffects::boolean_alternatives(self).await
    }

    async fn enum_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        EqualitySourceEffects::enum_alternatives(self, env, instance).await
    }

    async fn structural_specialized(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        EqualitySourceEffects::structural_specialized(
            self, evaluator, env, left, right, branch, operator,
        )
        .await
    }

    async fn excluded_string_literal(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::excluded_string_literal(self, env, left, right).await
    }

    async fn same_module(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        EqualitySourceEffects::same_module(self, left, right).await
    }

    async fn bound_method_identity(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        EqualitySourceEffects::bound_method_identity(self, left, right).await
    }

    async fn equality_equivalent(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::equality_equivalent(self, env, left, right).await
    }

    async fn equality_disjoint(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::equality_disjoint(self, env, left, right).await
    }

    async fn different_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        EqualitySourceEffects::different_semantics(self, env, left, right, operator).await
    }

    async fn nominal_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: NominalInstanceType<'db>,
        right: NominalInstanceType<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        EqualitySourceEffects::nominal_comparison(self, evaluator, left, right, operator).await
    }

    async fn equality_tuple_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        EqualitySourceEffects::equality_tuple_spec(self, env, instance).await
    }

    async fn equality_tuple_pairs<'tuple>(
        &self,
        left: &'tuple FixedLengthTuple<Type<'db>>,
        right: &'tuple FixedLengthTuple<Type<'db>>,
    ) -> RunResult<NominalTuplePairs<'tuple, 'db>> {
        EqualitySourceEffects::equality_tuple_pairs(self, left, right).await
    }

    async fn next_equality_tuple_pair(
        &self,
        pairs: &mut NominalTuplePairs<'_, 'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        EqualitySourceEffects::next_equality_tuple_pair(self, pairs).await
    }
    async fn types_equal(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        EqualitySourceEffects::types_equal(self, left, right).await
    }

    async fn members_equal(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> RunResult<bool> {
        EqualitySourceEffects::members_equal(self, left, right).await
    }
}
