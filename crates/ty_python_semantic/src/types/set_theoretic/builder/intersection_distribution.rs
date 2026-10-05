//! Ordered intersection distribution with shared bounded and unbounded decisions.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::ops::ControlFlow;

use ty_mapping_probe_macros::shared_semantic_family;

use super::intersection_assembly::IntersectionBranches;
use super::intersection_distribution_storage::{DistributionSet, RemovalIndices};
use super::{InnerIntersectionBuilder, IntersectionBuilder, IntersectionLimits};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct DistributionFacts;

pub(super) struct OrdinaryDistributionEffects<'a, 'db, L> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    limits: PhantomData<L>,
}

impl<'a, 'db, L> OrdinaryDistributionEffects<'a, 'db, L> {
    pub(super) fn new(db: &'db dyn Db, env: &'a ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env,
            limits: PhantomData,
        }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousDistributionEffects)]
    // Each owned input stays outside a rejectable callback until admission. Taking
    // branches, cloning and buffer growth also prepay cleanup on refusal or unwind.
    pub(in crate::types) trait DistributionEffects<'db> {
        type Error;
        type Break;

        #[operation(local)]
        async fn bounded(&self) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn take_branches(&self, child: &mut IntersectionBuilder<'db>) -> Result<IntersectionBranches<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_candidate(&self, branches: &mut IntersectionBranches<'db>) -> Result<Option<InnerIntersectionBuilder<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_branches(&self, branches: IntersectionBranches<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contains_never(&self, candidate: &InnerIntersectionBuilder<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn build_candidate(&self, candidate: &InnerIntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_old<'a>(&self, distributed: &'a DistributionSet<'db>, cursor: &mut usize) -> Result<Option<(usize, &'a InnerIntersectionBuilder<'db>)>, Self::Error>;
        #[operation(source)]
        async fn is_redundant(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn new_removals(&self) -> Result<RemovalIndices, Self::Error>;
        #[operation(local)]
        async fn defer_removal(&self, removals: &mut RemovalIndices, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn apply_removals(&self, distributed: &mut DistributionSet<'db>, removals: RemovalIndices) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn check_terms(&self, distributed: &DistributionSet<'db>) -> Result<ControlFlow<Self::Break>, Self::Error>;
        #[operation(local)]
        async fn insert(&self, distributed: &mut DistributionSet<'db>, candidate: InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn discard(&self, candidate: InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl DistributionFacts {
        fn is_never(&self, ty: Type<'_>) -> bool { ty.is_never() }
    }

    /// Add DNF branches, dropping `Never` and duplicate branches so later distribution does not
    /// multiply dead or repeated branches.
    #[synchronous(extend_sync)]
    #[capabilities(effects = DistributionEffects, facts = DistributionFacts)]
    #[passive_values(ControlFlow::Continue, ControlFlow::Break)]
    pub(in crate::types) async fn extend_with<'db, E: DistributionEffects<'db>>(
        child: &mut IntersectionBuilder<'db>,
        distributed: &mut DistributionSet<'db>,
        check_budget: bool,
        facts: DistributionFacts,
        effects: &E,
    ) -> Result<ControlFlow<E::Break>, E::Error> {
        let mut branches = effects.take_branches(child).await?;
        let bounded = effects.bounded().await?;
        // Retain the whole first disjunction: a later factor can eliminate all but a few of its
        // alternatives, including alternatives that occur beyond the budget's position.
        if !bounded || !check_budget {
            #[cursor_loop]
            while let Some(candidate) = effects.next_candidate(&mut branches).await? {
                if effects.contains_never(&candidate).await? {
                    effects.discard(candidate).await?;
                } else {
                    effects.insert(distributed, candidate).await?;
                }
            }
            effects.finish_branches(branches).await?;
            return Ok(ControlFlow::Continue(()));
        }

        #[cursor_loop]
        while let Some(candidate) = effects.next_candidate(&mut branches).await? {
            // Some branches only collapse during `build`, for example when a constrained
            // type variable has no remaining constraints. Those do not consume the budget.
            let candidate_type = effects.build_candidate(&candidate).await?;
            if facts.is_never(candidate_type) {
                effects.discard(candidate).await?;
                continue;
            }
            let mut cursor = 0;
            #[passive_state]
            let mut redundant = false;
            #[cursor_loop]
            while let Some(old) = effects.next_old(distributed, &mut cursor).await? {
                let (_, old) = old;
                let old_type = effects.build_candidate(old).await?;
                if effects.is_redundant(candidate_type, old_type).await? {
                    redundant = true;
                    break;
                }
            }
            if redundant {
                effects.discard(candidate).await?;
                continue;
            }

            let mut removals = effects.new_removals().await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(old) = effects.next_old(distributed, &mut cursor).await? {
                let (index, old) = old;
                let old_type = effects.build_candidate(old).await?;
                if effects.is_redundant(old_type, candidate_type).await? {
                    effects.defer_removal(&mut removals, index).await?;
                }
            }
            effects.apply_removals(distributed, removals).await?;
            if let ControlFlow::Break(value) = effects.check_terms(distributed).await? {
                effects.discard(candidate).await?;
                effects.finish_branches(branches).await?;
                return Ok(ControlFlow::Break(value));
            }
            effects.insert(distributed, candidate).await?;
        }
        effects.finish_branches(branches).await?;
        Ok(ControlFlow::Continue(()))
    }
}

impl<'db, L: IntersectionLimits> SynchronousDistributionEffects<'db>
    for OrdinaryDistributionEffects<'_, 'db, L>
{
    type Error = Infallible;
    type Break = L::Break;

    fn bounded(&self) -> Result<bool, Self::Error> {
        Ok(L::BOUNDED)
    }

    fn take_branches(
        &self,
        child: &mut IntersectionBuilder<'db>,
    ) -> Result<IntersectionBranches<'db>, Self::Error> {
        Ok(IntersectionBranches::take(child))
    }

    fn next_candidate(
        &self,
        branches: &mut IntersectionBranches<'db>,
    ) -> Result<Option<InnerIntersectionBuilder<'db>>, Self::Error> {
        Ok(branches.next())
    }

    fn finish_branches(&self, branches: IntersectionBranches<'db>) -> Result<(), Self::Error> {
        drop(branches);
        Ok(())
    }

    fn contains_never(
        &self,
        candidate: &InnerIntersectionBuilder<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(candidate.contains_never())
    }

    fn build_candidate(
        &self,
        candidate: &InnerIntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(candidate.clone().build(self.db, self.env))
    }

    fn next_old<'a>(
        &self,
        distributed: &'a DistributionSet<'db>,
        cursor: &mut usize,
    ) -> Result<Option<(usize, &'a InnerIntersectionBuilder<'db>)>, Self::Error> {
        Ok(distributed.next(cursor))
    }

    fn is_redundant(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        Ok(first.is_redundant_with(self.db, self.env, second))
    }

    fn new_removals(&self) -> Result<RemovalIndices, Self::Error> {
        Ok(RemovalIndices::default())
    }

    fn defer_removal(
        &self,
        removals: &mut RemovalIndices,
        index: usize,
    ) -> Result<(), Self::Error> {
        removals.push(index);
        Ok(())
    }

    fn apply_removals(
        &self,
        distributed: &mut DistributionSet<'db>,
        removals: RemovalIndices,
    ) -> Result<(), Self::Error> {
        distributed.apply_removals(removals);
        Ok(())
    }

    fn check_terms(
        &self,
        distributed: &DistributionSet<'db>,
    ) -> Result<ControlFlow<Self::Break>, Self::Error> {
        Ok(L::check_terms(distributed.len() + 1))
    }

    fn insert(
        &self,
        distributed: &mut DistributionSet<'db>,
        candidate: InnerIntersectionBuilder<'db>,
    ) -> Result<(), Self::Error> {
        distributed.insert(candidate);
        Ok(())
    }

    fn discard(&self, candidate: InnerIntersectionBuilder<'db>) -> Result<(), Self::Error> {
        drop(candidate);
        Ok(())
    }
}
