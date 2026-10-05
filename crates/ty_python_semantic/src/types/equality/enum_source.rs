//! Enum comparison phases retain their temporary collections across child operations.

use std::convert::Infallible;

use rustc_hash::FxHashSet;

pub(in crate::types) use super::enums::{EnumDomainSet, EnumValueSet, PartitionedEnumComparison};
use super::enums::{EnumValueSetMembers, compare_enum_domains};
use super::{ComparisonBranch, ComparisonEvaluator, ComparisonOperator, ComparisonResult};
use crate::types::newtype::NewType;
use crate::types::{
    EnumClassLiteral, EnumComplementType, EnumLiteralType, IntersectionType, LiteralValueType,
    LiteralValueTypeKind, NominalInstanceType, Type, UnionType,
};
use crate::{Db, ProgramEnvironment};

pub(super) struct OrdinaryEqualityEnumEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousEqualityEnumEffects)]
    pub(in crate::types) trait EqualityEnumEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn literal_value(&self, literal: LiteralValueType<'db>, enum_literal: EnumLiteralType<'db>) -> Result<Option<EnumValueSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn nominal_enum_class(&self, instance: NominalInstanceType<'db>) -> Result<Option<EnumClassLiteral<'db>>, Self::Error>;
        #[operation(local)]
        async fn all_members(&self, enum_class: EnumClassLiteral<'db>) -> Result<EnumValueSet<'db>, Self::Error>;
        #[operation(child)]
        async fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn complement_value(&self, complement: EnumComplementType<'db>) -> Result<EnumValueSet<'db>, Self::Error>;
        #[operation(child)]
        async fn union_value(&self, union: UnionType<'db>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<EnumValueSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn intersection_value(&self, intersection: IntersectionType<'db>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<EnumValueSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn has_members(&self, value_set: &EnumValueSet<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn evaluate_domains(&self, target: Type<'db>, other: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(child)]
        async fn domain_set(&self, ty: Type<'db>) -> Result<Option<EnumDomainSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn compare_domains(&self, target: EnumDomainSet<'db>, other: EnumDomainSet<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(child)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn same_type(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn partition(&self, target: Type<'db>, other: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<PartitionedEnumComparison<'db>>, Self::Error>;
        #[operation(child)]
        async fn evaluate_partition(&self, comparison: &PartitionedEnumComparison<'db>, evaluator: &mut ComparisonEvaluator<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(local)]
        async fn new_domains(&self) -> Result<Vec<EnumValueSet<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_active_types(&self) -> Result<FxHashSet<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn collect_domain(&self, ty: Type<'db>, domains: &mut Vec<EnumValueSet<'db>>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<()>, Self::Error>;
        #[operation(local)]
        async fn retire_active_types(&self, active_types: FxHashSet<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_domains(&self, domains: Vec<EnumValueSet<'db>>, collected: Option<()>) -> Result<Option<EnumDomainSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn value_set(&self, ty: Type<'db>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<EnumValueSet<'db>>, Self::Error>;
        #[operation(local)]
        async fn insert_active_type(&self, active_types: &mut FxHashSet<Type<'db>>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn remove_active_type(&self, active_types: &mut FxHashSet<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn literal_kind(&self, literal: LiteralValueType<'db>) -> Result<LiteralValueTypeKind<'db>, Self::Error>;
        #[operation(child)]
        async fn value_from_resolved_type(&self, ty: Type<'db>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<EnumValueSet<'db>>, Self::Error>;
        #[operation(child)]
        async fn append_domain(&self, domains: &mut Vec<EnumValueSet<'db>>, domain: EnumValueSet<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn collect_union_elements(&self, union: UnionType<'db>, domains: &mut Vec<EnumValueSet<'db>>, active_types: &mut FxHashSet<Type<'db>>) -> Result<Option<()>, Self::Error>;
    }

    #[synchronous(evaluate_enum_comparison_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values(ComparisonResult::CanNarrow, ComparisonResult::Ambiguous, Type::Union)]
    pub(in crate::types) async fn evaluate_enum_comparison_with<'db, E: EqualityEnumEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>,
        target: Type<'db>,
        other: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
        effects: &E,
    ) -> Result<Option<ComparisonResult<'db>>, E::Error> {
        effects.checkpoint().await?;
        if let Some(result) = effects.evaluate_domains(target, other, branch, operator).await? {
            return Ok(Some(result));
        }

        if !matches!(effects.resolve_alias(target).await?, Type::Union(_))
            && !matches!(effects.resolve_alias(other).await?, Type::Union(_))
        {
            return Ok(None);
        }
        let Some(comparison) = effects.partition(target, other, branch, operator).await? else {
            return Ok(None);
        };
        let result = effects.evaluate_partition(&comparison, evaluator, branch, operator).await?;
        if let ComparisonResult::CanNarrow(narrowed) = result
            && effects.same_type(narrowed, effects.resolve_alias(target).await?).await?
        {
            return Ok(Some(ComparisonResult::Ambiguous));
        }
        Ok(Some(result))
    }

    #[synchronous(evaluate_enum_domains_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values()]
    pub(in crate::types) async fn evaluate_enum_domains_with<'db, E: EqualityEnumEffects<'db>>(
        target: Type<'db>,
        other: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
        effects: &E,
    ) -> Result<Option<ComparisonResult<'db>>, E::Error> {
        effects.checkpoint().await?;
        let Some(target) = effects.domain_set(target).await? else {
            return Ok(None);
        };
        let Some(other) = effects.domain_set(other).await? else {
            return Ok(None);
        };
        effects.compare_domains(target, other, branch, operator).await
    }

    #[synchronous(enum_domain_set_from_type_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enum_domain_set_from_type_with<'db, E: EqualityEnumEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Option<EnumDomainSet<'db>>, E::Error> {
        effects.checkpoint().await?;
        let mut domains = effects.new_domains().await?;
        let mut active_types = effects.new_active_types().await?;
        let collected = effects.collect_domain(ty, &mut domains, &mut active_types).await?;
        effects.retire_active_types(active_types).await?;
        effects.finish_domains(domains, collected).await
    }

    #[synchronous(enum_domain_collect_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values(Type::Union)]
    pub(in crate::types) async fn enum_domain_collect_with<'db, E: EqualityEnumEffects<'db>>(
        ty: Type<'db>,
        domains: &mut Vec<EnumValueSet<'db>>,
        active_types: &mut FxHashSet<Type<'db>>,
        effects: &E,
    ) -> Result<Option<()>, E::Error> {
        effects.checkpoint().await?;
        if let Some(domain) = effects.value_set(ty, active_types).await? {
            effects.append_domain(domains, domain).await?;
            return Ok(Some(()));
        }

        if !effects.insert_active_type(active_types, ty).await? {
            return Ok(None);
        }
        let result = match effects.resolve_alias(ty).await? {
            Type::Union(union) => effects.collect_union_elements(union, domains, active_types).await?,
            _ => None,
        };
        effects.remove_active_type(active_types, ty).await?;
        Ok(result)
    }

    #[synchronous(enum_value_set_from_type_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enum_value_set_from_type_with<'db, E: EqualityEnumEffects<'db>>(
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
        effects: &E,
    ) -> Result<Option<EnumValueSet<'db>>, E::Error> {
        effects.checkpoint().await?;
        // A cycle prevents extracting a finite enum domain, so fall back to general comparison.
        if !effects.insert_active_type(active_types, ty).await? {
            return Ok(None);
        }
        let resolved = effects.resolve_alias(ty).await?;
        let value_set = match resolved {
            Type::LiteralValue(literal) => match effects.literal_kind(literal).await? {
                LiteralValueTypeKind::Enum(_) => effects.value_from_resolved_type(resolved, active_types).await?,
                _ => None,
            },
            Type::NominalInstance(_) | Type::NewTypeInstance(_) | Type::EnumComplement(_)
                | Type::Union(_) | Type::Intersection(_) => effects.value_from_resolved_type(resolved, active_types).await?,
            _ => None,
        };
        effects.remove_active_type(active_types, ty).await?;
        Ok(value_set)
    }

    /// Extracts a nonempty enum-member domain from a type whose aliases are already resolved.
    #[synchronous(enum_value_from_resolved_sync)]
    #[capabilities(effects = EqualityEnumEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enum_value_from_resolved_with<'db, E: EqualityEnumEffects<'db>>(
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
        effects: &E,
    ) -> Result<Option<EnumValueSet<'db>>, E::Error> {
        effects.checkpoint().await?;
        let value_set = match ty {
            Type::LiteralValue(literal) => {
                let LiteralValueTypeKind::Enum(enum_literal) = effects.literal_kind(literal).await? else {
                    return Ok(None);
                };
                let Some(value_set) = effects.literal_value(literal, enum_literal).await? else {
                    return Ok(None);
                };
                value_set
            }
            Type::NominalInstance(instance) => {
                let Some(enum_class) = effects.nominal_enum_class(instance).await? else {
                    return Ok(None);
                };
                effects.all_members(enum_class).await?
            }
            Type::NewTypeInstance(newtype) => {
                let base = effects.newtype_base(newtype).await?;
                let Some(value_set) = effects.value_set(base, active_types).await? else {
                    return Ok(None);
                };
                value_set
            }
            Type::EnumComplement(complement) => effects.complement_value(complement).await?,
            Type::Union(union) => {
                let Some(value_set) = effects.union_value(union, active_types).await? else {
                    return Ok(None);
                };
                value_set
            }
            Type::Intersection(intersection) => {
                let Some(value_set) = effects.intersection_value(intersection, active_types).await? else {
                    return Ok(None);
                };
                value_set
            }
            Type::Dynamic(_) | Type::Divergent(_) | Type::Recursive(_) | Type::RecursiveVar(_)
            | Type::Never | Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_)
            | Type::Callable(_) | Type::ModuleLiteral(_) | Type::ClassLiteral(_) | Type::GenericAlias(_)
            | Type::SubclassOf(_) | Type::ProtocolInstance(_) | Type::SpecialForm(_)
            | Type::KnownInstance(_) | Type::PropertyInstance(_) | Type::SlotDescriptor(_)
            | Type::AlwaysTruthy | Type::AlwaysFalsy | Type::TypeVar(_) | Type::BoundSuper(_)
            | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeForm(_) | Type::TypedDict(_)
            | Type::TypeAlias(_) => return Ok(None),
        };
        if effects.has_members(&value_set).await? {
            Ok(Some(value_set))
        } else {
            Ok(None)
        }
    }
}

impl<'db> SynchronousEqualityEnumEffects<'db> for OrdinaryEqualityEnumEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn literal_value(
        &self,
        literal: LiteralValueType<'db>,
        enum_literal: EnumLiteralType<'db>,
    ) -> Result<Option<EnumValueSet<'db>>, Self::Error> {
        let enum_class = enum_literal.enum_class_literal(self.db);
        let Some(name) = enum_class.resolve_member(self.db, enum_literal.name(self.db)) else {
            return Ok(None);
        };
        Ok(Some(EnumValueSet {
            enum_class,
            members: EnumValueSetMembers::One {
                name,
                promotable: literal.is_promotable(),
            },
        }))
    }

    fn nominal_enum_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<EnumClassLiteral<'db>>, Self::Error> {
        Ok(instance
            .class_literal(self.db, self.env)
            .into_enum_class(self.db))
    }

    fn all_members(
        &self,
        enum_class: EnumClassLiteral<'db>,
    ) -> Result<EnumValueSet<'db>, Self::Error> {
        Ok(EnumValueSet::all_members(enum_class))
    }

    fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(newtype.concrete_base_type(self.db))
    }

    fn complement_value(
        &self,
        complement: EnumComplementType<'db>,
    ) -> Result<EnumValueSet<'db>, Self::Error> {
        Ok(EnumValueSet {
            enum_class: complement.enum_class_literal(self.db),
            members: EnumValueSetMembers::AllExcept(complement),
        })
    }

    fn union_value(
        &self,
        union: UnionType<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<EnumValueSet<'db>>, Self::Error> {
        Ok(EnumValueSet::from_union(
            self.db,
            self.env,
            union.elements(self.db),
            active_types,
        ))
    }

    fn intersection_value(
        &self,
        intersection: IntersectionType<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<EnumValueSet<'db>>, Self::Error> {
        Ok(EnumValueSet::from_intersection(
            self.db,
            self.env,
            intersection,
            active_types,
        ))
    }

    fn has_members(&self, value_set: &EnumValueSet<'db>) -> Result<bool, Self::Error> {
        Ok(value_set.member_count(self.db) > 0)
    }

    fn evaluate_domains(
        &self,
        target: Type<'db>,
        other: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Self::Error> {
        evaluate_enum_domains_sync(target, other, branch, operator, self)
    }

    fn domain_set(&self, ty: Type<'db>) -> Result<Option<EnumDomainSet<'db>>, Self::Error> {
        enum_domain_set_from_type_sync(ty, self)
    }

    fn compare_domains(
        &self,
        target: EnumDomainSet<'db>,
        other: EnumDomainSet<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Self::Error> {
        Ok(compare_enum_domains(
            self.db, self.env, target, other, branch, operator,
        ))
    }

    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }

    fn same_type(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    fn partition(
        &self,
        target: Type<'db>,
        other: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<PartitionedEnumComparison<'db>>, Self::Error> {
        Ok(PartitionedEnumComparison::from_unions(
            self.db, self.env, target, other, branch, operator,
        ))
    }

    fn evaluate_partition(
        &self,
        comparison: &PartitionedEnumComparison<'db>,
        evaluator: &mut ComparisonEvaluator<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Self::Error> {
        Ok(comparison.evaluate(evaluator, branch, operator))
    }

    fn new_domains(&self) -> Result<Vec<EnumValueSet<'db>>, Self::Error> {
        Ok(Vec::new())
    }

    fn new_active_types(&self) -> Result<FxHashSet<Type<'db>>, Self::Error> {
        Ok(FxHashSet::default())
    }

    fn collect_domain(
        &self,
        ty: Type<'db>,
        domains: &mut Vec<EnumValueSet<'db>>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<()>, Self::Error> {
        enum_domain_collect_sync(ty, domains, active_types, self)
    }

    fn retire_active_types(&self, active_types: FxHashSet<Type<'db>>) -> Result<(), Self::Error> {
        drop(active_types);
        Ok(())
    }

    fn finish_domains(
        &self,
        domains: Vec<EnumValueSet<'db>>,
        collected: Option<()>,
    ) -> Result<Option<EnumDomainSet<'db>>, Self::Error> {
        Ok(collected.and_then(|()| (!domains.is_empty()).then_some(EnumDomainSet { domains })))
    }

    fn value_set(
        &self,
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<EnumValueSet<'db>>, Self::Error> {
        enum_value_set_from_type_sync(ty, active_types, self)
    }

    fn insert_active_type(
        &self,
        active_types: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(active_types.insert(ty))
    }

    fn remove_active_type(
        &self,
        active_types: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        active_types.remove(&ty);
        Ok(())
    }

    fn value_from_resolved_type(
        &self,
        ty: Type<'db>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<EnumValueSet<'db>>, Self::Error> {
        Ok(EnumValueSet::from_resolved_type(
            self.db,
            self.env,
            ty,
            active_types,
        ))
    }

    fn literal_kind(
        &self,
        literal: LiteralValueType<'db>,
    ) -> Result<LiteralValueTypeKind<'db>, Self::Error> {
        Ok(literal.kind())
    }

    fn append_domain(
        &self,
        domains: &mut Vec<EnumValueSet<'db>>,
        domain: EnumValueSet<'db>,
    ) -> Result<(), Self::Error> {
        domains.push(domain);
        Ok(())
    }

    fn collect_union_elements(
        &self,
        union: UnionType<'db>,
        domains: &mut Vec<EnumValueSet<'db>>,
        active_types: &mut FxHashSet<Type<'db>>,
    ) -> Result<Option<()>, Self::Error> {
        for element in union.elements(self.db) {
            if enum_domain_collect_sync(*element, domains, active_types, self)?.is_none() {
                return Ok(None);
            }
        }
        Ok(Some(()))
    }
}
