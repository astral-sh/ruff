//! Expression occurrences carried by inference bounds independently of their interned values.

use crate::types::Type;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::projection::{BoundSourceRecipe, ObservedType};
use crate::types::relation::{RelationContext, TypeVarEvaluation};
use crate::types::typevar::TypeVarSet;
use crate::{Db, ProgramEnvironment};

/// A bound can be supplied by several expressions even when their closed types are equal.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct BoundSources<'db> {
    contributors: Box<[BoundSourceRecipe<'db>]>,
}

impl<'db> BoundSources<'db> {
    pub(super) fn root_constraint(constraint: super::variables::Constraint<'db>) -> Self {
        let ty = match constraint {
            super::variables::Constraint::ConcreteLower(bound) => bound.bound,
            super::variables::Constraint::ConcreteUpper(bound) => bound.bound,
            super::variables::Constraint::ConcreteEquivalence(bound) => bound.bound,
            super::variables::Constraint::TypeVarRange(bound) => Type::TypeVar(bound.right),
            super::variables::Constraint::TypeVarEquivalence(bound) => Type::TypeVar(bound.right),
        };
        Self::from_observed(&ObservedType::root(ty))
    }
    pub(super) fn from_observed(observed: &ObservedType<'db>) -> Self {
        Self {
            contributors: BoundSourceRecipe::from_observed(observed),
        }
    }

    pub(super) fn merge(&mut self, other: &Self) {
        let mut contributors = self.contributors.to_vec();
        for contributor in &other.contributors {
            if !contributors.contains(contributor) {
                contributors.push(contributor.clone());
            }
        }
        self.contributors = contributors.into_boxed_slice();
    }

    /// Apply an unrecorded bound operation conservatively to its explicit input expressions.
    /// This never searches unrelated bounds for a matching resulting type.
    pub(super) fn observe(&self, ty: Type<'db>) -> ObservedType<'db> {
        if let [contributor] = self.contributors.as_ref() {
            return contributor.observe(ty);
        }
        ObservedType::dependent_on(
            ty,
            &self
                .contributors
                .iter()
                .map(|recipe| recipe.observe(ty))
                .collect::<Vec<_>>(),
        )
    }

    pub(super) fn relations<'a>(
        &'a self,
        context: &'a RelationContext<'db>,
    ) -> DerivedBoundRelations<'a, 'db> {
        DerivedBoundRelations {
            sources: self,
            context,
        }
    }
}

/// Queries on transformations of a specified collection of premise bounds.
/// When an operation has no exact recipe, both outputs retain those explicit dependencies.
#[derive(Clone, Copy)]
pub(super) struct DerivedBoundRelations<'a, 'db> {
    sources: &'a BoundSources<'db>,
    context: &'a RelationContext<'db>,
}

impl<'db> DerivedBoundRelations<'_, 'db> {
    pub(super) fn when_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
        evaluation: TypeVarEvaluation,
    ) -> OwnedConstraintSet<'db> {
        self.context.when_assignable(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
            inferable,
            evaluation,
        )
    }

    pub(super) fn when_equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> OwnedConstraintSet<'db> {
        self.context.when_equivalent(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_subtype(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_subtype(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_assignable(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_equivalent(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_equivalent_eager(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_equivalent_eager(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_redundant(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_redundant(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }

    pub(super) fn is_disjoint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        self.context.is_disjoint(
            db,
            env,
            self.sources.observe(source),
            self.sources.observe(target),
        )
    }
}
