//! Enum comparisons admit the real temporary active set and its retained backing.

use std::cell::Cell;

use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};

use super::super::storage::slots;
use super::{
    ActiveSetInsertion, equality_table_quote, equality_table_remove_quote, equality_type_key_work,
};

use crate::ProgramEnvironment;
use crate::types::equality::enum_source::{
    EnumDomainSet, EnumValueSet, EqualityEnumEffects as EqualityEnumEffectsTrait,
    PartitionedEnumComparison, enum_domain_collect_with, enum_domain_set_from_type_with,
    enum_value_from_resolved_with, enum_value_set_from_type_with, evaluate_enum_comparison_with,
    evaluate_enum_domains_with,
};
use crate::types::equality::source::EqualityOperation;
use crate::types::equality::{
    ComparisonBranch, ComparisonEvaluator, ComparisonOperator, ComparisonResult,
};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::instance::{
    NominalClassEffects, NominalClassFacts, NominalInstanceClass, nominal_class_with,
};
use crate::types::newtype::NewType;
use crate::types::tuple::TupleType;
use crate::types::type_alias::AliasResolutionStep;
use crate::types::{
    ClassType, EnumClassLiteral, EnumComplementType, EnumLiteralType, IntersectionType,
    LiteralValueType, LiteralValueTypeKind, NominalInstanceType, Type, UnionType,
};

/// Supplies enum-domain children while retaining each temporary active set's storage bounds.
struct EqualityEnumEffects<'a, 'source, 'run, 'db: 'run, A> {
    source: &'a SourceEffects<'source, 'run, 'db, A>,
    env: &'a ProgramEnvironment<'db>,
    active_backing: Cell<usize>,
    active_key_work: Cell<usize>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Selects a nominal class with the caller's environment available to environment-dependent branches.
    pub(in crate::types::infer::builder::source_definition::controlled) async fn equality_nominal_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        let effects = self
            .local_with_fixed_transfers(3, 0, || EqualityNominalClassEffects { source: self, env })
            .await?;
        self.type_parameter_future(|| nominal_class_with(instance, NominalClassFacts, &effects))
            .await?
            .await
    }

    pub(in crate::types::infer::builder) async fn equality_enum_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        // Use the ordinary comparison's environment snapshot while the child borrows the evaluator.
        let env = self
            .local_with_fixed_transfers(5, 0, || evaluator.env.clone())
            .await?;
        let effects = self
            .local_with_fixed_transfers(5, 0, || EqualityEnumEffects {
                source: self,
                env: &env,
                active_backing: Cell::new(0),
                active_key_work: Cell::new(0),
            })
            .await?;
        self.type_parameter_future(|| {
            evaluate_enum_comparison_with(evaluator, left, right, branch, operator, &effects)
        })
        .await?
        .await
    }
}

/// Supplies the program environment only to nominal-class branches that use it.
struct EqualityNominalClassEffects<'a, 'source, 'run, 'db: 'run, A> {
    source: &'a SourceEffects<'source, 'run, 'db, A>,
    env: &'a ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NominalClassEffects<'db>
    for EqualityNominalClassEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source
            .type_parameter_future(|| NominalClassEffects::checkpoint(self.source))
            .await?
            .await
    }

    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        self.source
            .type_parameter_future(|| NominalClassEffects::non_tuple_class(self.source, class))
            .await?
            .await
    }

    async fn tuple_class(&self, tuple: TupleType<'db>) -> RunResult<ClassType<'db>> {
        self.source
            .type_parameter_future(|| NominalClassEffects::tuple_class(self.source, tuple))
            .await?
            .await
    }

    async fn version_class(&self) -> RunResult<Option<ClassType<'db>>> {
        self.source
            .unavailable(SourceOperation::ClassSelection)
            .await
    }

    async fn object_class(&self) -> RunResult<ClassType<'db>> {
        self.source
            .type_parameter_future(|| self.source.environment_program(self.env))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| NominalClassEffects::object_class(self.source))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EqualityEnumEffectsTrait<'db>
    for EqualityEnumEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Each shared entry has at most twenty tag tests, short-circuit decisions, and result moves.
        // Its admitted native future and local effects separately fund the fixed value carriers.
        self.source.local_with_fixed_transfers(20, 0, || ()).await
    }

    async fn literal_value(
        &self,
        _literal: LiteralValueType<'db>,
        _enum_literal: EnumLiteralType<'db>,
    ) -> RunResult<Option<EnumValueSet<'db>>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn nominal_enum_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        let class = self
            .source
            .type_parameter_future(|| self.source.equality_nominal_class(self.env, instance))
            .await?
            .await?;
        let literal = self
            .source
            .type_parameter_future(|| self.source.equality_class_literal(class))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| self.source.enum_class_literal_source(literal))
            .await?
            .await
    }

    async fn all_members(&self, enum_class: EnumClassLiteral<'db>) -> RunResult<EnumValueSet<'db>> {
        self.source
            .local_with_fixed_transfers(4, 0, || EnumValueSet::all_members(enum_class))
            .await
    }

    async fn newtype_base(&self, _newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn complement_value(
        &self,
        _complement: EnumComplementType<'db>,
    ) -> RunResult<EnumValueSet<'db>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn union_value(
        &self,
        _union: UnionType<'db>,
        _active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<EnumValueSet<'db>>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn intersection_value(
        &self,
        _intersection: IntersectionType<'db>,
        _active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<EnumValueSet<'db>>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn has_members(&self, _value_set: &EnumValueSet<'db>) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn evaluate_domains(
        &self,
        target: Type<'db>,
        other: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.source
            .type_parameter_future(|| {
                evaluate_enum_domains_with(target, other, branch, operator, self)
            })
            .await?
            .await
    }

    async fn domain_set(&self, ty: Type<'db>) -> RunResult<Option<EnumDomainSet<'db>>> {
        self.source
            .type_parameter_future(|| enum_domain_set_from_type_with(ty, self))
            .await?
            .await
    }

    async fn compare_domains(
        &self,
        _target: EnumDomainSet<'db>,
        _other: EnumDomainSet<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<Option<ComparisonResult<'db>>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        match self
            .source
            .local_with_fixed_transfers(2, 0, || ty.alias_resolution_step())
            .await?
        {
            AliasResolutionStep::Resolved(ty) => Ok(ty),
            AliasResolutionStep::Alias(_)
            | AliasResolutionStep::Recursive(_)
            | AliasResolutionStep::UnboundRecursiveVariable => {
                self.source
                    .unavailable(SourceOperation::Equality(
                        EqualityOperation::AliasResolution,
                    ))
                    .await
            }
        }
    }

    async fn same_type(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        let work = self
            .source
            .local_with_fixed_transfers(
                32,
                size_of::<usize>() * 16 + size_of::<Option<usize>>() * 16,
                || {
                    let left = equality_type_key_work(left)
                        .ok_or(RunError::Contract("enum type equality quotation overflow"))?;
                    let right = equality_type_key_work(right)
                        .ok_or(RunError::Contract("enum type equality quotation overflow"))?;
                    Ok::<_, RunError>(left.max(right))
                },
            )
            .await??;
        self.source
            .local_with_fixed_transfers(work, size_of::<Type<'db>>() * 2, || left == right)
            .await
    }

    async fn partition(
        &self,
        _target: Type<'db>,
        _other: Type<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<Option<PartitionedEnumComparison<'db>>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn evaluate_partition(
        &self,
        _comparison: &PartitionedEnumComparison<'db>,
        _evaluator: &mut ComparisonEvaluator<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn new_domains(&self) -> RunResult<Vec<EnumValueSet<'db>>> {
        self.source.local_with_fixed_transfers(1, 0, Vec::new).await
    }

    async fn new_active_types(&self) -> RunResult<FxHashSet<Type<'db>>> {
        self.source
            .local_with_fixed_transfers(3, 0, || {
                self.active_backing.set(0);
                self.active_key_work.set(0);
                FxHashSet::default()
            })
            .await
    }

    async fn collect_domain(
        &self,
        ty: Type<'db>,
        domains: &mut Vec<EnumValueSet<'db>>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<()>> {
        self.source
            .type_parameter_future(|| enum_domain_collect_with(ty, domains, active_types, self))
            .await?
            .await
    }

    async fn retire_active_types(&self, active_types: FxHashSet<Type<'db>>) -> RunResult<()> {
        self.source
            .local_with_fixed_transfers(3, 0, || {
                drop(active_types);
                self.active_backing.set(0);
                self.active_key_work.set(0);
            })
            .await
    }

    async fn finish_domains(
        &self,
        domains: Vec<EnumValueSet<'db>>,
        collected: Option<()>,
    ) -> RunResult<Option<EnumDomainSet<'db>>> {
        self.source
            .local_with_fixed_transfers(6, 0, || {
                collected.and_then(|()| (!domains.is_empty()).then_some(EnumDomainSet { domains }))
            })
            .await
    }

    async fn value_set(
        &self,
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<EnumValueSet<'db>>> {
        self.source
            .type_parameter_future(|| enum_value_set_from_type_with(ty, active_types, self))
            .await?
            .await
    }

    async fn insert_active_type(
        &self,
        active_types: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let insertion = self
            .source
            .local_with_fixed_transfers(
                128,
                size_of::<usize>() * 56 + size_of::<Option<usize>>() * 56,
                || {
                    let len = active_types.len();
                    let capacity = active_types.capacity();
                    let previous_backing = self.active_backing.get();
                    let previous_key_work = self.active_key_work.get();
                    let incoming_work = equality_type_key_work(ty)
                        .ok_or(RunError::Contract("enum active type quotation overflow"))?;
                    let key_work = previous_key_work.max(incoming_work);
                    let (quote, backing) = equality_table_quote::<Type<'db>>(
                        len,
                        capacity,
                        previous_backing,
                        key_work,
                    )?;
                    Ok::<_, RunError>(ActiveSetInsertion {
                        work: quote.work,
                        requested_bytes: quote.bytes,
                        backing,
                        key_work,
                    })
                },
            )
            .await??;
        self.source
            .local_with_fixed_transfers(insertion.work, insertion.requested_bytes, || {
                let inserted = active_types.insert(ty);
                self.active_backing.set(
                    slots(active_types.capacity())
                        .map(|observed| self.active_backing.get().max(observed))
                        .unwrap_or(insertion.backing),
                );
                self.active_key_work.set(insertion.key_work);
                inserted
            })
            .await
    }

    async fn remove_active_type(
        &self,
        active_types: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let quote = self
            .source
            .local_with_fixed_transfers(
                32,
                size_of::<usize>() * 16 + size_of::<Option<usize>>() * 16,
                || {
                    equality_table_remove_quote::<Type<'db>>(
                        self.active_backing.get(),
                        self.active_key_work.get(),
                    )
                },
            )
            .await?;
        self.source
            .local_quoted_with_fixed_transfers(quote, || {
                active_types.remove(&ty);
            })
            .await
    }

    async fn value_from_resolved_type(
        &self,
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<EnumValueSet<'db>>> {
        self.source
            .type_parameter_future(|| enum_value_from_resolved_with(ty, active_types, self))
            .await?
            .await
    }

    async fn literal_kind(
        &self,
        literal: LiteralValueType<'db>,
    ) -> RunResult<LiteralValueTypeKind<'db>> {
        self.source
            .local_with_fixed_transfers(1, 0, || literal.kind())
            .await
    }

    async fn append_domain(
        &self,
        _domains: &mut Vec<EnumValueSet<'db>>,
        _domain: EnumValueSet<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }

    async fn collect_union_elements(
        &self,
        _union: UnionType<'db>,
        _domains: &mut Vec<EnumValueSet<'db>>,
        _active_types: &mut FxHashSet<Type<'db>>,
    ) -> RunResult<Option<()>> {
        self.source
            .unavailable(SourceOperation::Equality(EqualityOperation::EnumComparison))
            .await
    }
}
