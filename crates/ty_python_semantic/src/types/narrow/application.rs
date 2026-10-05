//! Ordered application of narrowing constraints with explicit semantic and storage effects.

use std::convert::Infallible;
use std::iter::Chain;

use smallvec::IntoIter;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{
    Conjunctions, NarrowingConstraint, NarrowingConstraintKind, NarrowingOperation,
    filter_generic_narrowing_constraint,
};
use crate::types::{IntersectionType, Type, UnionBuilder};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
pub(in crate::types) mod tests;

pub(in crate::types) struct ApplicationFacts;

pub(super) struct OrdinaryApplicationEffects<'db> {
    pub(super) db: &'db dyn Db,
}

pub(in crate::types) enum ConstraintApplication<'db> {
    Empty,
    Atomic(Type<'db>),
    Combined(Disjuncts<'db>),
}

impl<'db> ConstraintApplication<'db> {
    pub(in crate::types) fn new(constraint: NarrowingConstraint<'db>) -> Self {
        match constraint.0 {
            NarrowingConstraintKind::Empty => Self::Empty,
            NarrowingConstraintKind::Intersection(operation)
            | NarrowingConstraintKind::Replacement(operation) => Self::Atomic(operation.ty()),
            NarrowingConstraintKind::Combined(combined) => Self::Combined(Disjuncts {
                #[cfg(feature = "experimental-analysis")]
                backing: (
                    if combined.replacement_disjuncts.spilled() {
                        combined.replacement_disjuncts.capacity()
                    } else {
                        0
                    },
                    if combined.intersection_disjuncts.spilled() {
                        combined.intersection_disjuncts.capacity()
                    } else {
                        0
                    },
                ),
                remaining: combined
                    .replacement_disjuncts
                    .into_iter()
                    .chain(combined.intersection_disjuncts),
            }),
        }
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) const fn start_work() -> usize {
        size_of::<Self>() * 2
            + size_of::<NarrowingConstraint<'db>>() * 2
            + size_of::<super::CombinedNarrowingConstraint<'db>>() * 2
            + 1
    }
}

/// Retain the replacement-first iterator order and the original backing sizes during suspension.
pub(in crate::types) struct Disjuncts<'db> {
    remaining: Chain<IntoIter<[Conjunctions<'db>; 1]>, IntoIter<[Conjunctions<'db>; 1]>>,
    #[cfg(feature = "experimental-analysis")]
    backing: (usize, usize),
}

impl<'db> Disjuncts<'db> {
    pub(in crate::types) fn next(&mut self) -> Option<Conjunctions<'db>> {
        self.remaining.next()
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn retirement_work(&self) -> Option<usize> {
        self.backing
            .0
            .checked_add(self.backing.1)?
            .checked_mul(size_of::<Conjunctions<'db>>())?
            .checked_add(size_of::<Self>())?
            .checked_add(1)
    }
}

pub(in crate::types) enum ConjunctionApplication<'db> {
    Singleton(Type<'db>),
    Fold(Conjuncts<'db>),
}

impl<'db> ConjunctionApplication<'db> {
    pub(in crate::types) fn new(conjunction: Conjunctions<'db>) -> Self {
        if let [operation] = conjunction.conjuncts.as_slice() {
            Self::Singleton(operation.ty())
        } else {
            Self::Fold(Conjuncts {
                #[cfg(feature = "experimental-analysis")]
                backing: if conjunction.conjuncts.spilled() {
                    conjunction.conjuncts.capacity()
                } else {
                    0
                },
                remaining: conjunction.conjuncts.into_iter(),
            })
        }
    }
}

pub(in crate::types) struct Conjuncts<'db> {
    remaining: IntoIter<[NarrowingOperation<'db>; 2]>,
    #[cfg(feature = "experimental-analysis")]
    backing: usize,
}

impl<'db> Conjuncts<'db> {
    pub(in crate::types) fn next(&mut self) -> Option<NarrowingOperation<'db>> {
        self.remaining.next()
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn retirement_work(&self) -> Option<usize> {
        self.backing
            .checked_add(self.remaining.len())?
            .checked_mul(size_of::<NarrowingOperation<'db>>())?
            .checked_add(size_of::<Self>())?
            .checked_add(1)
    }
}

impl<'db> Conjunctions<'db> {
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn take(&mut self) -> Self {
        Self {
            conjuncts: std::mem::take(&mut self.conjuncts),
        }
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn application_start_work(&self) -> Option<usize> {
        // A singleton may still own spilled backing. Starting its application drops that backing
        // immediately, while a longer conjunction retains it in the operation iterator.
        let backing = if self.conjuncts.spilled() {
            self.conjuncts.capacity()
        } else {
            0
        };
        backing
            .checked_add(self.conjuncts.len())?
            .checked_mul(size_of::<NarrowingOperation<'db>>())?
            .checked_add(size_of::<Self>() * 2)?
            .checked_add(size_of::<ConjunctionApplication<'db>>() * 2)?
            .checked_add(1)
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousApplicationEffects)]
    pub(in crate::types) trait ApplicationEffects<'db> {
        type Error;

        // Inputs come from producers that prepay their eventual disposal. An admitted transition
        // moves ownership only after acceptance; interruption retires the remaining flat payloads.
        #[operation(local)]
        async fn start(&self, constraint: NarrowingConstraint<'db>) -> Result<ConstraintApplication<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_disjunct(&self, disjuncts: &mut Disjuncts<'db>) -> Result<Option<Conjunctions<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_disjuncts(&self, disjuncts: Disjuncts<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn conjunction(&self, env: &ProgramEnvironment<'db>, conjunction: Conjunctions<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn start_conjunction(&self, conjunction: Conjunctions<'db>) -> Result<ConjunctionApplication<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_operation(&self, conjuncts: &mut Conjuncts<'db>) -> Result<Option<NarrowingOperation<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_conjuncts(&self, conjuncts: Conjuncts<'db>) -> Result<(), Self::Error>;

        #[operation(local)]
        async fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn intersection(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn generic_filtering(&self, env: &ProgramEnvironment<'db>, subject: Type<'db>, target: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ApplicationFacts {
        fn object<'db>(&self) -> Type<'db> {
            Type::object()
        }
    }

    #[synchronous(evaluate_sync)]
    #[capabilities(effects = ApplicationEffects)]
    #[passive_values(Type::Never)]
    pub(in crate::types) async fn evaluate_with<'db, E: ApplicationEffects<'db>>(
        constraint: NarrowingConstraint<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match effects.start(constraint).await? {
            ConstraintApplication::Empty => Ok(Type::Never),
            ConstraintApplication::Atomic(ty) => {
                let mut union = effects.new_union(env).await?;
                effects.union_add(&mut union, ty).await?;
                effects.union_build(union).await
            }
            ConstraintApplication::Combined(mut disjuncts) => {
                let mut union = effects.new_union(env).await?;
                #[cursor_loop]
                while let Some(conjunction) = effects.next_disjunct(&mut disjuncts).await? {
                    let ty = effects.conjunction(env, conjunction).await?;
                    effects.union_add(&mut union, ty).await?;
                }
                effects.finish_disjuncts(disjuncts).await?;
                effects.union_build(union).await
            }
        }
    }

    #[synchronous(conjunction_sync)]
    #[capabilities(effects = ApplicationEffects, facts = ApplicationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn conjunction_with<'db, E: ApplicationEffects<'db>>(
        conjunction: Conjunctions<'db>,
        env: &ProgramEnvironment<'db>,
        facts: ApplicationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut conjuncts = match effects.start_conjunction(conjunction).await? {
            ConjunctionApplication::Singleton(ty) => return Ok(ty),
            ConjunctionApplication::Fold(conjuncts) => conjuncts,
        };
        // Collapse shared union arms before distributing the next constraint over them.
        #[passive_state]
        let mut accumulated = facts.object();
        #[cursor_loop]
        while let Some(conjunct) = effects.next_operation(&mut conjuncts).await? {
            accumulated = match conjunct {
                NarrowingOperation::Intersection(ty) => effects.intersection(env, accumulated, ty).await?,
                NarrowingOperation::GenericFiltering(ty) => effects.generic_filtering(env, accumulated, ty).await?,
            };
        }
        effects.finish_conjuncts(conjuncts).await?;
        Ok(accumulated)
    }
}

impl<'db> SynchronousApplicationEffects<'db> for OrdinaryApplicationEffects<'db> {
    type Error = Infallible;

    fn start(
        &self,
        constraint: NarrowingConstraint<'db>,
    ) -> Result<ConstraintApplication<'db>, Self::Error> {
        Ok(ConstraintApplication::new(constraint))
    }

    fn next_disjunct(
        &self,
        disjuncts: &mut Disjuncts<'db>,
    ) -> Result<Option<Conjunctions<'db>>, Self::Error> {
        Ok(disjuncts.next())
    }

    fn finish_disjuncts(&self, disjuncts: Disjuncts<'db>) -> Result<(), Self::Error> {
        drop(disjuncts);
        Ok(())
    }

    fn conjunction(
        &self,
        env: &ProgramEnvironment<'db>,
        conjunction: Conjunctions<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(conjunction.evaluate_constraint_type(self.db, env))
    }

    fn start_conjunction(
        &self,
        conjunction: Conjunctions<'db>,
    ) -> Result<ConjunctionApplication<'db>, Self::Error> {
        Ok(ConjunctionApplication::new(conjunction))
    }

    fn next_operation(
        &self,
        conjuncts: &mut Conjuncts<'db>,
    ) -> Result<Option<NarrowingOperation<'db>>, Self::Error> {
        Ok(conjuncts.next())
    }

    fn finish_conjuncts(&self, conjuncts: Conjuncts<'db>) -> Result<(), Self::Error> {
        drop(conjuncts);
        Ok(())
    }

    fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error> {
        Ok(UnionBuilder::new(self.db, env))
    }

    fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error> {
        union.add_in_place(ty);
        Ok(())
    }

    fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(union.build())
    }

    fn intersection(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(IntersectionType::from_two_elements(
            self.db, env, left, right,
        ))
    }

    fn generic_filtering(
        &self,
        env: &ProgramEnvironment<'db>,
        subject: Type<'db>,
        target: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(filter_generic_narrowing_constraint(
            self.db, env, subject, target,
        ))
    }
}
