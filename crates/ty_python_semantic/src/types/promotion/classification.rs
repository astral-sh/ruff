//! Singleton classification with explicit nominal fields and enum metadata dependencies.

use super::PublicPromotionFacts;
use crate::types::instance::{ExplicitAnyInstanceClass, NominalInstanceClass};
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, KnownClass, NominalInstanceType, StaticClassLiteral,
};
use crate::Db;

/// The stored nominal representation needed to choose singleton classification dependencies.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum SingletonRepresentation<'db> {
    Object,
    ExactTuple,
    SysVersionInfo,
    NonTuple(NominalInstanceClass<'db>),
}

/// Fixed decisions and transfers surrounding singleton field reads and enum metadata requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SingletonClassificationWork {
    Dispatch,
    ClassDispatch,
    LiteralDispatch,
    KnownDecision,
    EnumRequest,
    Result,
}

/// Reads copied nominal representations and the finite known-class singleton table.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct SingletonFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSingletonEffects)]
    pub(in crate::types) trait SingletonEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: SingletonClassificationWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn explicit_any_class(&self, class: ExplicitAnyInstanceClass<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn generic_origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn static_known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn enum_singleton(&self, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl SingletonFacts {
        fn representation<'db>(&self, instance: NominalInstanceType<'db>) -> SingletonRepresentation<'db> {
            instance.singleton_representation()
        }

        fn known_singleton(&self, known: KnownClass) -> bool {
            known.is_singleton()
        }
    }

    /// Classifies a nominal instance as a singleton, consulting enum metadata only for a class
    /// without a known-class tag. An empty tuple is classified as non-singleton.
    #[synchronous(classify_singleton_sync)]
    #[capabilities(effects = SingletonEffects, facts = SingletonFacts)]
    #[passive_values(SingletonClassificationWork::Dispatch, SingletonClassificationWork::ClassDispatch, SingletonClassificationWork::LiteralDispatch, SingletonClassificationWork::KnownDecision, SingletonClassificationWork::EnumRequest, SingletonClassificationWork::Result, ClassLiteral::Static)]
    pub(in crate::types) async fn classify_singleton_with<'db, E: SingletonEffects<'db>>(
        instance: NominalInstanceType<'db>,
        facts: SingletonFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint(SingletonClassificationWork::Dispatch).await?;
        let singleton = match facts.representation(instance) {
            SingletonRepresentation::Object | SingletonRepresentation::ExactTuple => false,
            SingletonRepresentation::SysVersionInfo => true,
            SingletonRepresentation::NonTuple(class) => {
                effects.checkpoint(SingletonClassificationWork::ClassDispatch).await?;
                let class = match class {
                    NominalInstanceClass::Plain(class) => class,
                    NominalInstanceClass::InheritsFromExplicitAny(class) => {
                        effects.explicit_any_class(class).await?
                    }
                };
                let literal = match class {
                    ClassType::NonGeneric(literal) => literal,
                    ClassType::Generic(alias) => {
                        ClassLiteral::Static(effects.generic_origin(alias).await?)
                    }
                };
                effects.checkpoint(SingletonClassificationWork::LiteralDispatch).await?;
                let known = match literal {
                    ClassLiteral::Static(class) => effects.static_known(class).await?,
                    ClassLiteral::Dynamic(_)
                    | ClassLiteral::DynamicNamedTuple(_)
                    | ClassLiteral::DynamicTypedDict(_)
                    | ClassLiteral::DynamicEnum(_) => None,
                };
                effects.checkpoint(SingletonClassificationWork::KnownDecision).await?;
                match known {
                    Some(known) => facts.known_singleton(known),
                    None => {
                        effects.checkpoint(SingletonClassificationWork::EnumRequest).await?;
                        effects.enum_singleton(literal).await?
                    }
                }
            }
        };
        effects.checkpoint(SingletonClassificationWork::Result).await?;
        Ok(singleton)
    }
}

/// Supplies ordinary nominal field reads and a caller's existing synchronous enum facts.
pub(in crate::types) struct OrdinarySingletonEffects<'a, 'db, E> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) facts: &'a E,
}

impl<'db, E: PublicPromotionFacts<'db>> SynchronousSingletonEffects<'db>
    for OrdinarySingletonEffects<'_, 'db, E>
{
    type Error = E::Error;

    fn checkpoint(&self, _work: SingletonClassificationWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.class(self.db))
    }

    fn generic_origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(self.db))
    }

    fn static_known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }

    fn enum_singleton(&self, class: ClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.facts.enum_singleton(self.db, class)
    }
}
