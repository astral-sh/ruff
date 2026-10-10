//! Deferred mappings on aliases. Each step acts on a closed, specialized unfolding;
//! recursive references retain the step without rebuilding the recursive constructor.
//!
//! Mapping first checks whether the closed body changes, using the ordinary visitor's cycle
//! guard and cache. Unchanged aliases keep their identity. Each retained step has a separate
//! transformation cache, while nested materialization comparisons share their recursion guard.

use super::generics::{ApplySpecialization, Specialization};
use super::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, GenericContext,
    MaterializationKind, PromotionKind, PromotionMode, SelfBinding, Type, TypeContext, TypeMapping,
};
use crate::{Db, FxIndexMap};

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
    operation: MappingOperation<'db>,
    context: TypeContext<'db>,
    materialize_bounds: bool,
}

impl get_size2::GetSize for DeferredTypeMapping<'_> {}

impl<'db> DeferredTypeMapping<'db> {
    pub(super) fn operation(self, db: &'db dyn Db) -> &'db MappingOperation<'db> {
        &self.step(db).operation
    }

    pub(super) fn preceding(self, db: &'db dyn Db) -> Option<Self> {
        self.previous(db)
    }
    pub(super) fn append(
        db: &'db dyn Db,
        previous: Option<Self>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let operation = MappingOperation::from_mapping(mapping)?;
        let context = if context.annotation.is_none() {
            TypeContext::default()
        } else {
            context
        };
        if let Some(previous) = previous
            && operation.is_idempotent()
            && previous.step(db).operation == operation
            && previous.step(db).context == context
            && previous.step(db).materialize_bounds
                == visitor.materialize_typevar_bounds_and_defaults
        {
            return Some(previous);
        }
        Some(Self::new(
            db,
            previous,
            MappingStep {
                operation,
                context,
                materialize_bounds: visitor.materialize_typevar_bounds_and_defaults,
            },
        ))
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match &self.step(db).operation {
            MappingOperation::Materialize(kind) => Some(*kind),
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
        self.step(db).operation.with_mapping(&mut |mapping| {
            ty.apply_type_mapping_impl(db, &mapping, self.step(db).context, &visitor)
        })
    }
}

/// Owns the data borrowed by a mapping, so an alias can retain it after inference returns.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum MappingOperation<'db> {
    Specialize(OwnedSpecialization<'db>, Option<MaterializationKind>),
    Promote(PromotionMode, PromotionKind),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars(GenericContext<'db>, u32),
    BindSelf(SelfBinding<'db>),
    ReplaceSelf(Type<'db>),
    Materialize(MaterializationKind),
    ReplaceParameterDefaults,
    RescopeReturnCallables(Box<[(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)]>),
}

impl<'db> MappingOperation<'db> {
    fn from_mapping(mapping: &TypeMapping<'_, 'db>) -> Option<Self> {
        Some(match mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                Self::Specialize(OwnedSpecialization::new(*specialization), None)
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => Self::Specialize(
                OwnedSpecialization::new(*specialization),
                Some(*materialization_kind),
            ),
            TypeMapping::Promote(mode, kind) => Self::Promote(*mode, *kind),
            TypeMapping::BindLegacyTypevars(context) => Self::BindLegacyTypevars(*context),
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => Self::FreshenBoundTypeVars(*generic_context, *delta),
            TypeMapping::BindSelf(binding) => Self::BindSelf(binding.clone()),
            TypeMapping::ReplaceSelf { new_upper_bound } => Self::ReplaceSelf(*new_upper_bound),
            TypeMapping::Materialize(kind) => Self::Materialize(*kind),
            TypeMapping::ReplaceParameterDefaults => Self::ReplaceParameterDefaults,
            TypeMapping::RescopeReturnCallables(replacements) => {
                Self::RescopeReturnCallables(replacements.iter().map(|(a, b)| (*a, *b)).collect())
            }
            TypeMapping::ApplyRecursiveSubstitution(_) | TypeMapping::EagerExpansion => {
                return None;
            }
        })
    }

    fn is_idempotent(&self) -> bool {
        matches!(self, Self::Promote(..) | Self::ReplaceParameterDefaults)
    }

    fn with_mapping<R>(&self, f: &mut dyn FnMut(TypeMapping<'_, 'db>) -> R) -> R {
        match self {
            Self::Specialize(specialization, kind) => {
                specialization.with_mapping(&mut |specialization| {
                    f(match kind {
                        Some(kind) => TypeMapping::ApplySpecializationWithMaterialization {
                            specialization,
                            materialization_kind: *kind,
                        },
                        None => TypeMapping::ApplySpecialization(specialization),
                    })
                })
            }
            Self::Promote(mode, kind) => f(TypeMapping::Promote(*mode, *kind)),
            Self::BindLegacyTypevars(context) => f(TypeMapping::BindLegacyTypevars(*context)),
            Self::FreshenBoundTypeVars(context, delta) => f(TypeMapping::FreshenBoundTypeVars {
                generic_context: *context,
                delta: *delta,
            }),
            Self::BindSelf(binding) => f(TypeMapping::BindSelf(binding.clone())),
            Self::ReplaceSelf(ty) => f(TypeMapping::ReplaceSelf {
                new_upper_bound: *ty,
            }),
            Self::Materialize(kind) => f(TypeMapping::Materialize(*kind)),
            Self::ReplaceParameterDefaults => f(TypeMapping::ReplaceParameterDefaults),
            Self::RescopeReturnCallables(replacements) => f(TypeMapping::RescopeReturnCallables(
                &replacements.iter().copied().collect(),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum OwnedSpecialization<'db> {
    Specialization(Specialization<'db>, bool),
    TypeAlias(Specialization<'db>),
    Partial(GenericContext<'db>, Box<[Type<'db>]>, Option<usize>),
    ReturnCallables(Box<[(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)]>),
    Single(BoundTypeVarInstance<'db>, Type<'db>),
    WithBindings(Box<Self>, Box<[(BoundTypeVarInstance<'db>, Type<'db>)]>),
}

impl<'db> OwnedSpecialization<'db> {
    /// The substitutions shown in the marker, including overrides not in its generic context.
    pub(super) fn bindings(
        &self,
        db: &'db dyn Db,
    ) -> FxIndexMap<BoundTypeVarInstance<'db>, Type<'db>> {
        match self {
            Self::Specialization(specialization, _) | Self::TypeAlias(specialization) => {
                specialization
                    .generic_context(db)
                    .variables(db)
                    .zip(specialization.types(db).iter().copied())
                    .collect()
            }
            Self::Partial(context, types, skip) => context
                .variables(db)
                .enumerate()
                .filter_map(|(index, variable)| {
                    if skip == &Some(index) {
                        Some((variable, Type::Never))
                    } else {
                        types.get(index).map(|ty| (variable, *ty))
                    }
                })
                .collect(),
            Self::ReturnCallables(replacements) => replacements
                .iter()
                .map(|(from, to)| (*from, Type::TypeVar(*to)))
                .collect(),
            Self::Single(variable, ty) => [(*variable, *ty)].into_iter().collect(),
            Self::WithBindings(specialization, overrides) => {
                let mut bindings = specialization.bindings(db);
                bindings.extend(overrides.iter().copied());
                bindings
            }
        }
    }

    fn new(specialization: ApplySpecialization<'_, 'db>) -> Self {
        match specialization {
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
                Self::ReturnCallables(replacements.iter().map(|(a, b)| (*a, *b)).collect())
            }
            ApplySpecialization::Single(variable, ty) => Self::Single(variable, ty),
            ApplySpecialization::WithBindings {
                specialization,
                bindings,
            } => Self::WithBindings(Box::new(Self::new(*specialization)), bindings.into()),
        }
    }

    fn with_mapping<R>(&self, f: &mut dyn FnMut(ApplySpecialization<'_, 'db>) -> R) -> R {
        match self {
            Self::Specialization(specialization, specialize_self_domain) => {
                f(ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                })
            }
            Self::TypeAlias(specialization) => f(ApplySpecialization::TypeAlias(*specialization)),
            Self::Partial(context, types, skip) => f(ApplySpecialization::Partial {
                generic_context: *context,
                types,
                skip: *skip,
            }),
            Self::ReturnCallables(replacements) => {
                let replacements: FxIndexMap<_, _> = replacements.iter().copied().collect();
                f(ApplySpecialization::ReturnCallables(&replacements))
            }
            Self::Single(variable, ty) => f(ApplySpecialization::Single(*variable, *ty)),
            Self::WithBindings(specialization, bindings) => {
                specialization.with_mapping(&mut |specialization| {
                    f(ApplySpecialization::WithBindings {
                        specialization: &specialization,
                        bindings,
                    })
                })
            }
        }
    }
}
