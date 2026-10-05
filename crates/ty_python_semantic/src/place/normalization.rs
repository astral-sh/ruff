use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance};
use crate::types::{Type, TypeQualifiers};
use crate::{Db, ProgramEnvironment};

pub(crate) struct PlaceNormalizationFacts;

pub(super) struct OrdinaryPlaceNormalizationEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousPlaceNormalizationEffects)]
    pub(crate) trait PlaceNormalizationEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn cycle_normalize(
            &self,
            env: &ProgramEnvironment<'db>,
            current: Type<'db>,
            previous: Type<'db>,
            cycle: &salsa::Cycle<'_>,
        ) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn normalize_heads(
            &self,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            cycle: &salsa::Cycle<'_>,
        ) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_head(
            &self,
            heads: &mut salsa::CycleHeadCandidates<'_>,
        ) -> Result<Option<salsa::CycleHeadCandidate>, Self::Error>;
    }

    #[finite_capability]
    impl PlaceNormalizationFacts {
        fn early_iteration(&self, cycle: &salsa::Cycle<'_>) -> bool {
            cycle.iteration() <= 1
        }

        fn union_qualifiers(&self, previous: TypeQualifiers, current: TypeQualifiers) -> TypeQualifiers {
            previous.union(current)
        }

        fn merge_provenance<'db>(&self, previous: Provenance<'db>, current: Provenance<'db>) -> Provenance<'db> {
            previous.or(current)
        }

        fn head_cursor<'cycle>(
            &self,
            cycle: &'cycle salsa::Cycle<'_>,
        ) -> salsa::CycleHeadCandidates<'cycle> {
            cycle.head_candidates()
        }

        fn is_divergent_head(&self, ty: Type<'_>, id: salsa::Id) -> bool {
            ty == Type::divergent(id)
        }
    }

    #[synchronous(place_cycle_normalized_sync)]
    #[capabilities(effects = PlaceNormalizationEffects, facts = PlaceNormalizationFacts)]
    #[passive_values(Place::Defined, Place::Undefined, DefinedPlace, Definedness::AlwaysDefined, Definedness::PossiblyUndefined, PlaceAndQualifiers, salsa::CycleHeadCandidate::Present)]
    pub(crate) async fn place_cycle_normalized_with<'db, E: PlaceNormalizationEffects<'db>>(
        current_place: PlaceAndQualifiers<'db>,
        env: &ProgramEnvironment<'db>,
        previous_place: PlaceAndQualifiers<'db>,
        cycle: &salsa::Cycle<'_>,
        facts: PlaceNormalizationFacts,
        effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint().await?;
        let qualifiers = if facts.early_iteration(cycle) {
            current_place.qualifiers
        } else {
            facts.union_qualifiers(previous_place.qualifiers, current_place.qualifiers)
        };
        let place = match (previous_place.place, current_place.place) {
            // In fixed-point iteration of type inference, the member result must be monotonically
            // widened and not "oscillate". The type component is widened by unioning the previous
            // iteration into the current result; after the first couple iterations, the same
            // applies to boundness and qualifiers.
            (Place::Defined(prev), Place::Defined(current)) => Place::Defined(DefinedPlace {
                ty: effects.cycle_normalize(env, current.ty, prev.ty, cycle).await?,
                definedness: if facts.early_iteration(cycle)
                    || matches!(
                        (prev.definedness, current.definedness),
                        (Definedness::AlwaysDefined, Definedness::AlwaysDefined)
                    ) {
                    current.definedness
                } else {
                    Definedness::PossiblyUndefined
                },
                provenance: facts.merge_provenance(prev.provenance, current.provenance),
                ..current
            }),
            // If a `Place` in the current cycle is `Defined` but `Undefined` in the previous cycle,
            // that means that its definedness depends on the truthiness of the previous cycle value.
            // In this case, the definedness of the current cycle `Place` is set to `PossiblyUndefined`.
            // Actually, this branch is unreachable. We evaluate the truthiness of non-definitely-bound places as Ambiguous (see #19579),
            // so convergence is guaranteed without resorting to this handling.
            // However, the handling described above may reduce the exactness of reachability analysis,
            // so it may be better to remove it. In that case, this branch is necessary.
            (Place::Undefined, Place::Defined(current)) => Place::Defined(DefinedPlace {
                ty: effects.normalize_heads(env, current.ty, cycle).await?,
                definedness: if facts.early_iteration(cycle) {
                    current.definedness
                } else {
                    Definedness::PossiblyUndefined
                },
                ..current
            }),
            // If a `Place` that was `Defined(Divergent)` in the previous cycle is actually found to be unreachable in the current cycle,
            // it is set to `Undefined` (because the cycle initial value does not include meaningful reachability information).
            (Place::Defined(prev), Place::Undefined) => {
                #[passive_state]
                let mut divergent_head = false;
                let mut heads = facts.head_cursor(cycle);
                #[cursor_loop]
                while let Some(candidate) = effects.next_head(&mut heads).await? {
                    if let salsa::CycleHeadCandidate::Present(id) = candidate
                        && facts.is_divergent_head(prev.ty, id)
                    {
                        divergent_head = true;
                        break;
                    }
                }
                if divergent_head {
                    Place::Undefined
                } else {
                    Place::Defined(DefinedPlace {
                        ty: effects.normalize_heads(env, prev.ty, cycle).await?,
                        definedness: Definedness::PossiblyUndefined,
                        ..prev
                    })
                }
            }
            (Place::Undefined, Place::Undefined) => Place::Undefined,
        };
        Ok(PlaceAndQualifiers { place, qualifiers })
    }
}

impl<'db> SynchronousPlaceNormalizationEffects<'db> for OrdinaryPlaceNormalizationEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn cycle_normalize(
        &self,
        env: &ProgramEnvironment<'db>,
        current: Type<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(current.cycle_normalized(self.db, env, previous, cycle))
    }

    fn normalize_heads(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.recursive_type_normalized(self.db, env, cycle))
    }

    fn next_head(
        &self,
        heads: &mut salsa::CycleHeadCandidates<'_>,
    ) -> Result<Option<salsa::CycleHeadCandidate>, Infallible> {
        Ok(heads.next())
    }
}
