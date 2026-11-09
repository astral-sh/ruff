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

/// An owned mapping and its application context, retained until a recursive type is unfolded.
/// Substitutions of free variables act on the constructor's arguments instead.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct DeferredTypeMapping<'db> {
    kind: OwnedTypeMapping<'db>,
    context: TypeContext<'db>,
    materialize_typevar_bounds_and_defaults: bool,
}

impl<'db> DeferredTypeMapping<'db> {
    /// Retain whole-body transformations; argument and graph substitutions are applied immediately.
    pub(super) fn from_mapping(
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let kind = match mapping {
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
        if let OwnedTypeMapping::RescopeReturnCallables(replacements) = &mut result.kind {
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
        result
    }
}

/// An owned form of the mappings applied to a whole recursive unfolding.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum OwnedTypeMapping<'db> {
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
