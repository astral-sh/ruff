//! Compare `type[T]` in the instance or metaclass domain without discarding target constraints.
//!
//! An inexact instance conversion can lose constraints such as an exact class object's identity.
//! The shared decision retains that conversion's quality and can instead transpose `T`'s bounds
//! or constraints into class-object types before comparing against the original target.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::TypeRelationChecker;
use crate::Db;
use crate::types::constraints::ConstraintSet;
use crate::types::{BoundTypeVarInstance, InstanceProjection, SubclassOfType, Type};

pub(super) struct TypeVarSubclassFacts;

shared_semantic_family! {
    #[synchronous(SynchronousTypeVarSubclassEffects)]
    pub(super) trait TypeVarSubclassEffects<'c, 'db: 'c> {
        type Error;

        #[operation(local)]
        async fn source_typevar(&self, source: SubclassOfType<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn exact_upper_bound(&self, source: SubclassOfType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_metaclass_instance(&self, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn metaclass_instance(&self, source: SubclassOfType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_projection(&self, target: Type<'db>) -> Result<Option<InstanceProjection<Type<'db>>>, Self::Error>;
        #[operation(child)]
        async fn transposed_typevar(&self, source: SubclassOfType<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn compare(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[finite_capability]
    impl TypeVarSubclassFacts {
        fn is_exact_upper_bound<'db>(&self, upper: Option<Type<'db>>, target: Type<'db>) -> bool {
            upper == Some(target)
        }

        fn projection_is_exact(&self, projection: &InstanceProjection<Type<'_>>) -> bool {
            projection.is_exact()
        }

        fn projected_type<'db>(&self, projection: InstanceProjection<Type<'db>>) -> Type<'db> {
            projection.into_inner()
        }

        fn typevar<'db>(&self, variable: BoundTypeVarInstance<'db>) -> Type<'db> {
            Type::TypeVar(variable)
        }
    }

    #[synchronous(check_typevar_subclass_sync)]
    #[capabilities(effects = TypeVarSubclassEffects, facts = TypeVarSubclassFacts)]
    #[passive_values()]
    pub(super) async fn check_typevar_subclass_with<'c, 'db: 'c, E: TypeVarSubclassEffects<'c, 'db>>(
        source: SubclassOfType<'db>, target: Type<'db>, facts: TypeVarSubclassFacts, effects: &E,
    ) -> Result<Option<ConstraintSet<'db, 'c>>, E::Error> {
        let Some(source_i) = effects.source_typevar(source).await? else {
            return Ok(None);
        };
        let upper_bound = effects.exact_upper_bound(source).await?;
        let is_exact_upper_bound = facts.is_exact_upper_bound(upper_bound, target);

        if effects.is_metaclass_instance(target).await? {
            let source = effects.metaclass_instance(source).await?;
            return Ok(Some(effects.compare(source, target).await?));
        }

        let Some(projection) = effects.instance_projection(target).await? else {
            return Ok(None);
        };
        if facts.projection_is_exact(&projection) || is_exact_upper_bound {
            return Ok(Some(effects.compare(facts.typevar(source_i), facts.projected_type(projection)).await?));
        }

        let Some(source) = effects.transposed_typevar(source).await? else {
            return Ok(None);
        };
        Ok(Some(effects.compare(facts.typevar(source), target).await?))
    }
}

pub(super) struct OrdinaryTypeVarSubclass<'check, 'a, 'c, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'c, 'db: 'c> SynchronousTypeVarSubclassEffects<'c, 'db>
    for OrdinaryTypeVarSubclass<'_, '_, 'c, 'db>
{
    type Error = Infallible;

    fn source_typevar(
        &self,
        source: SubclassOfType<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(source.into_type_var())
    }

    fn exact_upper_bound(
        &self,
        source: SubclassOfType<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(source.exact_typevar_upper_bound(self.db, self.checker.env))
    }

    fn is_metaclass_instance(&self, target: Type<'db>) -> Result<bool, Infallible> {
        Ok(self.checker.is_metaclass_instance(self.db, target))
    }

    fn metaclass_instance(&self, source: SubclassOfType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(source.to_metaclass_instance(self.db, self.checker.env))
    }

    fn instance_projection(
        &self,
        target: Type<'db>,
    ) -> Result<Option<InstanceProjection<Type<'db>>>, Infallible> {
        Ok(target.to_instance(self.db, self.checker.env))
    }

    fn transposed_typevar(
        &self,
        source: SubclassOfType<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(source
            .subclass_of()
            .with_transposed_type_var(self.db, self.checker.env)
            .into_type_var())
    }

    fn compare(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_type_pair(self.db, source, target))
    }
}
