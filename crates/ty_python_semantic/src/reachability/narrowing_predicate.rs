//! Cached predicate constraints shared by ordinary and controlled narrowing construction.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::predicate::ScopedPredicateId;

use super::NarrowingProjector;
use crate::types::{NarrowingConstraint, infer_narrowing_constraints};

pub(crate) type PredicateConstraints<'db> = (
    Option<NarrowingConstraint<'db>>,
    Option<NarrowingConstraint<'db>>,
);

pub(super) struct OrdinaryNarrowingPredicateEffects;

shared_semantic_family! {
    #[synchronous(SynchronousNarrowingPredicateEffects)]
    pub(crate) trait NarrowingPredicateEffects<'db> {
        type Error;

        #[operation(local)]
        async fn targets_place(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn cached_constraints(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<Option<PredicateConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn infer_constraints(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<PredicateConstraints<'db>, Self::Error>;
        #[operation(local)]
        async fn cache_constraints(&self, projector: &mut NarrowingProjector<'_, 'db>, id: ScopedPredicateId, constraints: &PredicateConstraints<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(predicate_constraints_sync)]
    #[capabilities(effects = NarrowingPredicateEffects)]
    #[passive_values()]
    pub(crate) async fn predicate_constraints_with<'db, E: NarrowingPredicateEffects<'db>>(
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
        effects: &E,
    ) -> Result<PredicateConstraints<'db>, E::Error> {
        if !effects.targets_place(projector, id).await? {
            return Ok((None, None));
        }
        if let Some(cached) = effects.cached_constraints(projector, id).await? {
            return Ok(cached);
        }

        let constraints = effects.infer_constraints(projector, id).await?;
        effects.cache_constraints(projector, id, &constraints).await?;
        Ok(constraints)
    }
}

impl<'db> SynchronousNarrowingPredicateEffects<'db> for OrdinaryNarrowingPredicateEffects {
    type Error = Infallible;

    fn targets_place(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<bool, Self::Error> {
        Ok(projector
            .predicate_narrowing_targets
            .contains(id, projector.place))
    }

    fn cached_constraints(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<Option<PredicateConstraints<'db>>, Self::Error> {
        Ok(projector
            .graph
            .predicate_constraints_cache
            .get(&id)
            .cloned())
    }

    fn infer_constraints(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<PredicateConstraints<'db>, Self::Error> {
        Ok(infer_narrowing_constraints(
            projector.db,
            projector.predicates[id],
            projector.place,
        ))
    }

    fn cache_constraints(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
        constraints: &PredicateConstraints<'db>,
    ) -> Result<(), Self::Error> {
        projector
            .graph
            .predicate_constraints_cache
            .insert(id, constraints.clone());
        Ok(())
    }
}
