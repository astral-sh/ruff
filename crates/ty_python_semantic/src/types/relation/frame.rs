//! Proof continuations independent of constraint storage.

use std::rc::Rc;

use super::{RelationSession, TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::ApplyTypeMappingVisitor;
use crate::types::constraints::{ConstraintProvenance, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::projection::{ObservedType, ObservedTypePair};
use crate::types::typevar::TypeVarSet;
use crate::{Db, ProgramEnvironment};

/// The ambient rules of a proof, without an implicit pair of operands.
/// Every query supplies its own observed expressions, including queries on stored bounds.
#[derive(Clone, Debug)]
pub(in crate::types) struct RelationContext<'db> {
    session: Rc<RelationSession<'db>>,
    provenance: ConstraintProvenance,
    negative: bool,
    perform_expensive_checks: bool,
}

impl Default for RelationContext<'_> {
    fn default() -> Self {
        Self::new(Rc::default())
    }
}

impl<'db> RelationContext<'db> {
    pub(in crate::types) fn new(session: Rc<RelationSession<'db>>) -> Self {
        Self {
            negative: session.is_negative(),
            session,
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
        }
    }

    fn always_satisfied(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        when: &OwnedConstraintSet<'db>,
    ) -> bool {
        self.session.with_polarity(self.negative, || {
            let constraints = ConstraintSetBuilder::with_relation_context(self.clone());
            constraints
                .load(db, env, when)
                .is_always_satisfied(db, env, TypeVarSet::None)
        })
    }

    pub(in crate::types) fn session(&self) -> &Rc<RelationSession<'db>> {
        &self.session
    }

    pub(in crate::types) fn provenance(&self) -> ConstraintProvenance {
        self.provenance
    }

    pub(in crate::types) fn with_provenance(&self, provenance: ConstraintProvenance) -> Self {
        Self {
            provenance,
            ..self.clone()
        }
    }

    pub(in crate::types) fn perform_expensive_checks(&self) -> bool {
        self.perform_expensive_checks
    }

    pub(in crate::types) fn permits_closed_query_cache(&self) -> bool {
        !self.session.is_active()
            && !self.negative
            && self.provenance == ConstraintProvenance::Evidence
            && self.perform_expensive_checks
    }

    pub(in crate::types) fn observe<R>(
        &self,
        db: &'db dyn Db,
        operand: &ObservedType<'db>,
        work: impl FnOnce() -> Option<R>,
    ) -> Option<R> {
        self.observe_goal(db, operand, super::RelationGoal::Observation, work)
    }

    pub(in crate::types) fn bind_call<R>(
        &self,
        db: &'db dyn Db,
        operand: &ObservedType<'db>,
        work: impl FnOnce() -> Option<R>,
    ) -> Option<R> {
        self.observe_goal(db, operand, super::RelationGoal::CallBinding, work)
    }

    pub(in crate::types) fn upcast_callable<R>(
        &self,
        db: &'db dyn Db,
        operand: &ObservedType<'db>,
        work: impl FnOnce() -> Option<R>,
    ) -> Option<R> {
        self.observe_goal(db, operand, super::RelationGoal::CallableUpcast, work)
    }

    fn observe_goal<R>(
        &self,
        db: &'db dyn Db,
        operand: &ObservedType<'db>,
        goal: super::RelationGoal,
        work: impl FnOnce() -> Option<R>,
    ) -> Option<R> {
        self.session.with_polarity(self.negative, || {
            let obligation = super::ObservedRelationObligation {
                obligation: super::RelationObligation {
                    source: operand.ty,
                    target: operand.ty,
                    relation: goal,
                    evaluation: TypeVarEvaluation::Eager,
                    inferable: TypeVarSet::None,
                    provenance: self.provenance,
                    perform_expensive_checks: self.perform_expensive_checks,
                    negative: self.negative,
                },
                source_origin: operand.origin(),
                target_origin: operand.origin(),
                source_dependency: operand.dependency_origins(),
                target_dependency: operand.dependency_origins(),
            };
            match self.session.visit(db, obligation, work) {
                Ok(result) => result,
                Err(_) => {
                    self.session.mark_incomplete();
                    None
                }
            }
        })
    }

    pub(in crate::types) fn try_equivalent_eager(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> Option<bool> {
        let epoch = self.session.incomplete_epoch();
        let result = self.equivalent(db, env, source, target, TypeVarEvaluation::Eager);
        if !result.query(|_, when| when.is_complete()) {
            return None;
        }
        let equivalent = self.always_satisfied(db, env, &result);
        (self.session.incomplete_epoch() == epoch).then_some(equivalent)
    }

    fn relation(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operands: ObservedTypePair<'db>,
        relation: TypeRelation,
        inferable: TypeVarSet<'db>,
        evaluation: TypeVarEvaluation,
    ) -> OwnedConstraintSet<'db> {
        self.session.with_polarity(self.negative, || {
            ConstraintSetBuilder::with_relation_context(self.clone()).into_owned(|constraints| {
                let visitor = ApplyTypeMappingVisitor::new(env);
                let mut checker = TypeRelationChecker::new(
                    env,
                    relation,
                    constraints,
                    inferable,
                    &visitor,
                    operands,
                );
                checker.typevar_evaluation = evaluation;
                checker.provenance = self.provenance;
                checker.perform_expensive_checks = self.perform_expensive_checks;
                checker.check_observed_pair(
                    db,
                    checker.operands().source.clone(),
                    checker.operands().target.clone(),
                )
            })
        })
    }

    pub(in crate::types) fn when_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
        inferable: TypeVarSet<'db>,
        evaluation: TypeVarEvaluation,
    ) -> OwnedConstraintSet<'db> {
        self.relation(
            db,
            env,
            ObservedTypePair::new(source, target),
            TypeRelation::Assignability,
            inferable,
            evaluation,
        )
    }

    pub(in crate::types) fn when_equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> OwnedConstraintSet<'db> {
        self.equivalent(db, env, source, target, TypeVarEvaluation::Lazy)
    }

    fn equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
        evaluation: TypeVarEvaluation,
    ) -> OwnedConstraintSet<'db> {
        self.session.with_polarity(self.negative, || {
            ConstraintSetBuilder::with_relation_context(self.clone()).into_owned(|constraints| {
                let visitor = ApplyTypeMappingVisitor::new(env);
                let checker = super::EquivalenceChecker {
                    env,
                    constraints,
                    provenance: self.provenance,
                    perform_expensive_checks: self.perform_expensive_checks,
                    typevar_evaluation: evaluation,
                    materialization_visitor: &visitor,
                    observations: ObservedTypePair::new(source, target),
                };
                checker.check_observed_pair(
                    db,
                    checker.observations.source.clone(),
                    checker.observations.target.clone(),
                )
            })
        })
    }

    pub(in crate::types) fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.when_assignable(
                db,
                env,
                source,
                target,
                TypeVarSet::None,
                TypeVarEvaluation::Lazy,
            ),
        )
    }

    pub(in crate::types) fn is_assignable_eager(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.when_assignable(
                db,
                env,
                source,
                target,
                TypeVarSet::None,
                TypeVarEvaluation::Eager,
            ),
        )
    }

    pub(in crate::types) fn is_subtype(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.relation(
                db,
                env,
                ObservedTypePair::new(source, target),
                TypeRelation::Subtyping,
                TypeVarSet::None,
                TypeVarEvaluation::Lazy,
            ),
        )
    }

    pub(in crate::types) fn is_subtype_eager(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.relation(
                db,
                env,
                ObservedTypePair::new(source, target),
                TypeRelation::Subtyping,
                TypeVarSet::None,
                TypeVarEvaluation::Eager,
            ),
        )
    }

    pub(in crate::types) fn is_redundant(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.relation(
                db,
                env,
                ObservedTypePair::new(source, target),
                TypeRelation::Redundancy { pure: false },
                TypeVarSet::None,
                TypeVarEvaluation::Eager,
            ),
        )
    }

    pub(in crate::types) fn is_equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.equivalent(db, env, source, target, TypeVarEvaluation::Lazy),
        )
    }

    pub(in crate::types) fn is_equivalent_eager(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.always_satisfied(
            db,
            env,
            &self.equivalent(db, env, source, target, TypeVarEvaluation::Eager),
        )
    }

    pub(in crate::types) fn is_disjoint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> bool {
        self.session.with_polarity(self.negative, || {
            let constraints = ConstraintSetBuilder::with_relation_context(self.clone());
            let visitor = ApplyTypeMappingVisitor::new(env);
            let mut checker = super::DisjointnessChecker::new(
                env,
                &constraints,
                TypeVarSet::None,
                &visitor,
                ObservedTypePair::new(source, target),
            );
            checker.provenance = self.provenance;
            checker.perform_expensive_checks = self.perform_expensive_checks;
            checker
                .check_observed_pair(
                    db,
                    checker.observations.source.clone(),
                    checker.observations.target.clone(),
                )
                .is_always_satisfied(db, env, TypeVarSet::None)
        })
    }
}

impl<'db> TypeRelationChecker<'_, '_, 'db> {
    pub(in crate::types) fn context(&self) -> RelationContext<'db> {
        RelationContext {
            session: Rc::clone(self.constraints.relation_session()),
            provenance: self.provenance,
            negative: self.constraints.relation_session().is_negative(),
            perform_expensive_checks: self.perform_expensive_checks,
        }
    }
}

impl<'db> super::DisjointnessChecker<'_, '_, 'db> {
    pub(in crate::types) fn context(&self) -> RelationContext<'db> {
        RelationContext {
            session: Rc::clone(self.constraints.relation_session()),
            provenance: self.provenance,
            negative: self.constraints.relation_session().is_negative(),
            perform_expensive_checks: self.perform_expensive_checks,
        }
    }
}
