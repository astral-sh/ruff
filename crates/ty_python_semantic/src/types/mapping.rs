//! Database-owned mappings retained by recursive types until they are unfolded.

use std::hash::{Hash, Hasher};
use std::mem::discriminant;

use rustc_hash::{FxHashMap, FxHasher};
use ty_python_core::scope::ScopeId;

use super::{
    ApplyTypeMappingVisitor, CallableType, MaterializationKind, PromotionKind, PromotionMode, Type,
    TypeContext, TypeMapping,
};
use crate::Db;

/// A transformation and its application context, independent of its original traversal.
/// Substitutions of free variables act on the constructor's arguments instead.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct DeferredTypeMapping<'db> {
    kind: DeferredMappingKind<'db>,
    context: TypeContext<'db>,
    materialize_typevar_bounds_and_defaults: bool,
}

impl<'db> DeferredTypeMapping<'db> {
    pub(super) fn from_mapping(
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let kind = match mapping {
            TypeMapping::Materialize(kind) => DeferredMappingKind::Materialize(*kind),
            TypeMapping::Promote(mode, kind) => DeferredMappingKind::Promote(*mode, *kind),
            TypeMapping::ReplaceParameterDefaults => DeferredMappingKind::ReplaceParameterDefaults,
            TypeMapping::EagerExpansion => DeferredMappingKind::EagerExpansion,
            TypeMapping::GeneralizeLocalTypes(scope) => {
                DeferredMappingKind::GeneralizeLocalTypes(*scope)
            }
            TypeMapping::RescopeReturnCallables(replacements) => {
                let mut replacements = (*replacements).clone();
                replacements.shrink_to_fit();
                DeferredMappingKind::RescopeReturnCallables(replacements)
            }
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(_)
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::ApplyRecursiveSubstitution(_) => return None,
        };
        Some(Self {
            kind,
            context,
            materialize_typevar_bounds_and_defaults: visitor
                .materialize_typevar_bounds_and_defaults,
        })
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
            DeferredMappingKind::Materialize(kind) => TypeMapping::Materialize(*kind),
            DeferredMappingKind::Promote(mode, kind) => TypeMapping::Promote(*mode, *kind),
            DeferredMappingKind::ReplaceParameterDefaults => TypeMapping::ReplaceParameterDefaults,
            DeferredMappingKind::EagerExpansion => TypeMapping::EagerExpansion,
            DeferredMappingKind::GeneralizeLocalTypes(scope) => {
                TypeMapping::GeneralizeLocalTypes(*scope)
            }
            DeferredMappingKind::RescopeReturnCallables(replacements) => {
                TypeMapping::RescopeReturnCallables(replacements)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum DeferredMappingKind<'db> {
    Materialize(MaterializationKind),
    Promote(PromotionMode, PromotionKind),
    ReplaceParameterDefaults,
    EagerExpansion,
    GeneralizeLocalTypes(ScopeId<'db>),
    RescopeReturnCallables(FxHashMap<CallableType<'db>, CallableType<'db>>),
}

impl Hash for DeferredMappingKind<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
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
