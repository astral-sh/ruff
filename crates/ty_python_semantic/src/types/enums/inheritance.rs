//! Enum classification checks the class before consulting its metaclass.

use std::convert::Infallible;

use crate::types::{ClassLiteral, KnownClass, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

pub(super) struct OrdinaryEnumInheritanceEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousEnumInheritanceEffects)]
    pub(in crate::types) trait EnumInheritanceEffects<'db> {
        type Error;

        #[operation(child)]
        async fn known_subclass(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn is_subtype(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn metaclass(&self, class: StaticClassLiteral<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(is_enum_class_by_inheritance_sync)]
    #[capabilities(effects = EnumInheritanceEffects)]
    #[passive_values(Type::ClassLiteral, ClassLiteral::Static, KnownClass::Enum, KnownClass::EnumType)]
    pub(in crate::types) async fn is_enum_class_by_inheritance_with<'db, E: EnumInheritanceEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let enum_subclass = effects.known_subclass(KnownClass::Enum).await?;
        if effects.is_subtype(Type::ClassLiteral(ClassLiteral::Static(class)), enum_subclass).await? {
            return Ok(true);
        }
        let metaclass = effects.metaclass(class).await?;
        let enum_metaclass = effects.known_subclass(KnownClass::EnumType).await?;
        effects.is_subtype(metaclass, enum_metaclass).await
    }
}

impl<'db> SynchronousEnumInheritanceEffects<'db> for OrdinaryEnumInheritanceEffects<'_, 'db> {
    type Error = Infallible;

    fn known_subclass(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_subclass_of(self.db, self.env))
    }

    fn is_subtype(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error> {
        Ok(source.is_subtype_of(self.db, self.env, target))
    }

    fn metaclass(&self, class: StaticClassLiteral<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(class.metaclass(self.db))
    }
}
