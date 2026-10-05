//! Rename bound type variables without evaluating their bounds or defaults.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;

use super::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraintsEvaluation,
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarVariance,
};
use crate::Db;

/// Supplies the name separator and preserves occurrence metadata when replacing an identity.
#[derive(Debug)]
pub(in crate::types) struct TypeVarNameSuffixFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarNameSuffixEffects)]
    /// Reads stored metadata and interns a renamed identity, variable, and bound occurrence.
    pub(in crate::types) trait TypeVarNameSuffixEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn source_name(&self, identity: TypeVarIdentity<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(local)]
        async fn concatenate_name(&self, parts: [&str; 3]) -> Result<Name, Self::Error>;
        #[operation(source)]
        async fn definition(&self, identity: TypeVarIdentity<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(source)]
        async fn kind(&self, identity: TypeVarIdentity<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(local)]
        async fn intern_identity(&self, name: &Name, definition: Option<Definition<'db>>, kind: TypeVarKind) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn bounds(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Self::Error>;
        #[operation(source)]
        async fn variance(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarVariance>, Self::Error>;
        #[operation(source)]
        async fn default(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Self::Error>;
        #[operation(local)]
        async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn bound_identity(&self, bound: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_bound(&self, typevar: TypeVarInstance<'db>, identity: BoundTypeVarIdentity<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TypeVarNameSuffixFacts {
        fn name_parts<'a>(&self, name: &'a Name, suffix: &'a str) -> [&'a str; 3] {
            [name.as_str(), "'", suffix]
        }

        const fn renamed_identity<'db>(
            &self,
            mut bound: BoundTypeVarIdentity<'db>,
            identity: TypeVarIdentity<'db>,
        ) -> BoundTypeVarIdentity<'db> {
            bound.identity = identity;
            bound
        }
    }

    #[synchronous(with_name_suffix_sync)]
    #[capabilities(effects = TypeVarNameSuffixEffects, facts = TypeVarNameSuffixFacts)]
    #[passive_values()]
    /// Appends an apostrophe and suffix, preserving raw metadata, binding, ParamSpec attributes, and freshness.
    pub(in crate::types) async fn with_name_suffix_with<'db, E: TypeVarNameSuffixEffects<'db>>(
        bound: BoundTypeVarInstance<'db>,
        suffix: &str,
        facts: TypeVarNameSuffixFacts,
        effects: &E,
    ) -> Result<BoundTypeVarInstance<'db>, E::Error> {
        effects.checkpoint().await?;
        let typevar = effects.typevar(bound).await?;
        let identity = effects.identity(typevar).await?;
        let name = effects.source_name(identity).await?;
        let name = effects.concatenate_name(facts.name_parts(name, suffix)).await?;
        let definition = effects.definition(identity).await?;
        let kind = effects.kind(identity).await?;
        let identity = effects.intern_identity(&name, definition, kind).await?;
        let bounds = effects.bounds(typevar).await?;
        let variance = effects.variance(typevar).await?;
        let default = effects.default(typevar).await?;
        let typevar = effects.intern_variable(identity, bounds, variance, default).await?;
        let bound_identity = effects.bound_identity(bound).await?;
        let bound_identity = facts.renamed_identity(bound_identity, identity);
        effects.intern_bound(typevar, bound_identity).await
    }
}

/// Uses ordinary field access and interning for the shared suffix-renaming algorithm.
struct OrdinaryTypeVarNameSuffix<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousTypeVarNameSuffixEffects<'db> for OrdinaryTypeVarNameSuffix<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn typevar(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(bound.typevar(self.db))
    }

    fn identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(typevar.identity(self.db))
    }

    fn source_name(&self, identity: TypeVarIdentity<'db>) -> Result<&'db Name, Infallible> {
        Ok(identity.name(self.db))
    }

    fn concatenate_name(&self, parts: [&str; 3]) -> Result<Name, Infallible> {
        Ok(Name::concat(&parts))
    }

    fn definition(
        &self,
        identity: TypeVarIdentity<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(identity.definition(self.db))
    }

    fn kind(&self, identity: TypeVarIdentity<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(identity.kind(self.db))
    }

    fn intern_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(TypeVarIdentity::new(
            self.db,
            name.clone(),
            definition,
            kind,
        ))
    }

    fn bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Infallible> {
        Ok(typevar._bound_or_constraints(self.db))
    }

    fn variance(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarVariance>, Infallible> {
        Ok(typevar.explicit_variance(self.db))
    }

    fn default(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Infallible> {
        Ok(typevar._default(self.db))
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

    fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(bound.identity(self.db))
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
}

/// Runs suffix renaming synchronously while preserving lazy bound and default descriptors.
pub(super) fn with_name_suffix<'db>(
    db: &'db dyn Db,
    bound: BoundTypeVarInstance<'db>,
    suffix: &str,
) -> BoundTypeVarInstance<'db> {
    match with_name_suffix_sync(
        bound,
        suffix,
        TypeVarNameSuffixFacts,
        &OrdinaryTypeVarNameSuffix { db },
    ) {
        Ok(renamed) => renamed,
        Err(never) => match never {},
    }
}
