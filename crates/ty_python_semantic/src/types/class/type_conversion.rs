//! Conversion from class-valued types applies defaults to unspecialized literals.

use std::convert::Infallible;

use crate::Db;
use crate::types::{ClassLiteral, ClassType, Type};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeToClassEffects)]
    pub(in crate::types) trait TypeToClassEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn default_specialization(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
    }

    #[synchronous(type_to_class_type_sync)]
    #[capabilities(effects = TypeToClassEffects)]
    #[passive_values(ClassType::Generic)]
    pub(in crate::types) async fn type_to_class_type_with<'db, E: TypeToClassEffects<'db>>(
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Option<ClassType<'db>>, E::Error> {
        effects.checkpoint().await?;
        Ok(match ty {
            Type::ClassLiteral(class) => Some(effects.default_specialization(class).await?),
            Type::GenericAlias(alias) => Some(ClassType::Generic(alias)),
            _ => None,
        })
    }
}

pub(in crate::types) fn type_to_class_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<ClassType<'db>> {
    match type_to_class_type_sync(ty, &InlineTypeToClassEffects { db }) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

struct InlineTypeToClassEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousTypeToClassEffects<'db> for InlineTypeToClassEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn default_specialization(
        &self,
        class: ClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(class.default_specialization(self.db))
    }
}
