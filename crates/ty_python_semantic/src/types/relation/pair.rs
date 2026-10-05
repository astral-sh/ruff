//! Entry and completion of a comparison, with observations published only on completion.

use super::dependencies::RelationDependencies;
use super::{
    RelationObservation, RelationObservationSite, RelationOutcome, TypeArity, TypeRelationChecker,
};
use crate::Db;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::TypeStructureSize;
use crate::types::{BoundTypeVarIdentity, Type};

#[cfg(test)]
use crate::types::constructor::expansion_probe::descriptor_observation;

#[cfg(test)]
mod tests;

pub(super) struct PairEvaluation<'checker, 'a, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(super) source: Type<'db>,
    pub(super) target: Type<'db>,
    #[cfg(test)]
    _observation: Option<descriptor_observation::Scope>,
}

impl<'checker, 'a, 'c, 'db> PairEvaluation<'checker, 'a, 'c, 'db> {
    pub(super) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        dependencies.run(db, || Self {
            checker,
            source,
            target,
            #[cfg(test)]
            _observation: descriptor_observation::within_descriptor().then(|| {
                descriptor_observation::pair((
                    std::ptr::from_ref(checker).addr(),
                    std::ptr::from_ref(checker.constraints).addr(),
                    (
                        std::ptr::from_ref(checker.relation_visitor).addr(),
                        std::ptr::from_ref(checker.disjointness_visitor).addr(),
                        std::ptr::from_ref(checker.signature_relation_visitor).addr(),
                        std::ptr::from_ref(checker.materialization_visitor).addr(),
                    ),
                    checker.relation,
                    checker.typevar_evaluation,
                    descriptor_observation::key(checker.inferable),
                    checker.context_tree.is_some(),
                    checker.perform_expensive_checks,
                    descriptor_observation::key(source),
                    descriptor_observation::key(target),
                    crate::types::constructor::expansion_probe::stopped(db),
                ))
            }),
        })
    }

    pub(super) fn finish<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ConstraintSet<'db, 'c>, D::Error> {
        self.finish_borrowed(db, result, dependencies)
    }

    pub(super) fn finish_borrowed<D: RelationDependencies>(
        &self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ConstraintSet<'db, 'c>, D::Error> {
        let checker = self.checker;
        let mut pending = [None, None];
        if let Some(observations) = checker.observations {
            let outcome = if result.is_trivially_never_satisfied() {
                RelationOutcome::Never
            } else if result.is_trivially_always_satisfied() {
                RelationOutcome::Always
            } else {
                RelationOutcome::Conditional
            };
            for (slot, (pattern, actual, is_target)) in pending.iter_mut().zip([
                (self.source, self.target, false),
                (self.target, self.source, true),
            ]) {
                if !observations.patterns.contains(&pattern) {
                    continue;
                }
                let inferred = dependencies.run(db, || {
                    if matches!(
                        observations.site.get(),
                        RelationObservationSite::Argument(_)
                    ) && let Type::TypeVar(typevar) = pattern
                        && typevar.is_inferable(db, checker.inferable)
                    {
                        Some((typevar.identity(db), actual))
                    } else {
                        None
                    }
                })?;
                let actual_arity =
                    dependencies.run(db, || TypeArity::of(db, checker.env, actual))?;
                let pattern_arity =
                    dependencies.run(db, || TypeArity::of(db, checker.env, pattern))?;
                let actual_size =
                    dependencies.run(db, || TypeStructureSize::of(db, checker.env, actual))?;
                let pattern_size =
                    dependencies.run(db, || TypeStructureSize::of(db, checker.env, pattern))?;
                *slot = Some(PendingObservation {
                    inferred,
                    observation: RelationObservation {
                        site: observations.site.get(),
                        pattern,
                        is_target,
                        outcome,
                        size: actual_size.min(pattern_size),
                        arity: actual_arity.min(pattern_arity),
                    },
                });
            }
        }

        // Structural measurements can request dependencies. Finish them before borrowing or
        // mutating observation storage, so an interrupted pair publishes neither result nor
        // inferred arguments. There are at most two records, so staging them needs no allocation.
        dependencies.run(db, || {
            if let Some(observations) = checker.observations {
                let mut results = observations.results.borrow_mut();
                let mut inferred = observations.inferred.borrow_mut();
                for pending in pending.into_iter().flatten() {
                    if let Some((typevar, actual)) = pending.inferred {
                        inferred.entry(typevar).or_default().insert(actual);
                    }
                    results.insert(pending.observation);
                }
            }
            #[cfg(test)]
            if descriptor_observation::within_descriptor() {
                descriptor_observation::event(
                    "PairResult",
                    (
                        result.is_trivially_always_satisfied(),
                        result.is_trivially_never_satisfied(),
                        crate::types::constructor::expansion_probe::stopped(db),
                    ),
                );
            }
            result
        })
    }
}

struct PendingObservation<'db> {
    inferred: Option<(BoundTypeVarIdentity<'db>, Type<'db>)>,
    observation: RelationObservation<'db>,
}
