//! Selects a class base's metaclass, including the declaring class's Protocol provenance.

use std::convert::Infallible;

use ty_module_resolver::{SearchPath, file_to_module};

use super::ClassBase;
use crate::types::class::ClassMetaclass;
use crate::types::{ClassLiteral, ClassType, DynamicType, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies metaclass inference and the declaring class's source provenance.
    #[synchronous(SynchronousClassBaseMetaclassEffects)]
    pub(in crate::types) trait ClassBaseMetaclassEffects<'db> {
        type Error;

        /// Funds one finite dispatch and its fixed value transfers.
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn class_metaclass(&self, class: ClassType<'db>) -> Result<ClassMetaclass<'db>, Self::Error>;
        #[operation(source)]
        async fn subclass_is_stub(&self, subclass: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn subclass_is_standard_library(&self, subclass: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn known_class_literal(&self, env: &ProgramEnvironment<'db>, known: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, env: &ProgramEnvironment<'db>, known: KnownClass) -> Result<Type<'db>, Self::Error>;
    }

    /// Returns a base's metaclass constraint or non-constraining fallback for a standard-library stub.
    /// Only a direct Protocol base consults the declaring subclass's provenance; a named class
    /// retains its own inferred result. Resolver lookup follows the stub check and is skipped
    /// for Python source files.
    #[synchronous(class_base_metaclass_sync)]
    #[capabilities(effects = ClassBaseMetaclassEffects)]
    #[passive_values(ClassMetaclass::Selected, ClassMetaclass::ProtocolFallback, Type::Dynamic, Type::Divergent, DynamicType::Any, KnownClass::ProtocolMeta, KnownClass::Type)]
    pub(in crate::types) async fn class_base_metaclass_with<'db, E: ClassBaseMetaclassEffects<'db>>(
        base: ClassBase<'db>,
        env: &ProgramEnvironment<'db>,
        subclass: ClassLiteral<'db>,
        effects: &E,
    ) -> Result<ClassMetaclass<'db>, E::Error> {
        effects.checkpoint().await?;
        let metaclass = match base {
            ClassBase::Class(class) => return effects.class_metaclass(class).await,
            ClassBase::Protocol => {
                if effects.subclass_is_stub(subclass).await?
                    && effects.subclass_is_standard_library(subclass).await?
                {
                    return Ok(ClassMetaclass::ProtocolFallback);
                }
                effects.known_class_literal(env, KnownClass::ProtocolMeta).await?
            }
            ClassBase::Any => Type::Dynamic(DynamicType::Any),
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Divergent(divergent) => Type::Divergent(divergent),
            ClassBase::Generic | ClassBase::TypedDict(_) => effects.known_instance(env, KnownClass::Type).await?,
        };
        Ok(ClassMetaclass::Selected(metaclass))
    }
}

/// Supplies ordinary database reads and inference to the shared class-base dispatch.
pub(super) struct InlineClassBaseMetaclassEffects<'db>(pub(super) &'db dyn Db);

impl<'db> SynchronousClassBaseMetaclassEffects<'db> for InlineClassBaseMetaclassEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn class_metaclass(&self, class: ClassType<'db>) -> Result<ClassMetaclass<'db>, Self::Error> {
        Ok(class.inferred_metaclass(self.0))
    }

    fn subclass_is_stub(&self, subclass: ClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(subclass.file(self.0).is_stub(self.0))
    }

    fn subclass_is_standard_library(&self, subclass: ClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(file_to_module(self.0, subclass.program_file(self.0).resolver_file(self.0))
            .and_then(|module| module.search_path(self.0))
            .is_some_and(SearchPath::is_standard_library))
    }

    fn known_class_literal(
        &self,
        env: &ProgramEnvironment<'db>,
        known: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(known.to_class_literal(self.0, env))
    }

    fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        known: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(known.to_instance(self.0, env))
    }
}
