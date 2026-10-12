//! Deferred mappings on aliases. Each step acts on a closed, specialized unfolding;
//! recursive references retain the step without rebuilding the recursive constructor.
//!
//! Each retained step has a separate transformation cache; equivalence checks share their
//! recursion guard.

use super::constraints::resolution::Dependencies;
use super::generics::ApplySpecialization;
use super::visitor::{DynamicContentMode, dynamic_content_impl};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, MaterializationKind, PromotionKind, Type,
    TypeContext, TypeMapping,
};
use crate::Db;

impl<'db> TypeMapping<'db> {
    fn is_idempotent(&self) -> bool {
        matches!(self, Self::Promote(..) | Self::ReplaceParameterDefaults)
    }

    /// Remove redundant singleton widening, which adds Unknown without changing variable bindings.
    pub(super) fn simplify_mapped_alias(
        &self,
        db: &'db dyn Db,
        original: Type<'db>,
        mapped: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if matches!(self, Self::Promote(_, PromotionKind::SingletonsOnly))
            && visitor
                .materialization_equivalence()
                .visit(db, (original, mapped), || {
                    original.is_equivalent_to_with_materialization_visitor(db, mapped, visitor)
                })
        {
            original
        } else {
            mapped
        }
    }

    /// Whether an alias needs to retain this mapping after simplifying substitutions.
    pub(super) fn changes_alias(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        if let Self::RescopeReturnCallables(replacements) = self
            && replacements.is_empty()
        {
            return false;
        }
        if let Self::ApplySpecialization(specialization)
        | Self::ApplySpecializationWithMaterialization { specialization, .. } = self
            && !specialization.materializes_alias_arguments(db)
        {
            // Substitutions only act on type variables. Checking both polarities also covers
            // materialization of replacements inside callable parameters.
            return !Dependencies::check(db, visitor.env, ty, |variable| {
                let ty = Type::TypeVar(variable);
                ty.apply_type_mapping_impl(db, self, context, visitor) == ty
                    && ty.apply_type_mapping_impl(db, &self.flip(), context, visitor) == ty
            });
        }
        match self {
            Self::Materialize(_) => {
                !dynamic_content_impl(db, visitor.env, ty, DynamicContentMode::Materialization)
                    .is_absent()
            }
            _ => true,
        }
    }
}

/// An ordered sequence of mappings applied after the alias's own specialization.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DeferredTypeMapping<'db> {
    #[returns(copy)]
    previous: Option<DeferredTypeMapping<'db>>,
    #[returns(ref)]
    step: MappingStep<'db>,
}

/// A retained operation and the inference context under which it was requested.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct MappingStep<'db> {
    operation: TypeMapping<'db>,
    context: TypeContext<'db>,
    materialize_bounds: bool,
}

impl<'db> MappingStep<'db> {
    fn absorbs(
        &self,
        db: &'db dyn Db,
        later: &Self,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        self.context == later.context
            && self.materialize_bounds == later.materialize_bounds
            && (matches!(
                (&self.operation, &later.operation),
                (TypeMapping::Materialize(_), TypeMapping::Materialize(_))
            ) || self.operation == later.operation && self.operation.is_idempotent()
                || match (&self.operation, &later.operation) {
                    (
                        TypeMapping::ApplySpecialization(earlier),
                        TypeMapping::ApplySpecialization(later),
                    ) => earlier.absorbs(db, later, visitor),
                    _ => false,
                })
    }

    fn commutes_with(&self, other: &Self) -> bool {
        // Other promotion kinds can introduce Unknown or dynamic default arguments.
        self.context == other.context
            && self.materialize_bounds == other.materialize_bounds
            && matches!(
                (&self.operation, &other.operation),
                (
                    TypeMapping::Promote(_, PromotionKind::Regular),
                    TypeMapping::Promote(_, PromotionKind::Regular) | TypeMapping::Materialize(_)
                ) | (
                    TypeMapping::Materialize(_),
                    TypeMapping::Promote(_, PromotionKind::Regular)
                )
            )
    }

    /// Whether this step preserves a fixed point of `other`.
    /// Singleton widening only introduces Unknown, so P(S(P(T))) = S(P(T)) for regular promotion P.
    fn preserves_fixed_points_of(&self, other: &Self) -> bool {
        self.commutes_with(other)
            || self.context == other.context
                && self.materialize_bounds == other.materialize_bounds
                && matches!(
                    (&self.operation, &other.operation),
                    (
                        TypeMapping::Promote(_, PromotionKind::SingletonsOnly),
                        TypeMapping::Promote(_, PromotionKind::Regular)
                    )
                )
    }
}

impl get_size2::GetSize for DeferredTypeMapping<'_> {}

impl<'db> DeferredTypeMapping<'db> {
    /// Map an argument in both callable polarities. An enclosing annotation can make the
    /// result depend on the argument's position, in which case no context-free images exist.
    pub(super) fn argument_images(
        self,
        db: &'db dyn Db,
        argument: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<[Type<'db>; 2]> {
        let mut images = match self.previous(db) {
            Some(previous) => previous.argument_images(db, argument, visitor)?,
            None => [argument; 2],
        };
        let step = self.step(db);
        if step.context.annotation.is_some() {
            return None;
        }
        let mut visitor = visitor.for_new_materialization_root();
        visitor.materialize_typevar_bounds_and_defaults = step.materialize_bounds;
        images[0] = images[0].apply_type_mapping_impl(
            db,
            &step.operation,
            TypeContext::default(),
            &visitor,
        );
        images[1] = images[1].apply_type_mapping_impl(
            db,
            &step.operation.flip(),
            TypeContext::default(),
            &visitor,
        );
        Some(images)
    }

    pub(super) fn append(
        db: &'db dyn Db,
        previous: Option<Self>,
        mapping: &TypeMapping<'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        if matches!(
            mapping,
            TypeMapping::ApplyRecursiveSubstitution(_) | TypeMapping::EagerExpansion
        ) {
            return None;
        }
        let operation = mapping.clone();
        let context = if context.annotation.is_none() {
            TypeContext::default()
        } else {
            context
        };
        Self::append_step(
            db,
            previous,
            MappingStep {
                operation,
                context,
                materialize_bounds: visitor.materialize_typevar_bounds_and_defaults,
            },
            visitor,
        )
    }

    fn append_step(
        db: &'db dyn Db,
        previous: Option<Self>,
        step: MappingStep<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let mut cursor = previous;
        while let Some(node) = cursor {
            let earlier = node.step(db);
            if earlier.absorbs(db, &step, visitor) {
                return previous;
            }
            if !earlier.preserves_fixed_points_of(&step) {
                break;
            }
            cursor = node.previous(db);
        }
        if let Some(previous) = previous
            && matches!(previous.step(db).operation, TypeMapping::Materialize(_))
            && previous.step(db).commutes_with(&step)
        {
            // Keep materialization last so its marker survives subsequent promotion.
            let prefix = Self::append_step(db, previous.previous(db), step, visitor);
            return Some(Self::new(db, prefix, previous.step(db).clone()));
        }
        Some(Self::new(db, previous, step))
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match &self.step(db).operation {
            TypeMapping::Materialize(kind) => Some(*kind),
            _ => None,
        }
    }

    pub(super) fn without_materialization(self, db: &'db dyn Db) -> Option<Self> {
        if self.materialization_kind(db).is_some() {
            self.previous(db)
        } else {
            Some(self)
        }
    }

    pub(super) fn apply(
        self,
        db: &'db dyn Db,
        mut ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if let Some(previous) = self.previous(db) {
            ty = previous.apply(db, ty, visitor);
        }
        // Each step has its own mapping identity, but materialization comparisons share a guard.
        let mut visitor = visitor.for_new_materialization_root();
        visitor.materialize_typevar_bounds_and_defaults = self.step(db).materialize_bounds;
        ty.apply_type_mapping_impl(
            db,
            &self.step(db).operation,
            self.step(db).context,
            &visitor,
        )
    }
}

impl<'db> ApplySpecialization<'db> {
    fn absorbs(
        &self,
        db: &'db dyn Db,
        later: &Self,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        if self.specialize_self_domain()
            || later.specialize_self_domain()
            || self.preserves_lazy_signatures()
            || self.materializes_alias_arguments(db)
            || later.materializes_alias_arguments(db)
        {
            return false;
        }
        // Partial substitutions preserve their skipped variable in alias arguments.
        // Both the direct substitution and its alias projection must absorb the later mapping.
        if let Some(earlier_alias) = self.as_specialization(db) {
            let Some(later_alias) = later.as_specialization(db) else {
                return false;
            };
            if !Self::specialization(earlier_alias).absorbs_bindings(
                db,
                &Self::specialization(later_alias),
                visitor,
            ) {
                return false;
            }
        }
        self.absorbs_bindings(db, later, visitor)
    }

    fn materializes_alias_arguments(&self, db: &'db dyn Db) -> bool {
        // Stored materialization also acts on existing alias arguments, beyond substitutions.
        match self {
            Self::Specialization { specialization, .. } | Self::TypeAlias(specialization) => {
                specialization.materialization_kind(db).is_some()
            }
            Self::WithBindings { specialization, .. } => {
                specialization.materializes_alias_arguments(db)
            }
            Self::Partial { .. } | Self::ReturnCallables(_) | Self::Single(..) => false,
        }
    }

    fn variables(&self, db: &'db dyn Db) -> Vec<BoundTypeVarInstance<'db>> {
        match self {
            Self::Specialization { specialization, .. } | Self::TypeAlias(specialization) => {
                specialization.generic_context(db).variables(db).collect()
            }
            Self::Partial {
                generic_context, ..
            } => generic_context.variables(db).collect(),
            Self::ReturnCallables(replacements) => replacements.keys().copied().collect(),
            Self::Single(variable, _) => vec![*variable],
            Self::WithBindings {
                specialization,
                bindings,
            } => {
                let mut variables = specialization.variables(db);
                variables.extend(bindings.iter().map(|(variable, _)| *variable));
                variables
            }
        }
    }

    fn absorbs_bindings(
        &self,
        db: &'db dyn Db,
        later: &ApplySpecialization<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        if !later
            .variables(db)
            .into_iter()
            .all(|variable| later.get(db, variable).is_none() || self.get(db, variable).is_some())
        {
            return false;
        }
        let mapping = TypeMapping::ApplySpecialization(later.clone());
        // The earlier replacements must be unchanged by the later substitution.
        // In particular, T -> list[T] is not idempotent.
        self.variables(db)
            .into_iter()
            .filter_map(|variable| self.get(db, variable))
            .all(|ty| {
                Dependencies::check(db, visitor.env, ty, |variable| {
                    let ty = Type::TypeVar(variable);
                    ty.apply_type_mapping_impl(db, &mapping, TypeContext::default(), visitor) == ty
                })
            })
    }
}
