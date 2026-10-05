//! Shared type substitution with explicit recursive and legacy dependencies.

use std::future::Future;

use self::effects::{MappingEffects, MappingTransformationScope};
use crate::Db;
use crate::types::generics::prefix::{InitializedTypePrefix, TypeArgumentPrefix};
use crate::types::tuple::TupleType;
use self::return_callables::{RetainedReturnCallables, RetainedReturnTypevars, ReturnCallableReplacements, ReturnTypevarReplacements};
use crate::types::{
    ApplySpecialization, ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance,
    CallableType, FunctionType, GenericAlias, IntersectionType, MaterializationKind, NominalInstanceType, SelfBinding,
    PromotionKind, PromotionMode, Specialization, SubclassOfType, Type, TypeContext, TypeFormType, TypeMapping, UnionType,
};

pub(crate) mod effects;
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod materialization;
#[cfg(test)]
pub(in crate::types) mod runtime;
pub(in crate::types) mod self_binding;
pub(in crate::types) mod return_callables;
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod source;
pub(in crate::types) mod specialization;
pub(crate) mod specialization_start;

#[cfg(test)]
pub(in crate::types) mod attempt;

/// Mapping modes whose requests retain handles or a prefix owned by source resource storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum OwnedTypeMapping<'owner, 'db> {
    Partial {
        generic_context: crate::types::GenericContext<'db>,
        types: InitializedTypePrefix<'owner, 'db>,
        skip: Option<usize>,
    },
    Materialize(MaterializationKind),
    PromoteRegular(PromotionMode),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars {
        generic_context: crate::types::GenericContext<'db>,
        delta: u32,
    },
    ReturnCallables(RetainedReturnTypevars<'owner, 'db>),
    RescopeReturnCallables(RetainedReturnCallables<'owner, 'db>),
    BindSelf(SelfBinding<'db>),
    Single {
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    },
    Specialization {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        materialization_kind: Option<MaterializationKind>,
    },
}

impl<'owner, 'db> OwnedTypeMapping<'owner, 'db> {
    pub(in crate::types) fn into_mapping<'a>(self) -> TypeMapping<'a, 'db>
    where
        'db: 'a,
        'owner: 'a,
    {
        match self {
            Self::Partial { generic_context, types, skip } => {
                TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                    generic_context, types: TypeArgumentPrefix::Retained(types), skip,
                })
            }
            Self::Materialize(kind) => TypeMapping::Materialize(kind),
            Self::PromoteRegular(mode) => TypeMapping::Promote(mode, PromotionKind::Regular),
            Self::BindLegacyTypevars(context) => TypeMapping::BindLegacyTypevars(context),
            Self::FreshenBoundTypeVars { generic_context, delta } => {
                TypeMapping::FreshenBoundTypeVars { generic_context, delta }
            }
            Self::ReturnCallables(values) => TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(ReturnTypevarReplacements::Retained(values))),
            Self::RescopeReturnCallables(values) => TypeMapping::RescopeReturnCallables(ReturnCallableReplacements::Retained(values)),
            Self::BindSelf(binding) => TypeMapping::BindSelf(binding),
            Self::Single { variable, replacement } => {
                TypeMapping::ApplySpecialization(ApplySpecialization::Single(variable, replacement))
            }
            Self::Specialization {
                specialization,
                specialize_self_domain,
                materialization_kind,
            } => {
                let specialization = ApplySpecialization::Specialization {
                    specialization,
                    specialize_self_domain,
                };
                match materialization_kind {
                    None => TypeMapping::ApplySpecialization(specialization),
                    Some(materialization_kind) => {
                        TypeMapping::ApplySpecializationWithMaterialization {
                            specialization,
                            materialization_kind,
                        }
                    }
                }
            }
        }
    }

    pub(in crate::types) fn from_mapping(mapping: &TypeMapping<'owner, 'db>) -> Option<Self> {
        match mapping {
            TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                generic_context, types: TypeArgumentPrefix::Retained(types), skip,
            }) => Some(Self::Partial { generic_context: *generic_context, types: *types, skip: *skip }),
            TypeMapping::Materialize(kind) => Some(Self::Materialize(*kind)),
            TypeMapping::Promote(mode, PromotionKind::Regular) => Some(Self::PromoteRegular(*mode)),
            TypeMapping::BindLegacyTypevars(context) => Some(Self::BindLegacyTypevars(*context)),
            TypeMapping::FreshenBoundTypeVars { generic_context, delta } => {
                Some(Self::FreshenBoundTypeVars { generic_context: *generic_context, delta: *delta })
            }
            TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(ReturnTypevarReplacements::Retained(values))) => Some(Self::ReturnCallables(*values)),
            TypeMapping::RescopeReturnCallables(ReturnCallableReplacements::Retained(values)) => Some(Self::RescopeReturnCallables(*values)),
            TypeMapping::BindSelf(binding) => Some(Self::BindSelf(*binding)),
            TypeMapping::ApplySpecialization(ApplySpecialization::Single(variable, replacement)) => {
                Some(Self::Single { variable: *variable, replacement: *replacement })
            }
            TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            }) => Some(Self::Specialization {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
                materialization_kind: None,
            }),
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization:
                    ApplySpecialization::Specialization {
                        specialization,
                        specialize_self_domain,
                    },
                materialization_kind,
            } => Some(Self::Specialization {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
                materialization_kind: Some(*materialization_kind),
            }),
            _ => None,
        }
    }

    /// Recover the retained owner lifetime using bounded descriptor checks. A short borrow of
    /// an ordinary slice cannot become the input of a queued mapping child.
    pub(in crate::types) fn recapture(self, mapping: &TypeMapping<'_, 'db>) -> Option<Self> {
        match (self, mapping) {
            (Self::Partial { generic_context, types, skip },
             TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                 generic_context: actual_context,
                 types: TypeArgumentPrefix::Retained(actual_types),
                 skip: actual_skip,
             })) if generic_context == *actual_context && skip == *actual_skip && types.same_storage(*actual_types) => Some(self),
            (Self::ReturnCallables(values), TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(ReturnTypevarReplacements::Retained(actual)))) if values.same_storage(*actual) => Some(self),
            (Self::RescopeReturnCallables(values), TypeMapping::RescopeReturnCallables(ReturnCallableReplacements::Retained(actual))) if values.same_storage(*actual) => Some(self),
            (Self::BindSelf(binding), TypeMapping::BindSelf(actual)) if binding == *actual => {
                Some(self)
            }
            (Self::FreshenBoundTypeVars { generic_context, delta },
             TypeMapping::FreshenBoundTypeVars { generic_context: actual_context, delta: actual_delta })
                if generic_context == *actual_context && delta == *actual_delta => Some(self),
            (_, TypeMapping::Materialize(kind)) => Some(Self::Materialize(*kind)),
            (_, TypeMapping::Promote(mode, PromotionKind::Regular)) => Some(Self::PromoteRegular(*mode)),
            (_, TypeMapping::BindLegacyTypevars(context)) => Some(Self::BindLegacyTypevars(*context)),
            (_, TypeMapping::ApplySpecialization(ApplySpecialization::Single(variable, replacement))) => {
                Some(Self::Single { variable: *variable, replacement: *replacement })
            }
            (_, TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization, specialize_self_domain,
            })) => Some(Self::Specialization {
                specialization: *specialization, specialize_self_domain: *specialize_self_domain,
                materialization_kind: None,
            }),
            (_, TypeMapping::ApplySpecializationWithMaterialization {
                specialization: ApplySpecialization::Specialization { specialization, specialize_self_domain },
                materialization_kind,
            }) => Some(Self::Specialization {
                specialization: *specialization, specialize_self_domain: *specialize_self_domain,
                materialization_kind: Some(*materialization_kind),
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterializationOperation {
    Mode,
    Identity,
    TypeVar,
    Leaf(effects::MappingOperation),
    LegacyContinuation,
    Recovery,
}

/// A completed operation needs no suspended storage. A continuation retains the state after
/// the start's work, so resuming it never repeats a lookup, reservation, or transformation.
pub(crate) enum MappingStart<T, C> {
    Complete(T),
    Continue(C),
}

pub(crate) enum LegacyTypeMappingContinuation<'db> {
    TypeVar(TypeVarMappingContinuation<'db>),
    Nominal(NominalInstanceType<'db>),
    GenericAlias(GenericAlias<'db>),
    SubclassOf(SubclassOfType<'db>),
    Promotion(Type<'db>),
}

pub(crate) enum TypeMappingContinuation<'v, 'db> {
    Legacy(LegacyTypeMappingContinuation<'db>),
    Tuple(TupleType<'db>),
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    Function {
        function: FunctionType<'db>,
        scope: MappingTransformationScope<'v, 'db>,
    },
    Callable {
        callable: CallableType<'db>,
        scope: MappingTransformationScope<'v, 'db>,
    },
    TypeForm {
        typeform: TypeFormType<'db>,
        scope: MappingTransformationScope<'v, 'db>,
    },
}

impl<'db> LegacyTypeMappingContinuation<'db> {
    #[ty_mapping_probe_macros::dual_mapping]
    pub(super) async fn resume_mapping_with<E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mapped = match self {
            Self::TypeVar(continuation) => {
                continuation
                    .resume_mapping_with(db, visitor, effects)
                    .await?
            }
            Self::Nominal(instance) => {
                instance
                    .apply_type_mapping_with(db, mapping, tcx, visitor, effects)
                    .await?
            }
            Self::GenericAlias(alias) => Type::GenericAlias(
                alias
                    .apply_type_mapping_with(db, mapping, tcx, visitor, effects)
                    .await?,
            ),
            Self::SubclassOf(subclass) => {
                subclass
                    .apply_type_mapping_with(db, mapping, tcx, visitor, effects)
                    .await?
            }
            Self::Promotion(ty) => ty.promote_impl_with(db, visitor.env, effects).await?,
        };
        Ok(mapped)
    }
}

pub(crate) struct TypeVarMappingContinuation<'db> {
    pub(super) binding: SelfBinding<'db>,
    pub(super) variable: BoundTypeVarInstance<'db>,
}

impl<'db> TypeVarMappingContinuation<'db> {
    /// Selects the replacement when Self matching succeeds, or preserves the original variable.
    pub(in crate::types) const fn finish(self, should_bind: bool) -> Type<'db> {
        if should_bind {
            self.binding.ty
        } else {
            Type::TypeVar(self.variable)
        }
    }

    #[ty_mapping_probe_macros::dual_mapping]
    pub(super) async fn resume_mapping_with<E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let should_bind = effects
            .should_bind_self(db, visitor.env, &self.binding, self.variable)
            .await?;
        Ok(self.finish(should_bind))
    }
}
impl<'db> TypeMappingContinuation<'_, 'db> {
    pub(super) fn resume_mapping_with<E: effects::SharedMappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> impl Future<Output = Result<Type<'db>, E::Failure>> {
        effects::resume_type_mapping_with(db, self, mapping, tcx, visitor, effects)
    }
    pub(super) fn resume_mapping_sync<E: effects::SynchronousSharedMappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Failure> {
        effects::resume_type_mapping_sync(db, self, mapping, tcx, visitor, effects)
    }
}
