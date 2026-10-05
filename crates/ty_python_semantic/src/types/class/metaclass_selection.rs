//! Metaclass selection from stored class headers, with the ordinary inference fallback.

use super::{ClassMetaclass, MetaclassError, MetaclassErrorKind};
use crate::types::{KnownClass, MetaclassTransformInfo, StaticClassLiteral, SubclassOfType, Type};

pub(in crate::types) type MetaclassSelectionResult<'db> =
    Result<(ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>>;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousStaticMetaclassEffects)]
    pub(in crate::types) trait StaticMetaclassEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn known_class(&self, class: StaticClassLiteral<'db>, known: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn try_metaclass_inner(&self, class: StaticClassLiteral<'db>) -> Result<MetaclassSelectionResult<'db>, Self::Error>;
        #[operation(child)]
        async fn try_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<MetaclassSelectionResult<'db>, Self::Error>;
        #[operation(child)]
        async fn inferred_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<ClassMetaclass<'db>, Self::Error>;
    }

    #[synchronous(static_try_metaclass_sync)]
    #[capabilities(effects = StaticMetaclassEffects)]
    #[passive_values(ClassMetaclass::Selected, KnownClass::Type)]
    pub(in crate::types) async fn static_try_metaclass_with<'db, E: StaticMetaclassEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<MetaclassSelectionResult<'db>, E::Error> {
        if !effects.has_explicit_bases(class).await?
            && !effects.has_explicit_metaclass(class).await?
        {
            let metaclass = effects.known_class(class, KnownClass::Type).await?;
            return Ok(Ok((ClassMetaclass::Selected(metaclass), None)));
        }
        effects.try_metaclass_inner(class).await
    }

    #[synchronous(static_inferred_metaclass_sync)]
    #[capabilities(effects = StaticMetaclassEffects)]
    #[passive_values(ClassMetaclass::Selected, Type::from, SubclassOfType::subclass_of_unknown)]
    pub(in crate::types) async fn static_inferred_metaclass_with<'db, E: StaticMetaclassEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<ClassMetaclass<'db>, E::Error> {
        match effects.try_metaclass(class).await? {
            Ok((metaclass, _)) => Ok(metaclass),
            Err(error) => match error.kind {
                MetaclassErrorKind::Conflict {
                    explicit_metaclass: Some(metaclass),
                    ..
                } => Ok(ClassMetaclass::Selected(Type::from(metaclass))),
                _ => Ok(ClassMetaclass::Selected(SubclassOfType::subclass_of_unknown())),
            },
        }
    }

    #[synchronous(static_metaclass_sync)]
    #[capabilities(effects = StaticMetaclassEffects)]
    #[passive_values(ClassMetaclass::lookup_target)]
    pub(in crate::types) async fn static_metaclass_with<'db, E: StaticMetaclassEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match ClassMetaclass::lookup_target(effects.inferred_metaclass(class).await?) {
            Ok(metaclass) => Ok(metaclass),
            Err(known) => effects.known_class(class, known).await,
        }
    }
}
