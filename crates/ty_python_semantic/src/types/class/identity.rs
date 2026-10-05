//! Identity specialization retains each generic parameter as its own argument.

use std::convert::Infallible;

use crate::Db;
use crate::types::{ClassLiteral, ClassType, GenericAlias, GenericContext, StaticClassLiteral};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassIdentityEffects)]
    pub(in crate::types) trait ClassIdentityEffects<'db> {
        type Error;

        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn identity_alias(&self, class: StaticClassLiteral<'db>, context: GenericContext<'db>) -> Result<ClassType<'db>, Self::Error>;
    }

    #[synchronous(class_identity_specialization_sync)]
    #[capabilities(effects = ClassIdentityEffects)]
    #[passive_values(ClassType::NonGeneric, ClassLiteral::Static)]
    pub(in crate::types) async fn class_identity_specialization_with<'db, E: ClassIdentityEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<ClassType<'db>, E::Error> {
        let Some(context) = effects.generic_context(class).await? else {
            return Ok(ClassType::NonGeneric(ClassLiteral::Static(class)));
        };
        effects.identity_alias(class, context).await
    }
}

pub(super) struct OrdinaryClassIdentityEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousClassIdentityEffects<'db> for OrdinaryClassIdentityEffects<'db> {
    type Error = Infallible;

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.generic_context(self.db))
    }

    fn identity_alias(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        let specialization = context.identity_specialization(self.db);
        Ok(ClassType::Generic(GenericAlias::new(
            self.db,
            class,
            specialization,
        )))
    }
}
