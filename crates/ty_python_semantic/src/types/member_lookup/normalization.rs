use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::place::PlaceAndQualifiers;
use crate::types::{
    LookupFacts, LookupParts, MemberLookupResult, member_lookup_result_with_origin,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct MemberNormalizationFacts;

pub(in crate::types) struct OrdinaryMemberNormalizationEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousMemberNormalizationEffects)]
    pub(in crate::types) trait MemberNormalizationEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn parts(
            &self,
            result: MemberLookupResult<'db>,
        ) -> Result<LookupParts<'db>, Self::Error>;
        #[operation(child)]
        async fn normalize_place(
            &self,
            env: &ProgramEnvironment<'db>,
            current: PlaceAndQualifiers<'db>,
            previous: PlaceAndQualifiers<'db>,
            cycle: &salsa::Cycle<'_>,
        ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(local)]
        async fn result(
            &self,
            parts: LookupParts<'db>,
        ) -> Result<MemberLookupResult<'db>, Self::Error>;
    }

    #[finite_capability]
    impl MemberNormalizationFacts {
        fn retain_error(&self, cycle: &salsa::Cycle<'_>, previous: MemberLookupResult<'_>) -> bool {
            cycle.iteration() <= crate::TAINTED_CYCLES || previous.is_err()
        }
    }

    #[synchronous(member_cycle_normalized_sync)]
    #[capabilities(effects = MemberNormalizationEffects, facts = MemberNormalizationFacts)]
    #[passive_values(LookupParts)]
    pub(in crate::types) async fn member_cycle_normalized_with<'db, E: MemberNormalizationEffects<'db>>(
        current: MemberLookupResult<'db>,
        env: &ProgramEnvironment<'db>,
        previous: MemberLookupResult<'db>,
        cycle: &salsa::Cycle<'_>,
        facts: MemberNormalizationFacts,
        effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let retain_error = facts.retain_error(cycle, previous);
        let current = effects.parts(current).await?;
        let previous = effects.parts(previous).await?;
        let member = effects
            .normalize_place(env, current.member, previous.member, cycle)
            .await?;
        effects.result(LookupParts {
            member,
            error: if retain_error { current.error } else { None },
            ..current
        }).await
    }
}

impl<'db> SynchronousMemberNormalizationEffects<'db> for OrdinaryMemberNormalizationEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn parts(&self, result: MemberLookupResult<'db>) -> Result<LookupParts<'db>, Infallible> {
        Ok(LookupFacts.parts(salsa::FieldReads::new(self.db), result))
    }

    fn normalize_place(
        &self,
        env: &ProgramEnvironment<'db>,
        current: PlaceAndQualifiers<'db>,
        previous: PlaceAndQualifiers<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(current.cycle_normalized(self.db, env, previous, cycle))
    }

    fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(member_lookup_result_with_origin(
            self.db,
            parts.member,
            parts.error,
            parts.properties,
            parts.descriptor,
        ))
    }
}
