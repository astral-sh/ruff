//! Finite narrowing decisions shared by ordinary queries and controlled source inference.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::narrowing_constraints::ScopedNarrowingConstraint;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::{NarrowingEvaluator, PredicateNarrowingTargets};

use super::NarrowingProjector;
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

pub(crate) struct NarrowingEntryFacts;

shared_semantic_family! {
    #[synchronous(SynchronousNarrowingEntryEffects)]
    pub(crate) trait NarrowingEntryEffects<'db> {
        type Error;

        #[operation(local)]
        async fn create_projector<'map>(
            &self,
            env: &'map ProgramEnvironment<'db>,
            evaluator: &NarrowingEvaluator<'map, 'db>,
            place: ScopedPlaceId,
            base_ty: Type<'db>,
        ) -> Result<NarrowingProjector<'map, 'db>, Self::Error>
        where
            'db: 'map;
        #[operation(local)]
        async fn set_base_type(&self, projector: &mut NarrowingProjector<'_, 'db>, base_ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contains_place(&self, targets: &PredicateNarrowingTargets, place: ScopedPlaceId) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn narrow_projector(&self, projector: &mut NarrowingProjector<'_, 'db>, constraint: ScopedNarrowingConstraint, base_ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn narrow_graph(&self, projector: &mut NarrowingProjector<'_, 'db>, constraint: ScopedNarrowingConstraint, base_ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn retire_projector(&self, projector: NarrowingProjector<'_, 'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl NarrowingEntryFacts {
        fn constraint(&self, evaluator: &NarrowingEvaluator<'_, '_>) -> ScopedNarrowingConstraint {
            evaluator.constraint()
        }

        fn terminal<'db>(&self, constraint: ScopedNarrowingConstraint, base_ty: Type<'db>) -> Option<Type<'db>> {
            match constraint {
                ScopedNarrowingConstraint::ALWAYS_TRUE => Some(base_ty),
                ScopedNarrowingConstraint::ALWAYS_FALSE => Some(Type::Never),
                _ => None,
            }
        }

        fn targets<'map>(&self, projector: &NarrowingProjector<'map, '_>) -> &'map PredicateNarrowingTargets {
            projector.predicate_narrowing_targets
        }

        fn place(&self, projector: &NarrowingProjector<'_, '_>) -> ScopedPlaceId {
            projector.place
        }
    }

    #[synchronous(narrow_type_by_constraint_sync)]
    #[capabilities(effects = NarrowingEntryEffects, facts = NarrowingEntryFacts)]
    #[passive_values()]
    pub(crate) async fn narrow_type_by_constraint_with<'map, 'db, E: NarrowingEntryEffects<'db>>(
        env: &'map ProgramEnvironment<'db>,
        evaluator: &NarrowingEvaluator<'map, 'db>,
        base_ty: Type<'db>,
        place: ScopedPlaceId,
        facts: NarrowingEntryFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let constraint = facts.constraint(evaluator);
        if let Some(ty) = facts.terminal(constraint, base_ty) {
            return Ok(ty);
        }

        let mut projector = effects.create_projector(env, evaluator, place, base_ty).await?;
        let ty = effects.narrow_projector(&mut projector, constraint, base_ty).await?;
        effects.retire_projector(projector).await?;
        Ok(ty)
    }

    #[synchronous(narrow_projector_sync)]
    #[capabilities(effects = NarrowingEntryEffects, facts = NarrowingEntryFacts)]
    #[passive_values()]
    pub(crate) async fn narrow_projector_with<'db, E: NarrowingEntryEffects<'db>>(
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
        facts: NarrowingEntryFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.set_base_type(projector, base_ty).await?;
        if let Some(ty) = facts.terminal(constraint, base_ty) {
            return Ok(ty);
        }

        // Reachability gates can mention predicates that do not narrow this place.
        // Avoid evaluating unrelated expressions, which can introduce inference cycles.
        if !effects.contains_place(facts.targets(projector), facts.place(projector)).await? {
            return Ok(base_ty);
        }

        effects.narrow_graph(projector, constraint, base_ty).await
    }
}

pub(crate) struct OrdinaryNarrowingEntryEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> OrdinaryNarrowingEntryEffects<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousNarrowingEntryEffects<'db> for OrdinaryNarrowingEntryEffects<'db> {
    type Error = Infallible;

    fn create_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        evaluator: &NarrowingEvaluator<'map, 'db>,
        place: ScopedPlaceId,
        base_ty: Type<'db>,
    ) -> Result<NarrowingProjector<'map, 'db>, Self::Error>
    where
        'db: 'map,
    {
        Ok(NarrowingProjector::new(
            self.db,
            env,
            evaluator.narrowing_constraints(),
            evaluator.predicates(),
            evaluator.predicate_narrowing_targets(),
            place,
            base_ty,
        ))
    }

    fn set_base_type(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        base_ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        projector.set_base_type(base_ty);
        Ok(())
    }

    fn contains_place(
        &self,
        targets: &PredicateNarrowingTargets,
        place: ScopedPlaceId,
    ) -> Result<bool, Self::Error> {
        Ok(targets.contains_place(place))
    }

    fn narrow_projector(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        narrow_projector_sync(projector, constraint, base_ty, NarrowingEntryFacts, self)
    }

    fn narrow_graph(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let root = projector.project(constraint, true);
        Ok(projector.narrow_projected(root, base_ty))
    }

    fn retire_projector(&self, projector: NarrowingProjector<'_, 'db>) -> Result<(), Self::Error> {
        drop(projector);
        Ok(())
    }
}
