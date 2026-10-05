//! Shared lookup of a constructor's `__new__` member before descriptor binding.

use std::convert::Infallible;

use crate::place::PlaceAndQualifiers;
use crate::types::{KnownClass, MemberLookupPolicy, Type};
use crate::{Db, ProgramEnvironment};

/// Selects which inherited `__new__` definitions participate in construction.
#[derive(Debug)]
pub(in crate::types) struct NewLookupFacts;

/// Evaluates constructor member lookup with the ordinary query dependencies.
pub(in crate::types) struct OrdinaryNewLookupEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousNewLookupEffects)]
    pub(in crate::types) trait NewLookupEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn type_instance(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn is_subtype(&self, ty: Type<'db>, target: Type<'db>, env: &ProgramEnvironment<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn lookup(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl NewLookupFacts {
        fn policy(&self, is_type_instance: bool) -> MemberLookupPolicy {
            let mut policy = MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK;
            if !is_type_instance {
                policy |= MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK;
            }
            policy
        }
    }

    /// Looks up `__new__`, omitting `object` and permitting `type` fallback only for subtypes of `type`.
    #[synchronous(lookup_dunder_new_sync)]
    #[capabilities(effects = NewLookupEffects, facts = NewLookupFacts)]
    #[passive_values()]
    pub(in crate::types) async fn lookup_dunder_new_with<'db, E: NewLookupEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        facts: NewLookupFacts,
        effects: &E,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, E::Error> {
        effects.checkpoint().await?;
        let target = effects.type_instance(env).await?;
        let is_type_instance = effects.is_subtype(ty, target, env).await?;
        let policy = facts.policy(is_type_instance);
        effects.lookup(ty, env, policy).await
    }
}

impl<'db> SynchronousNewLookupEffects<'db> for OrdinaryNewLookupEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn type_instance(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Type.to_instance(self.db, env))
    }

    fn is_subtype(
        &self,
        ty: Type<'db>,
        target: Type<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.is_subtype_of(self.db, env, target))
    }

    fn lookup(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        Ok(ty.find_name_in_mro_with_policy(self.db, env, "__new__", policy))
    }
}
