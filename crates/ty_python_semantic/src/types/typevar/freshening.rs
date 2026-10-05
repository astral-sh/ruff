//! Freshen bound type variables and eagerly reconstruct their mapped bounds and defaults.

use std::convert::Infallible;
use std::slice;

use super::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraints,
    TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
    TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarVariance,
};
use crate::Db;
use crate::types::{ApplyTypeMappingVisitor, GenericContext, Type, TypeContext, TypeMapping};

/// Preserves occurrence metadata while changing its freshness nonce.
#[derive(Debug)]
pub(in crate::types) struct TypeVarFresheningFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarFresheningEffects)]
    /// Supplies canonical metadata, recursive mapping, and ordered constraint storage.
    pub(in crate::types) trait TypeVarFresheningEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn bound_identity(&self, bound: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn kind(&self, identity: TypeVarIdentity<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(source)]
        async fn contains(&self, context: GenericContext<'db>, identity: BoundTypeVarIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn bounds(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(child)]
        async fn bound_default(&self, bound: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn variance(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarVariance>, Self::Error>;
        #[operation(child)]
        /// Maps a descendant with the same mapping descriptor and visitor as the enclosing occurrence.
        async fn map_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn constraint_elements(&self, constraints: TypeVarConstraints<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        /// Creates empty storage sized for the mapped replacements of these ordered elements.
        async fn new_constraints(&self, elements: &[Type<'db>]) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_constraint(&self, elements: &mut slice::Iter<'db, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn push_constraint(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern_constraints(&self, elements: &mut Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_bound(&self, typevar: TypeVarInstance<'db>, identity: BoundTypeVarIdentity<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn finish(&self, bound: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TypeVarFresheningFacts {
        /// Returns the source identity whose kind determines ParamSpec handling.
        const fn source_identity<'db>(&self, identity: BoundTypeVarIdentity<'db>) -> TypeVarIdentity<'db> {
            identity.identity
        }

        /// Tests whether freshening must preserve this parameter specification unchanged.
        const fn is_paramspec(&self, kind: TypeVarKind) -> bool {
            kind.is_paramspec()
        }

        /// Makes ParamSpec attributes use their declaration's context-membership key.
        const fn membership_identity<'db>(
            &self,
            mut identity: BoundTypeVarIdentity<'db>,
            kind: TypeVarKind,
        ) -> BoundTypeVarIdentity<'db> {
            if kind.is_paramspec() {
                identity.paramspec_attr = None;
            }
            identity
        }

        /// Adds the delta to the freshness nonce without changing source identity, binding, or ParamSpec attribute.
        fn fresh_identity<'db>(
            &self,
            mut identity: BoundTypeVarIdentity<'db>,
            delta: u32,
        ) -> BoundTypeVarIdentity<'db> {
            identity.freshness = identity.freshness.add(delta);
            identity
        }

        /// Borrows the canonical constraint order without copying its element buffer.
        fn constraint_cursor<'db>(&self, elements: &'db [Type<'db>]) -> slice::Iter<'db, Type<'db>> {
            elements.iter()
        }
    }

    #[synchronous(freshen_bound_typevar_sync)]
    #[capabilities(effects = TypeVarFresheningEffects, facts = TypeVarFresheningFacts)]
    #[passive_values(TypeVarBoundOrConstraints::UpperBound, TypeVarBoundOrConstraints::Constraints, TypeVarBoundOrConstraintsEvaluation::Eager, TypeVarDefaultEvaluation::Eager)]
    /// Adds `delta` to a non-ParamSpec occurrence's freshness when its identity belongs to `generic_context`.
    /// Maps its bounds and then the default of the original bound occurrence using the caller's visitor.
    /// Occurrences outside the context and ParamSpecs remain unchanged.
    pub(in crate::types) async fn freshen_bound_typevar_with<'db, E: TypeVarFresheningEffects<'db>>(
        bound: BoundTypeVarInstance<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
        facts: TypeVarFresheningFacts,
        effects: &E,
    ) -> Result<BoundTypeVarInstance<'db>, E::Error> {
        effects.checkpoint().await?;
        let identity = effects.bound_identity(bound).await?;
        let kind = effects.kind(facts.source_identity(identity)).await?;
        let membership = facts.membership_identity(identity, kind);
        if !effects.contains(generic_context, membership).await? || facts.is_paramspec(kind) {
            return effects.finish(bound).await;
        }

        let identity = facts.fresh_identity(identity, delta);
        let typevar = effects.typevar(bound).await?;
        let bounds = effects.bounds(typevar).await?;
        let default = effects.bound_default(bound).await?;
        let typevar = match (bounds, default) {
            (None, None) => typevar,
            (bounds, default) => {
                let source_identity = effects.identity(typevar).await?;
                let bounds = match bounds {
                    None => None,
                    Some(TypeVarBoundOrConstraints::UpperBound(upper)) => {
                        let upper = effects.map_type(upper).await?;
                        Some(TypeVarBoundOrConstraintsEvaluation::Eager(
                            TypeVarBoundOrConstraints::UpperBound(upper),
                        ))
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        let elements = effects.constraint_elements(constraints).await?;
                        let mut mapped = effects.new_constraints(elements).await?;
                        let mut cursor = facts.constraint_cursor(elements);
                        #[cursor_loop]
                        while let Some(element) = effects.next_constraint(&mut cursor).await? {
                            let element = effects.map_type(element).await?;
                            effects.push_constraint(&mut mapped, element).await?;
                        }
                        let constraints = effects.intern_constraints(&mut mapped).await?;
                        Some(TypeVarBoundOrConstraintsEvaluation::Eager(
                            TypeVarBoundOrConstraints::Constraints(constraints),
                        ))
                    }
                };
                let variance = effects.variance(typevar).await?;
                let default = match default {
                    None => None,
                    Some(default) => Some(TypeVarDefaultEvaluation::Eager(
                        effects.map_type(default).await?,
                    )),
                };
                effects.intern_variable(source_identity, bounds, variance, default).await?
            }
        };
        let bound = effects.intern_bound(typevar, identity).await?;
        effects.finish(bound).await
    }
}

/// Uses ordinary reads and interning while retaining the caller's mapping and visitor.
struct OrdinaryTypeVarFreshening<'a, 'mapping, 'env, 'db> {
    db: &'db dyn Db,
    mapping: &'a TypeMapping<'mapping, 'db>,
    visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'db> SynchronousTypeVarFresheningEffects<'db> for OrdinaryTypeVarFreshening<'_, '_, '_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(bound.identity(self.db))
    }

    fn kind(&self, identity: TypeVarIdentity<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(identity.kind(self.db))
    }

    fn contains(
        &self,
        context: GenericContext<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> Result<bool, Infallible> {
        Ok(context
            .variables_with_fields(salsa::FieldReads::new(self.db))
            .contains_key(&identity))
    }

    fn typevar(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(bound.typevar(self.db))
    }

    fn bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Infallible> {
        super::bounds::typevar_bounds_sync(
            typevar,
            self.visitor.env,
            &super::bounds::OrdinaryTypeVarBoundsEffects { db: self.db },
        )
    }

    fn bound_default(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(bound.default_type(self.db))
    }

    fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(typevar.identity(self.db))
    }

    fn variance(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarVariance>, Infallible> {
        Ok(typevar.explicit_variance(self.db))
    }

    fn map_type(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(self.db, self.mapping, TypeContext::default(), self.visitor))
    }

    fn constraint_elements(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(constraints.elements(self.db))
    }

    fn new_constraints(&self, elements: &[Type<'db>]) -> Result<Vec<Type<'db>>, Infallible> {
        Ok(Vec::with_capacity(elements.len()))
    }

    fn next_constraint(
        &self,
        elements: &mut slice::Iter<'db, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }

    fn push_constraint(
        &self,
        elements: &mut Vec<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        elements.push(ty);
        Ok(())
    }

    fn intern_constraints(
        &self,
        elements: &mut Vec<Type<'db>>,
    ) -> Result<TypeVarConstraints<'db>, Infallible> {
        Ok(TypeVarConstraints::new(
            self.db,
            std::mem::take(elements).into_boxed_slice(),
        ))
    }

    fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(TypeVarInstance::new(
            self.db, identity, bounds, variance, default,
        ))
    }

    fn intern_bound(
        &self,
        typevar: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(BoundTypeVarInstance::new_internal(
            self.db, typevar, identity,
        ))
    }

    fn finish(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(bound)
    }
}

/// Freshens a context-selected non-ParamSpec occurrence synchronously by adding `delta` to its nonce.
/// Bounds and the original bound occurrence's default use the supplied `mapping` and `visitor`,
/// so recursive descendants remain part of the caller's mapping operation.
pub(super) fn freshen_bound_typevar<'db>(
    db: &'db dyn Db,
    bound: BoundTypeVarInstance<'db>,
    generic_context: GenericContext<'db>,
    delta: u32,
    mapping: &TypeMapping<'_, 'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
) -> BoundTypeVarInstance<'db> {
    match freshen_bound_typevar_sync(
        bound,
        generic_context,
        delta,
        TypeVarFresheningFacts,
        &OrdinaryTypeVarFreshening {
            db,
            mapping,
            visitor,
        },
    ) {
        Ok(bound) => bound,
        Err(never) => match never {},
    }
}
