//! Protocol classification uses stored identity before inspecting explicit bases.

use std::convert::Infallible;

use crate::Db;
use crate::types::{KnownClass, StaticClassLiteral, Type};

pub(super) struct InlineProtocolStatusEffects<'db> {
    pub(super) db: &'db dyn Db,
}

struct ProtocolStatusFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousProtocolStatusEffects)]
    pub(in crate::types) trait ProtocolStatusEffects<'db> {
        type Error;

        #[operation(source)]
        async fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(source)]
        async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn classify_bases(&self, bases: &[Type<'db>]) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl ProtocolStatusFacts {
        fn known_is_protocol(&self, known: KnownClass) -> bool {
            known.is_protocol()
        }
    }

    #[synchronous(static_is_protocol_impl_sync)]
    #[capabilities(effects = ProtocolStatusEffects, facts = ProtocolStatusFacts)]
    #[passive_values()]
    async fn static_is_protocol_impl_with<'db, E: ProtocolStatusEffects<'db>>(
        class: StaticClassLiteral<'db>,
        facts: ProtocolStatusFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if let Some(known) = effects.known(class).await? {
            return Ok(facts.known_is_protocol(known));
        }
        if !effects.has_explicit_bases(class).await? {
            return Ok(false);
        }
        let bases = effects.explicit_bases(class).await?;
        effects.classify_bases(bases).await
    }
}

impl<'db> SynchronousProtocolStatusEffects<'db> for InlineProtocolStatusEffects<'db> {
    type Error = Infallible;

    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(self.db))
    }

    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_explicit_bases(self.db))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    fn classify_bases(&self, bases: &[Type<'db>]) -> Result<bool, Infallible> {
        Ok(StaticClassLiteral::protocol_explicit_bases(bases))
    }
}

pub(in crate::types) async fn static_is_protocol_with<'db, E: ProtocolStatusEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    static_is_protocol_impl_with(class, ProtocolStatusFacts, effects).await
}

pub(in crate::types) fn static_is_protocol_sync<'db, E: SynchronousProtocolStatusEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    static_is_protocol_impl_sync(class, ProtocolStatusFacts, effects)
}
