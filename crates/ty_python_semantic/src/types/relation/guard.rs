//! Recursion-guard admission for comparisons whose cache checks can request other relations.

use std::future::Future;

use super::dependencies::RelationDependencies;
use super::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::Db;
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
#[cfg(test)]
use crate::types::cyclic::RelationGuardControl;
use crate::types::cyclic::{
    CycleDetectorCachedVisit, CycleDetectorLookup, CycleDetectorScope, CycleDetectorVisit,
};
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::types::cyclic::{CycleGuardControl, RelationGuardError};

pub(super) type RelationKey<'db> = (Type<'db>, Type<'db>, TypeRelation, TypeVarEvaluation);

pub(super) type RelationScope<'a, 'c, 'db> =
    CycleDetectorScope<'a, 'db, TypeRelation, RelationKey<'db>, ConstraintSet<'db, 'c>, 1>;

type CachedRelation<'a, 'c, 'db> =
    CycleDetectorCachedVisit<'a, 'db, TypeRelation, RelationKey<'db>, ConstraintSet<'db, 'c>, 1>;

pub(super) enum RelationGuardStep<'a, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    CheckCached(PendingCachedRelation<'a, 'c, 'db>),
    Cycle {
        source: Type<'db>,
        target: Type<'db>,
    },
    Evaluate(RelationScope<'a, 'c, 'db>),
}

impl<'a, 'c, 'db> RelationGuardStep<'a, 'c, 'db> {
    pub(super) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let lookup = dependencies.run(db, || {
            checker.relation_visitor.lookup_visit(
                db,
                (source, target, checker.relation, checker.typevar_evaluation),
            )
        })?;
        Ok(match lookup {
            CycleDetectorLookup::Visit(visit) => Self::from_visit(visit),
            CycleDetectorLookup::Cached(cached) => Self::from_cached(cached, checker),
        })
    }

    #[cfg(test)]
    pub(super) fn start_with<C: RelationGuardControl<'db>>(
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        control: &mut C,
    ) -> Result<Self, RelationGuardError<C::Error>> {
        let lookup = checker.relation_visitor.lookup_visit_with(
            db,
            (source, target, checker.relation, checker.typevar_evaluation),
            control,
        )?;
        Ok(match lookup {
            CycleDetectorLookup::Visit(visit) => Self::from_visit(visit),
            CycleDetectorLookup::Cached(cached) => Self::from_cached(cached, checker),
        })
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(super) fn start_admitted<C: CycleGuardControl<'db, RelationKey<'db>>>(
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        control: &mut C,
    ) -> Result<Self, RelationGuardError<C::Error>> {
        let lookup = checker.relation_visitor.lookup_visit_admitted(
            db,
            (source, target, checker.relation, checker.typevar_evaluation),
            control,
        )?;
        Ok(match lookup {
            CycleDetectorLookup::Visit(visit) => Self::from_visit(visit),
            CycleDetectorLookup::Cached(cached) => Self::from_cached(cached, checker),
        })
    }

    #[inline]
    fn from_cached(
        cached: CachedRelation<'a, 'c, 'db>,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
    ) -> Self {
        if checker.is_context_collection_enabled() {
            // Completed constraints do not retain explanations. Whether this entry needs
            // recomputation is itself a semantic question and can require child comparisons.
            Self::CheckCached(PendingCachedRelation { cached })
        } else {
            Self::Complete(cached.into_result())
        }
    }

    fn from_visit(
        visit: CycleDetectorVisit<
            RelationKey<'db>,
            ConstraintSet<'db, 'c>,
            RelationScope<'a, 'c, 'db>,
        >,
    ) -> Self {
        match visit {
            CycleDetectorVisit::Ready(result) => Self::Complete(result),
            CycleDetectorVisit::Cycle((source, target, ..)) => Self::Cycle { source, target },
            CycleDetectorVisit::Pending(scope) => Self::Evaluate(scope),
        }
    }
}

pub(super) struct PendingCachedRelation<'a, 'c, 'db> {
    cached: CachedRelation<'a, 'c, 'db>,
}

impl<'a, 'c, 'db> PendingCachedRelation<'a, 'c, 'db> {
    pub(super) fn constraints(&self) -> ConstraintSet<'db, 'c> {
        *self.cached.result()
    }

    pub(super) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        is_never_satisfied: bool,
        dependencies: &D,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, D::Error> {
        dependencies.run(db, || {
            if is_never_satisfied {
                RelationGuardStep::from_visit(self.cached.recompute(db))
            } else {
                RelationGuardStep::Complete(self.cached.into_result())
            }
        })
    }

    #[cfg(test)]
    pub(super) fn resume_with<C: RelationGuardControl<'db>>(
        &self,
        db: &'db dyn Db,
        is_never_satisfied: bool,
        control: &mut C,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, RelationGuardError<C::Error>> {
        if is_never_satisfied {
            Ok(RelationGuardStep::from_visit(
                self.cached.recompute_with(db, control)?,
            ))
        } else {
            Ok(RelationGuardStep::Complete(*self.cached.result()))
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(super) fn resume_admitted<C: CycleGuardControl<'db, RelationKey<'db>>>(
        self,
        db: &'db dyn Db,
        is_never_satisfied: bool,
        control: &mut C,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, RelationGuardError<C::Error>> {
        if is_never_satisfied {
            Ok(RelationGuardStep::from_visit(
                self.cached.recompute_admitted(db, control)?,
            ))
        } else {
            Ok(RelationGuardStep::Complete(self.cached.into_result()))
        }
    }
}

pub(super) trait RelationGuardEffects<'a, 'c: 'a, 'db: 'c>: Sized {
    type Error;
    type Prepared;

    async fn start(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, Self::Error>;

    async fn complete(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn child<F>(
        &self,
        work: impl FnOnce() -> F,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    async fn prepare_finish(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        scope: &RelationScope<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> Result<Self::Prepared, Self::Error>;

    async fn commit_finish(
        &self,
        scope: RelationScope<'a, 'c, 'db>,
        prepared: Self::Prepared,
        result: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn resume(
        &self,
        pending: PendingCachedRelation<'a, 'c, 'db>,
        is_never_satisfied: bool,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, Self::Error>;

    async fn recursive_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
}

pub(super) async fn with_relation_guard<'a, 'c: 'a, 'db: 'c, E, F>(
    checker: &TypeRelationChecker<'a, 'c, 'db>,
    source: Type<'db>,
    target: Type<'db>,
    work: impl FnOnce() -> F,
    effects: &E,
) -> Result<ConstraintSet<'db, 'c>, E::Error>
where
    E: RelationGuardEffects<'a, 'c, 'db>,
    F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
{
    let mut step = effects.start(checker, source, target).await?;
    loop {
        step = match step {
            RelationGuardStep::Complete(result) => return effects.complete(checker, result).await,
            RelationGuardStep::Evaluate(scope) => {
                // Keep the active scope outside child execution so interruption drains children first.
                let result = effects.child(work).await?;
                let prepared = effects.prepare_finish(checker, &scope, result).await?;
                return effects.commit_finish(scope, prepared, result).await;
            }
            RelationGuardStep::Cycle { source, target } => {
                return effects.recursive_fallback(checker, source, target).await;
            }
            RelationGuardStep::CheckCached(pending) => {
                let never = effects
                    .is_never_satisfied(checker, pending.constraints())
                    .await?;
                effects.resume(pending, never).await?
            }
        };
    }
}

pub(super) struct InlineGuard<'effects, 'db, D> {
    db: &'db dyn Db,
    dependencies: &'effects D,
}

impl<'effects, 'db, D> InlineGuard<'effects, 'db, D> {
    pub(super) fn new(db: &'db dyn Db, dependencies: &'effects D) -> Self {
        Self { db, dependencies }
    }
}

impl<'a, 'c: 'a, 'db: 'c, D: RelationDependencies> RelationGuardEffects<'a, 'c, 'db>
    for InlineGuard<'_, 'db, D>
{
    type Error = D::Error;
    type Prepared = ();

    async fn start(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, Self::Error> {
        RelationGuardStep::start(self.db, checker, source, target, self.dependencies)
    }

    async fn complete(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        Ok(result)
    }

    async fn child<F>(
        &self,
        work: impl FnOnce() -> F,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>,
    {
        work().await
    }

    async fn prepare_finish(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        _scope: &RelationScope<'a, 'c, 'db>,
        _result: ConstraintSet<'db, 'c>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(())
    }

    async fn commit_finish(
        &self,
        scope: RelationScope<'a, 'c, 'db>,
        _prepared: Self::Prepared,
        result: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || scope.finish(result))
    }

    async fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error> {
        self.dependencies.run(self.db, || {
            constraints.is_never_satisfied(self.db, checker.env)
        })
    }

    async fn resume(
        &self,
        pending: PendingCachedRelation<'a, 'c, 'db>,
        is_never_satisfied: bool,
    ) -> Result<RelationGuardStep<'a, 'c, 'db>, Self::Error> {
        pending.resume(self.db, is_never_satisfied, self.dependencies)
    }

    async fn recursive_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || {
            checker.recursive_type_pair_fallback(self.db, source, target)
        })
    }
}
