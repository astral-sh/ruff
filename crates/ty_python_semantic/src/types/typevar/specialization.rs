//! Bound-variable substitution transfers replacement types without substituting them again.

use std::convert::Infallible;

use super::{BoundTypeVarIdentity, BoundTypeVarInstance, ParamSpecAttrKind, TypeVarKind};
use crate::Db;
use crate::types::generics::prefix::TypeArgumentPrefix;
use crate::types::{ApplySpecialization, GenericContext, Type};

pub(in crate::types) enum TypeVarSpecialization<'db> {
    Type(Type<'db>),
    RetainedSelf,
}

pub(in crate::types) struct TypeVarSpecializationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarSpecializationEffects)]
    pub(in crate::types) trait TypeVarSpecializationEffects<'db> {
        type Error;

        #[operation(source)]
        async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn kind(&self, identity: BoundTypeVarIdentity<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(child)]
        async fn without_paramspec_attr(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn context_index(&self, context: GenericContext<'db>, identity: BoundTypeVarIdentity<'db>) -> Result<Option<usize>, Self::Error>;
        #[operation(local)]
        async fn prefix_type(&self, types: TypeArgumentPrefix<'_, 'db>, index: usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn other_lookup(&self, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn with_paramspec_attr(&self, variable: BoundTypeVarInstance<'db>, attr: ParamSpecAttrKind) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn specialize_self_domain(&self, specialization: &ApplySpecialization<'_, 'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn finish(&self, result: TypeVarSpecialization<'db>) -> Result<TypeVarSpecialization<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TypeVarSpecializationFacts {
        fn is_paramspec(&self, kind: TypeVarKind) -> bool {
            kind.is_paramspec()
        }

        fn is_self(&self, kind: TypeVarKind) -> bool {
            matches!(kind, TypeVarKind::TypingSelf)
        }

        fn paramspec_attr(&self, identity: BoundTypeVarIdentity<'_>) -> Option<ParamSpecAttrKind> {
            identity.paramspec_attr
        }

        fn skipped(&self, skip: Option<usize>, index: usize) -> bool {
            skip == Some(index)
        }

        fn partial<'a, 'db>(
            &self,
            specialization: &ApplySpecialization<'a, 'db>,
        ) -> Option<(GenericContext<'db>, TypeArgumentPrefix<'a, 'db>, Option<usize>)> {
            match specialization {
                ApplySpecialization::Partial { generic_context, types, skip } => {
                    Some((*generic_context, *types, *skip))
                }
                _ => None,
            }
        }
    }

    #[synchronous(specialize_bound_typevar_sync)]
    #[capabilities(effects = TypeVarSpecializationEffects, facts = TypeVarSpecializationFacts)]
    #[passive_values(Type::TypeVar, Type::Never, TypeVarSpecialization::Type, TypeVarSpecialization::RetainedSelf)]
    pub(in crate::types) async fn specialize_bound_typevar_with<'db, E: TypeVarSpecializationEffects<'db>>(
        variable: BoundTypeVarInstance<'db>,
        specialization: &ApplySpecialization<'_, 'db>,
        facts: TypeVarSpecializationFacts,
        effects: &E,
    ) -> Result<TypeVarSpecialization<'db>, E::Error> {
        let identity = effects.identity(variable).await?;
        let kind = effects.kind(identity).await?;
        let (lookup_variable, lookup_identity) = if facts.is_paramspec(kind) {
            let variable = effects.without_paramspec_attr(variable).await?;
            let identity = effects.identity(variable).await?;
            (variable, identity)
        } else {
            (variable, identity)
        };

        let mapped = match facts.partial(specialization) {
            Some((generic_context, types, skip)) => {
                match effects.context_index(generic_context, lookup_identity).await? {
                    Some(index) if facts.skipped(skip, index) => Some(Type::Never),
                    Some(index) => effects.prefix_type(types, index).await?,
                    None => None,
                }
            }
            None => effects.other_lookup(specialization, lookup_variable).await?,
        };

        if let Some(mapped) = mapped {
            let mapped = if let Some(attr) = facts.paramspec_attr(identity)
                && let Type::TypeVar(replacement) = mapped
            {
                let replacement_identity = effects.identity(replacement).await?;
                if facts.is_paramspec(effects.kind(replacement_identity).await?) {
                    Type::TypeVar(effects.with_paramspec_attr(replacement, attr).await?)
                } else {
                    mapped
                }
            } else {
                mapped
            };
            // Substitution transfers the replacement handle; it does not apply the same
            // substitution recursively to the replacement's own type variables.
            effects.finish(TypeVarSpecialization::Type(mapped)).await
        } else if facts.is_self(kind) && effects.specialize_self_domain(specialization).await? {
            effects.finish(TypeVarSpecialization::RetainedSelf).await
        } else {
            effects.finish(TypeVarSpecialization::Type(Type::TypeVar(variable))).await
        }
    }
}

pub(super) struct OrdinaryTypeVarSpecialization<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousTypeVarSpecializationEffects<'db> for OrdinaryTypeVarSpecialization<'db> {
    type Error = Infallible;

    fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(variable.identity(self.db))
    }

    fn kind(&self, identity: BoundTypeVarIdentity<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(identity.kind(self.db))
    }

    fn without_paramspec_attr(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(variable.without_paramspec_attr(self.db))
    }

    fn context_index(
        &self,
        context: GenericContext<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> Result<Option<usize>, Infallible> {
        Ok(context
            .variables_with_fields(salsa::FieldReads::new(self.db))
            .get_index_of(&identity))
    }

    fn prefix_type(
        &self,
        types: TypeArgumentPrefix<'_, 'db>,
        index: usize,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(types.get(index))
    }

    fn other_lookup(
        &self,
        specialization: &ApplySpecialization<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(specialization.get(self.db, variable))
    }

    fn with_paramspec_attr(
        &self,
        variable: BoundTypeVarInstance<'db>,
        attr: ParamSpecAttrKind,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(variable.with_paramspec_attr(self.db, attr))
    }

    fn specialize_self_domain(
        &self,
        specialization: &ApplySpecialization<'_, 'db>,
    ) -> Result<bool, Infallible> {
        Ok(specialization.specialize_self_domain())
    }

    fn finish(
        &self,
        result: TypeVarSpecialization<'db>,
    ) -> Result<TypeVarSpecialization<'db>, Infallible> {
        Ok(result)
    }
}
