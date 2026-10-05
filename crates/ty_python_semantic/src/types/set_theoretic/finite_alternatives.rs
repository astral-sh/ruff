//! Recognize an intersection's enum complement before expanding its exact finite alternatives.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::{IntersectionType, NegativeIntersectionElements};
use crate::types::Type;
use crate::types::enums::EnumComplement;
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(super) struct OrdinaryFiniteAlternativeEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousFiniteAlternativeEffects)]
    pub(in crate::types) trait FiniteAlternativeEffects<'db> {
        type Error;

        #[operation(source)]
        async fn positive(&self, intersection: IntersectionType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn negative(&self, intersection: IntersectionType<'db>) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error>;
        #[operation(child)]
        async fn enum_complement(&self, env: &ProgramEnvironment<'db>, positive: &FxOrderSet<Type<'db>>, negative: &NegativeIntersectionElements<'db>) -> Result<Option<EnumComplement<'db>>, Self::Error>;
        #[operation(child)]
        async fn remaining_literal_union(&self, env: &ProgramEnvironment<'db>, complement: EnumComplement<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = FiniteAlternativeEffects)]
    #[passive_values()]
    pub(in crate::types) async fn produce_with<'db, E: FiniteAlternativeEffects<'db>>(
        intersection: IntersectionType<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let positive = effects.positive(intersection).await?;
        let negative = effects.negative(intersection).await?;
        let Some(complement) = effects.enum_complement(env, positive, negative).await? else {
            return Ok(None);
        };
        Ok(Some(effects.remaining_literal_union(env, complement).await?))
    }
}

impl<'db> SynchronousFiniteAlternativeEffects<'db> for OrdinaryFiniteAlternativeEffects<'db> {
    type Error = Infallible;

    fn positive(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(intersection.positive(self.db))
    }

    fn negative(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error> {
        Ok(intersection.negative(self.db))
    }

    fn enum_complement(
        &self,
        env: &ProgramEnvironment<'db>,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> Result<Option<EnumComplement<'db>>, Self::Error> {
        Ok(EnumComplement::from_intersection_parts(
            self.db, env, positive, negative,
        ))
    }

    fn remaining_literal_union(
        &self,
        env: &ProgramEnvironment<'db>,
        complement: EnumComplement<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(complement.remaining_literal_union(self.db, env))
    }
}
