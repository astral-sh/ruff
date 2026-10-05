use crate::ProgramEnvironment;
use crate::types::ApplyTypeMappingVisitor;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::relation::{
    DisjointnessChecker, EquivalenceChecker, HasRelationToVisitor, IsDisjointVisitor, TypeRelation,
    TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;

pub(in crate::types) struct RelationOwners<'env, 'c, 'db> {
    env: &'env ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    relation: HasRelationToVisitor<'db, 'c>,
    disjointness: IsDisjointVisitor<'db, 'c>,
    signatures: SignatureRelationVisitor<'db>,
    mapping: ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'env, 'c, 'db> RelationOwners<'env, 'c, 'db> {
    pub(in crate::types) fn new(
        env: &'env ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            relation: HasRelationToVisitor::default(constraints),
            disjointness: IsDisjointVisitor::default(constraints),
            signatures: SignatureRelationVisitor::default(),
            mapping: ApplyTypeMappingVisitor::new(env),
        }
    }

    pub(in crate::types) fn subtyping(
        &self,
        inferable: TypeVarSet<'db>,
    ) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker::subtyping(
            self.env,
            self.constraints,
            inferable,
            &self.relation,
            &self.disjointness,
            &self.signatures,
            &self.mapping,
        )
    }

    pub(in crate::types) fn assignability(
        &self,
        inferable: TypeVarSet<'db>,
    ) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker::new(
            self.env,
            TypeRelation::Assignability,
            self.constraints,
            inferable,
            &self.relation,
            &self.disjointness,
            &self.signatures,
            &self.mapping,
        )
    }

    pub(in crate::types) fn redundancy(&self) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker::new(
            self.env,
            TypeRelation::Redundancy { pure: false },
            self.constraints,
            TypeVarSet::None,
            &self.relation,
            &self.disjointness,
            &self.signatures,
            &self.mapping,
        )
    }

    pub(in crate::types) fn disjointness(
        &self,
        inferable: TypeVarSet<'db>,
    ) -> DisjointnessChecker<'_, 'c, 'db> {
        DisjointnessChecker::new(
            self.env,
            self.constraints,
            inferable,
            &self.relation,
            &self.disjointness,
            &self.signatures,
            &self.mapping,
        )
    }

    pub(in crate::types) fn constraint_set_assignability(
        &self,
    ) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker::constraint_set_assignability(
            self.env,
            self.constraints,
            &self.relation,
            &self.disjointness,
            &self.signatures,
            &self.mapping,
        )
    }

    pub(in crate::types) fn equivalence(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        self.equivalence_with_typevar_evaluation(TypeVarEvaluation::Eager)
    }

    pub(in crate::types) fn constraint_set_equivalence(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        self.equivalence_with_typevar_evaluation(TypeVarEvaluation::Lazy)
    }

    fn equivalence_with_typevar_evaluation(
        &self,
        typevar_evaluation: TypeVarEvaluation,
    ) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            observations: None,
            env: self.env,
            constraints: self.constraints,
            given: ConstraintSet::from_bool(self.constraints, false),
            perform_expensive_checks: true,
            typevar_evaluation,
            relation_visitor: &self.relation,
            disjointness_visitor: &self.disjointness,
            signature_relation_visitor: &self.signatures,
            materialization_visitor: &self.mapping,
        }
    }
}
