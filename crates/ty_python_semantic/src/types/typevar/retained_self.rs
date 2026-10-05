//! Specializes a retained Self domain (its declared upper bound or ordered constraints) without
//! changing its occurrence identity or stored default.

use std::convert::Infallible;

use super::{BoundTypeVarInstance, TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarVariance};
use crate::types::{ApplySpecialization, ApplyTypeMappingVisitor, Specialization, Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    /// Resolves bounds in the occurrence's binding environment and reconstructs unchanged metadata.
    #[synchronous(SynchronousRetainedSelfEffects)]
    pub(in crate::types) trait RetainedSelfEffects<'db> {
        type Error;

        #[operation(local)]
        async fn prepare(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn binding_environment(&self, bound: BoundTypeVarInstance<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(source)]
        async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn bounds(&self, typevar: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(child)]
        async fn map_bounds(&self, bounds: TypeVarBoundOrConstraints<'db>, specialization: Specialization<'db>, env: &ProgramEnvironment<'db>) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn variance(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarVariance>, Self::Error>;
        #[operation(source)]
        async fn stored_default(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Self::Error>;
        #[operation(local)]
        async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn rebind(&self, variable: TypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
    }

    /// Maps an upper bound or ordered constraints with a single visitor, preserving constraint order and canonical storage.
    #[synchronous(SynchronousRetainedBoundsEffects)]
    pub(in crate::types) trait RetainedBoundsEffects<'db> {
        type Error;

        #[operation(local)]
        async fn prepare(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn elements(&self, constraints: TypeVarConstraints<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn new_elements(&self, elements: &[Type<'db>]) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, elements: &[Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern(&self, elements: &mut Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Self::Error>;
    }

    /// Rewrites only the domain (declared upper bound or ordered constraints) of a retained Self
    /// occurrence, preserving its default and binding metadata.
    ///
    /// Declared bounds are resolved in the occurrence's binding environment; `env` is the current
    /// mapping environment used to apply `specialization` to those bounds. Absent bounds are
    /// reconstructed without allocating a visitor. Present bounds use one new mapping visitor;
    /// the occurrence's identity and freshness stay unchanged.
    #[synchronous(retain_self_domain_sync)]
    #[capabilities(effects = RetainedSelfEffects)]
    #[passive_values(TypeVarBoundOrConstraintsEvaluation::Eager)]
    pub(in crate::types) async fn retain_self_domain_with<'db, E: RetainedSelfEffects<'db>>(
        bound: BoundTypeVarInstance<'db>,
        specialization: Specialization<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<BoundTypeVarInstance<'db>, E::Error> {
        effects.prepare().await?;
        let binding_env = effects.binding_environment(bound).await?;
        let variable = effects.typevar(bound).await?;
        let bounds = match effects.bounds(variable, &binding_env).await? {
            Some(bounds) => Some(effects.map_bounds(bounds, specialization, env).await?),
            None => None,
        };
        let identity = effects.identity(variable).await?;
        let bounds = match bounds {
            Some(bounds) => Some(TypeVarBoundOrConstraintsEvaluation::Eager(bounds)),
            None => None,
        };
        let variance = effects.variance(variable).await?;
        let default = effects.stored_default(variable).await?;
        let variable = effects.intern_variable(identity, bounds, variance, default).await?;
        effects.rebind(variable, bound).await
    }

    /// Maps an upper bound or each constraint through the same retained visitor, using
    /// non-materializing stored specialization with Self-domain recursion disabled.
    #[synchronous(map_retained_bounds_sync)]
    #[capabilities(effects = RetainedBoundsEffects)]
    #[passive_values(TypeVarBoundOrConstraints::UpperBound, TypeVarBoundOrConstraints::Constraints)]
    pub(in crate::types) async fn map_retained_bounds_with<'db, E: RetainedBoundsEffects<'db>>(
        db: &'db dyn Db,
        bounds: TypeVarBoundOrConstraints<'db>,
        effects: &E,
    ) -> Result<TypeVarBoundOrConstraints<'db>, E::Error> {
        effects.prepare().await?;
        match bounds {
            TypeVarBoundOrConstraints::UpperBound(upper) => {
                Ok(TypeVarBoundOrConstraints::UpperBound(effects.map_type(db, upper).await?))
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                let elements = effects.elements(constraints).await?;
                let mut mapped = effects.new_elements(elements).await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(element) = effects.next(elements, &mut cursor).await? {
                    let element = effects.map_type(db, element).await?;
                    effects.push(&mut mapped, element).await?;
                }
                Ok(TypeVarBoundOrConstraints::Constraints(effects.intern(&mut mapped).await?))
            }
        }
    }
}

/// Supplies ordinary canonical fields and interning for retained Self reconstruction.
pub(super) struct OrdinaryRetainedSelf<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousRetainedSelfEffects<'db> for OrdinaryRetainedSelf<'db> {
    type Error = Infallible;

    fn prepare(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn binding_environment(&self, bound: BoundTypeVarInstance<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_program(bound.binding_context(self.db).program(self.db)))
    }

    fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(bound.typevar(self.db))
    }

    fn bounds(&self, typevar: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Infallible> {
        Ok(typevar.bound_or_constraints(self.db, env))
    }

    fn map_bounds(&self, bounds: TypeVarBoundOrConstraints<'db>, specialization: Specialization<'db>, env: &ProgramEnvironment<'db>) -> Result<TypeVarBoundOrConstraints<'db>, Infallible> {
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
        let visitor = ApplyTypeMappingVisitor::new(env);
        map_retained_bounds_sync(self.db, bounds, &OrdinaryRetainedBounds { db: self.db, mapping: &mapping, visitor: &visitor })
    }

    fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(typevar.identity(self.db))
    }

    fn variance(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarVariance>, Infallible> {
        Ok(typevar.explicit_variance(self.db))
    }

    fn stored_default(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Infallible> {
        Ok(typevar._default(self.db))
    }

    fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(TypeVarInstance::new(self.db, identity, bounds, variance, default))
    }

    fn rebind(&self, variable: TypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(BoundTypeVarInstance::new(self.db, variable, original.binding_context(self.db), original.paramspec_attr(self.db), original.freshness(self.db)))
    }
}

/// Shares one ordinary visitor across every element of a present Self domain.
struct OrdinaryRetainedBounds<'a, 'env, 'db> {
    db: &'db dyn Db,
    mapping: &'a TypeMapping<'a, 'db>,
    visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'db> SynchronousRetainedBoundsEffects<'db> for OrdinaryRetainedBounds<'_, '_, 'db> {
    type Error = Infallible;

    fn prepare(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(db, self.mapping, TypeContext::default(), self.visitor))
    }

    fn elements(&self, constraints: TypeVarConstraints<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(constraints.elements(self.db))
    }

    fn new_elements(&self, elements: &[Type<'db>]) -> Result<Vec<Type<'db>>, Infallible> {
        Ok(Vec::with_capacity(elements.len()))
    }

    fn next(&self, elements: &[Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Infallible> {
        let result = elements.get(*cursor).copied();
        if result.is_some() {
            *cursor += 1;
        }
        Ok(result)
    }

    fn push(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Infallible> {
        elements.push(ty);
        Ok(())
    }

    fn intern(&self, elements: &mut Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Infallible> {
        Ok(TypeVarConstraints::new(self.db, std::mem::take(elements).into_boxed_slice()))
    }
}
