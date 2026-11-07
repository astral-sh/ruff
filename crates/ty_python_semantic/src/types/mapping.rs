//! Database-owned mappings retained by recursive types until they are unfolded.

use std::hash::{Hash, Hasher};
use std::mem::discriminant;

use rustc_hash::{FxHashMap, FxHasher};
use smallvec::{SmallVec, smallvec};
use ty_python_core::scope::ScopeId;

use super::generics::{ApplySpecialization, Specialization};
use super::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, GenericContext,
    MaterializationKind, PromotionKind, PromotionMode, SelfBinding, SelfTypeVarOrigin, Type,
    TypeContext, TypeMapping,
};
use crate::{Db, FxIndexMap};

/// An owned mapping and its application context, retained until a recursive type is unfolded.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct DeferredTypeMapping<'db> {
    kind: OwnedTypeMapping<'db>,
    context: TypeContext<'db>,
    materialize_typevar_bounds_and_defaults: bool,
}

impl<'db> DeferredTypeMapping<'db> {
    pub(super) fn from_mapping(
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> SmallVec<[Self; 2]> {
        let kind = match mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                OwnedTypeMapping::Specialize(OwnedApplySpecialization::new(*specialization), None)
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => OwnedTypeMapping::Specialize(
                OwnedApplySpecialization::new(*specialization),
                Some(*materialization_kind),
            ),
            TypeMapping::BindLegacyTypevars(context) => {
                OwnedTypeMapping::BindLegacyTypevars(*context)
            }
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => OwnedTypeMapping::FreshenBoundTypeVars(*generic_context, *delta),
            TypeMapping::BindSelf(binding) => OwnedTypeMapping::BindSelf(binding.clone()),
            TypeMapping::ReplaceSelf {
                new_origin,
                new_upper_bound,
            } => OwnedTypeMapping::ReplaceSelf(*new_origin, *new_upper_bound),
            TypeMapping::Materialize(kind) => OwnedTypeMapping::Materialize(*kind),
            TypeMapping::Promote(mode, kind) => OwnedTypeMapping::Promote(*mode, *kind),
            TypeMapping::ReplaceParameterDefaults => OwnedTypeMapping::ReplaceParameterDefaults,
            TypeMapping::EagerExpansion => OwnedTypeMapping::EagerExpansion,
            TypeMapping::GeneralizeLocalTypes(scope) => {
                OwnedTypeMapping::GeneralizeLocalTypes(*scope)
            }
            TypeMapping::RescopeReturnCallables(replacements) => {
                let mut replacements = (*replacements).clone();
                replacements.shrink_to_fit();
                OwnedTypeMapping::RescopeReturnCallables(replacements)
            }
            TypeMapping::ApplyRecursiveSubstitution(_) => return SmallVec::new(),
        };
        let operation = Self {
            kind,
            context,
            materialize_typevar_bounds_and_defaults: visitor
                .materialize_typevar_bounds_and_defaults,
        };
        let OwnedTypeMapping::Specialize(specialization, None) = &operation.kind else {
            return smallvec![operation];
        };
        let Some(substitution) = specialization.substitution(db) else {
            return smallvec![operation];
        };
        // Substitution can cancel without restoring removed signature parameters. Keep
        // declaration changes and lazy signature evaluation separate from the composable table.
        let original = specialization.borrowed();
        smallvec![
            Self {
                kind: OwnedTypeMapping::Specialize(
                    OwnedApplySpecialization::Effects {
                        specialization: substitution,
                        specialize_self_domain: original.specialize_self_domain(),
                        evaluate_lazy_signatures: !original.preserves_lazy_signatures(),
                    },
                    None
                ),
                ..operation.clone()
            },
            Self {
                kind: OwnedTypeMapping::Specialize(
                    OwnedApplySpecialization::Substitution(substitution),
                    None
                ),
                ..operation
            }
        ]
    }

    /// Append an operation, composing adjacent pure substitutions in application order.
    pub(super) fn append(
        self,
        db: &'db dyn Db,
        mappings: &mut Vec<Self>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) {
        if let Some(previous) = mappings.last_mut()
            && let OwnedTypeMapping::Specialize(OwnedApplySpecialization::Substitution(left), None) =
                &mut previous.kind
            && let OwnedTypeMapping::Specialize(OwnedApplySpecialization::Substitution(right), None) =
                self.kind
            && previous.context == self.context
            && previous.materialize_typevar_bounds_and_defaults
                == self.materialize_typevar_bounds_and_defaults
        {
            let mut variables: Vec<_> = left.generic_context(db).variables(db).collect();
            for variable in right.generic_context(db).variables(db) {
                if left
                    .generic_context(db)
                    .variable_index(db, variable.identity(db))
                    .is_none()
                {
                    variables.push(variable);
                }
            }
            let mapping =
                TypeMapping::ApplySpecialization(ApplySpecialization::Substitution(right));
            let types: Vec<_> = variables
                .iter()
                .map(|variable| {
                    left.get(db, *variable).map_or_else(
                        || right.get(db, *variable).unwrap_or(Type::TypeVar(*variable)),
                        |ty| {
                            ty.apply_type_mapping_impl(
                                db,
                                &mapping,
                                TypeContext::default(),
                                visitor,
                            )
                        },
                    )
                })
                .collect();
            *left = GenericContext::from_typevar_instances(db, visitor.env, variables)
                .specialize(db, types);
            return;
        }
        mappings.push(self);
    }

    pub(super) fn apply(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let visitor = ApplyTypeMappingVisitor {
            materialize_typevar_bounds_and_defaults: self.materialize_typevar_bounds_and_defaults,
            ..visitor.for_new_materialization_root()
        };
        ty.apply_type_mapping_impl(db, &self.type_mapping(), self.context, &visitor)
    }

    pub(super) fn type_mapping(&self) -> TypeMapping<'_, 'db> {
        match &self.kind {
            OwnedTypeMapping::Specialize(specialization, materialization) => {
                let specialization = specialization.borrowed();
                match materialization {
                    None => TypeMapping::ApplySpecialization(specialization),
                    Some(kind) => TypeMapping::ApplySpecializationWithMaterialization {
                        specialization,
                        materialization_kind: *kind,
                    },
                }
            }
            OwnedTypeMapping::BindLegacyTypevars(context) => {
                TypeMapping::BindLegacyTypevars(*context)
            }
            OwnedTypeMapping::FreshenBoundTypeVars(generic_context, delta) => {
                TypeMapping::FreshenBoundTypeVars {
                    generic_context: *generic_context,
                    delta: *delta,
                }
            }
            OwnedTypeMapping::BindSelf(binding) => TypeMapping::BindSelf(binding.clone()),
            OwnedTypeMapping::ReplaceSelf(new_origin, new_upper_bound) => {
                TypeMapping::ReplaceSelf {
                    new_origin: *new_origin,
                    new_upper_bound: *new_upper_bound,
                }
            }
            OwnedTypeMapping::Materialize(kind) => TypeMapping::Materialize(*kind),
            OwnedTypeMapping::Promote(mode, kind) => TypeMapping::Promote(*mode, *kind),
            OwnedTypeMapping::ReplaceParameterDefaults => TypeMapping::ReplaceParameterDefaults,
            OwnedTypeMapping::EagerExpansion => TypeMapping::EagerExpansion,
            OwnedTypeMapping::GeneralizeLocalTypes(scope) => {
                TypeMapping::GeneralizeLocalTypes(*scope)
            }
            OwnedTypeMapping::RescopeReturnCallables(replacements) => {
                TypeMapping::RescopeReturnCallables(replacements)
            }
        }
    }

    pub(super) fn is_substitution(&self) -> bool {
        matches!(
            self.kind,
            OwnedTypeMapping::Specialize(..)
                | OwnedTypeMapping::BindLegacyTypevars(_)
                | OwnedTypeMapping::FreshenBoundTypeVars(..)
                | OwnedTypeMapping::BindSelf(_)
                | OwnedTypeMapping::ReplaceSelf(..)
        )
    }

    pub(super) fn substitution(&self) -> Option<Specialization<'db>> {
        match self.kind {
            OwnedTypeMapping::Specialize(
                OwnedApplySpecialization::Substitution(specialization),
                None,
            ) => Some(specialization),
            _ => None,
        }
    }

    pub(super) fn without_variable(
        &self,
        db: &'db dyn Db,
        index: usize,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let specialization = self.substitution()?;
        let variables = specialization
            .generic_context(db)
            .variables(db)
            .enumerate()
            .filter_map(|(i, variable)| (i != index).then_some(variable));
        let context = GenericContext::try_from_typevar_instances(db, visitor.env, variables)?;
        let types: Vec<_> = specialization
            .types(db)
            .iter()
            .enumerate()
            .filter_map(|(i, ty)| (i != index).then_some(*ty))
            .collect();
        Some(Self {
            kind: OwnedTypeMapping::Specialize(
                OwnedApplySpecialization::Substitution(context.specialize(db, types)),
                None,
            ),
            ..self.clone()
        })
    }

    /// Close or rebind recursive references in the retained operation's operands.
    pub(super) fn rewrite(
        &self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let map = |ty: Type<'db>| {
            ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor)
        };
        let mut result = self.clone();
        result.context = result
            .context
            .with_declared_type(result.context.declared_type().map(map));
        match &mut result.kind {
            OwnedTypeMapping::Specialize(specialization, _) => {
                specialization.rewrite(db, mapping, visitor);
            }
            OwnedTypeMapping::BindSelf(binding) => binding.ty = map(binding.ty),
            OwnedTypeMapping::ReplaceSelf(_, upper_bound) => *upper_bound = map(*upper_bound),
            OwnedTypeMapping::RescopeReturnCallables(replacements) => {
                *replacements = replacements
                    .iter()
                    .map(|(source, target)| {
                        (
                            source.apply_type_mapping_impl(
                                db,
                                mapping,
                                TypeContext::default(),
                                visitor,
                            ),
                            target.apply_type_mapping_impl(
                                db,
                                mapping,
                                TypeContext::default(),
                                visitor,
                            ),
                        )
                    })
                    .collect();
                replacements.shrink_to_fit();
            }
            _ => {}
        }
        result
    }
}

/// An owned form of [`TypeMapping`] that can outlive the traversal that created it.
/// Traversal-local recursive substitutions are applied immediately and are not stored here.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum OwnedTypeMapping<'db> {
    Specialize(OwnedApplySpecialization<'db>, Option<MaterializationKind>),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars(GenericContext<'db>, u32),
    BindSelf(SelfBinding<'db>),
    ReplaceSelf(SelfTypeVarOrigin<'db>, Type<'db>),
    Materialize(MaterializationKind),
    Promote(PromotionMode, PromotionKind),
    ReplaceParameterDefaults,
    EagerExpansion,
    GeneralizeLocalTypes(ScopeId<'db>),
    RescopeReturnCallables(FxHashMap<CallableType<'db>, CallableType<'db>>),
}

impl Hash for OwnedTypeMapping<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Specialize(specialization, materialization) => {
                (specialization, materialization).hash(state);
            }
            Self::BindLegacyTypevars(context) => context.hash(state),
            Self::FreshenBoundTypeVars(context, delta) => (context, delta).hash(state),
            Self::BindSelf(binding) => binding.hash(state),
            Self::ReplaceSelf(origin, bound) => (origin, bound).hash(state),
            Self::Materialize(kind) => kind.hash(state),
            Self::Promote(mode, kind) => (mode, kind).hash(state),
            Self::GeneralizeLocalTypes(scope) => scope.hash(state),
            Self::RescopeReturnCallables(replacements) => {
                // Map equality ignores insertion order.
                replacements
                    .iter()
                    .map(|pair| {
                        let mut hash = FxHasher::default();
                        pair.hash(&mut hash);
                        hash.finish()
                    })
                    .fold(0_u64, u64::wrapping_add)
                    .hash(state);
            }
            Self::ReplaceParameterDefaults | Self::EagerExpansion => {}
        }
    }
}

/// An owned form of [`ApplySpecialization`] for a later unfolding.
/// Callable-local `WithBindings` selections are omitted; their underlying specialization is retained.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum OwnedApplySpecialization<'db> {
    Substitution(Specialization<'db>),
    Effects {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        evaluate_lazy_signatures: bool,
    },
    Specialization(Specialization<'db>, bool),
    TypeAlias(Specialization<'db>),
    Partial(GenericContext<'db>, Box<[Type<'db>]>, Option<usize>),
    ReturnCallables(FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>),
    Single(BoundTypeVarInstance<'db>, Type<'db>),
}

impl<'db> OwnedApplySpecialization<'db> {
    fn substitution(&self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            Self::Specialization(specialization, _) | Self::TypeAlias(specialization)
                if specialization.materialization_kind(db).is_none()
                    && specialization.tuple(db).is_none() =>
            {
                Some(*specialization)
            }
            Self::Partial(context, _, _) => Some(
                context.specialize(
                    db,
                    context
                        .variables(db)
                        .map(|variable| {
                            self.borrowed()
                                .get(db, variable)
                                .unwrap_or(Type::TypeVar(variable))
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
            _ => None,
        }
    }

    fn new(specialization: ApplySpecialization<'_, 'db>) -> Self {
        match specialization {
            ApplySpecialization::Substitution(specialization) => Self::Substitution(specialization),
            ApplySpecialization::Effects {
                specialization,
                specialize_self_domain,
                evaluate_lazy_signatures,
            } => Self::Effects {
                specialization,
                specialize_self_domain,
                evaluate_lazy_signatures,
            },
            ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            } => Self::Specialization(specialization, specialize_self_domain),
            ApplySpecialization::TypeAlias(specialization) => Self::TypeAlias(specialization),
            ApplySpecialization::Partial {
                generic_context,
                types,
                skip,
            } => Self::Partial(generic_context, types.into(), skip),
            ApplySpecialization::ReturnCallables(replacements) => {
                let mut replacements = replacements.clone();
                replacements.shrink_to_fit();
                Self::ReturnCallables(replacements)
            }
            ApplySpecialization::Single(variable, ty) => Self::Single(variable, ty),
            // Parameter-list selections belong to the enclosing callable, not a nested alias.
            ApplySpecialization::WithBindings { specialization, .. } => Self::new(*specialization),
        }
    }

    fn borrowed(&self) -> ApplySpecialization<'_, 'db> {
        match self {
            Self::Substitution(specialization) => {
                ApplySpecialization::Substitution(*specialization)
            }
            Self::Effects {
                specialization,
                specialize_self_domain,
                evaluate_lazy_signatures,
            } => ApplySpecialization::Effects {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
                evaluate_lazy_signatures: *evaluate_lazy_signatures,
            },
            Self::Specialization(specialization, specialize_self_domain) => {
                ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                }
            }
            Self::TypeAlias(specialization) => ApplySpecialization::TypeAlias(*specialization),
            Self::Partial(generic_context, types, skip) => ApplySpecialization::Partial {
                generic_context: *generic_context,
                types,
                skip: *skip,
            },
            Self::ReturnCallables(replacements) => {
                ApplySpecialization::ReturnCallables(replacements)
            }
            Self::Single(variable, ty) => ApplySpecialization::Single(*variable, *ty),
        }
    }

    fn rewrite(
        &mut self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) {
        match self {
            Self::Specialization(specialization, _)
            | Self::TypeAlias(specialization)
            | Self::Substitution(specialization)
            | Self::Effects { specialization, .. } => {
                *specialization = specialization.apply_type_mapping_impl(db, mapping, &[], visitor);
            }
            Self::Partial(_, types, _) => {
                for ty in types {
                    *ty = ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor);
                }
            }
            Self::Single(_, ty) => {
                *ty = ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor);
            }
            Self::ReturnCallables(_) => {}
        }
    }
}

impl Hash for OwnedApplySpecialization<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Substitution(specialization) => specialization.hash(state),
            Self::Effects {
                specialization,
                specialize_self_domain,
                evaluate_lazy_signatures,
            } => (
                specialization,
                specialize_self_domain,
                evaluate_lazy_signatures,
            )
                .hash(state),
            Self::Specialization(specialization, domain) => (specialization, domain).hash(state),
            Self::TypeAlias(specialization) => specialization.hash(state),
            Self::Partial(context, types, skip) => (context, types, skip).hash(state),
            Self::Single(variable, ty) => (variable, ty).hash(state),
            Self::ReturnCallables(replacements) => replacements
                .iter()
                .map(|pair| {
                    let mut hash = FxHasher::default();
                    pair.hash(&mut hash);
                    hash.finish()
                })
                .fold(0_u64, u64::wrapping_add)
                .hash(state),
        }
    }
}
