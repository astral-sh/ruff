//! Intersection finalization shared by ordinary and controlled inference.

use std::convert::Infallible;

use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

use super::InnerIntersectionBuilder;
use crate::types::enums::EnumComplement;
use crate::types::set_theoretic::expand_intersection_typevars_and_newtypes;
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    BoundTypeVarInstance, IntersectionType, NegativeIntersectionElements, Type,
    TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct FinalizationFacts;

pub(in crate::types) struct RemainingConstraints<'db> {
    original: &'db [Type<'db>],
    remaining: Vec<Option<Type<'db>>>,
    #[cfg(test)]
    lifetime: Option<constraints_observations::OwnerLifetime>,
}

impl<'db> RemainingConstraints<'db> {
    pub(in crate::types) fn new(original: &'db [Type<'db>]) -> Self {
        Self {
            original,
            remaining: original.iter().copied().map(Some).collect(),
            #[cfg(test)]
            lifetime: None,
        }
    }

    #[cfg(test)]
    pub(in crate::types) fn record(&mut self, db: &dyn Db) {
        self.lifetime = Some(constraints_observations::owner_ready(db));
    }

    pub(in crate::types) fn storage(&self) -> (usize, usize) {
        (self.remaining.len(), self.remaining.capacity())
    }

    pub(in crate::types) fn next_original(&self, cursor: &mut usize) -> Option<(usize, Type<'db>)> {
        let next = self.original.get(*cursor).copied().map(|ty| (*cursor, ty));
        if next.is_some() {
            *cursor += 1;
        }
        next
    }

    pub(in crate::types) fn exclude(&mut self, index: usize) -> bool {
        let Some(slot) = self.remaining.get_mut(index) else {
            return false;
        };
        *slot = None;
        true
    }

    pub(in crate::types) fn next_remaining(&self, cursor: &mut usize) -> Option<Option<Type<'db>>> {
        let next = self.remaining.get(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        next
    }
}

#[cfg(test)]
macro_rules! owner_observations {
    ($name:ident) => {
        pub(in crate::types) mod $name {
            use std::cell::Cell;

            use crate::Db;

            thread_local! {
                static LIVE: Cell<usize> = const { Cell::new(0) };
                static ENTERED: Cell<usize> = const { Cell::new(0) };
                static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
                static CANCEL: Cell<bool> = const { Cell::new(false) };
            }

            pub(in crate::types) fn reset(cancel: bool) {
                assert_eq!(LIVE.get(), 0);
                ENTERED.set(0);
                REMAINING.set(None);
                CANCEL.set(cancel);
            }

            pub(in crate::types) fn progress() -> (usize, usize, Option<usize>) {
                (LIVE.get(), ENTERED.get(), REMAINING.get())
            }

            pub(in crate::types) struct OwnerLifetime;

            impl Drop for OwnerLifetime {
                fn drop(&mut self) {
                    LIVE.set(LIVE.get() - 1);
                }
            }

            pub(in crate::types) fn owner_ready(db: &dyn Db) -> OwnerLifetime {
                LIVE.set(LIVE.get() + 1);
                ENTERED.set(ENTERED.get() + 1);
                REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(db));
                if CANCEL.replace(false) {
                    db.cancellation_token().cancel();
                }
                OwnerLifetime
            }
        }
    };
}

#[cfg(test)]
owner_observations!(finalization_observations);
#[cfg(test)]
owner_observations!(constraints_observations);

struct OrdinaryFinalizationEffects<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
}

pub(super) fn build<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    builder: InnerIntersectionBuilder<'db>,
) -> Type<'db> {
    match build_sync(
        builder,
        FinalizationFacts,
        &OrdinaryFinalizationEffects { db, env },
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousFinalizationEffects)]
    // A controlled implementation admits storage and cleanup before allocating, growing, or
    // transferring owned payloads. Owned arguments stay outside rejectable admission callbacks
    // until acceptance; semantic descendants without a controlled implementation must refuse.
    pub(in crate::types) trait FinalizationEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_empty_enum_complement(&self, builder: &InnerIntersectionBuilder<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn simplify_constrained_typevars(&self, builder: &mut InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_additions(&self) -> Result<SmallVec<[Type<'db>; 1]>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_positive(&self, builder: &InnerIntersectionBuilder<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn bound_or_constraints(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn constraint_elements(&self, constraints: TypeVarConstraints<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn remaining_constraints(&self, original: &'db [Type<'db>]) -> Result<RemainingConstraints<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_negative(&self, builder: &InnerIntersectionBuilder<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_constraint(&self, constraints: &RemainingConstraints<'db>, cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(source)]
        async fn is_subtype(&self, constraint: Type<'db>, negative: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn exclude_constraint(&self, constraints: &mut RemainingConstraints<'db>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_remaining(&self, constraints: &RemainingConstraints<'db>, cursor: &mut usize) -> Result<Option<Option<Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn finish_constraints(&self, constraints: RemainingConstraints<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn set_never(&self, builder: &mut InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn queue_addition(&self, additions: &mut SmallVec<[Type<'db>; 1]>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_addition(&self, additions: &SmallVec<[Type<'db>; 1]>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_additions(&self, additions: SmallVec<[Type<'db>; 1]>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn add_positive(&self, builder: &mut InnerIntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn expand_typevars_and_newtypes(&self, builder: &InnerIntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn is_singleton(&self, complement: EnumComplement<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn remaining_literal_union(&self, complement: EnumComplement<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn enum_complement(&self, builder: &InnerIntersectionBuilder<'db>) -> Result<Option<EnumComplement<'db>>, Self::Error>;
        #[operation(local)]
        async fn lengths(&self, builder: &InnerIntersectionBuilder<'db>) -> Result<(usize, usize), Self::Error>;
        #[operation(local)]
        async fn first_positive(&self, builder: &InnerIntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn shrink(&self, builder: &mut InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn intern(&self, builder: InnerIntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn finish(&self, builder: InnerIntersectionBuilder<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl FinalizationFacts {
        fn object<'db>(&self) -> Type<'db> {
            Type::object()
        }

        fn is_never(&self, ty: Type<'_>) -> bool {
            ty.is_never()
        }
    }

    /// Tries to simplify any constrained typevars in the intersection.
    ///
    /// We must preserve the constrained `TypeVar` itself in the result, even if only a single
    /// compatible constraint remains, because other occurrences of the same `TypeVar` still need
    /// to correlate with it (for example, when returning a narrowed value as `T`).
    ///
    /// - If the intersection contains negative entries for all but one of the constraints, we can
    ///   add that remaining constraint as a positive entry.
    ///
    /// - If the intersection contains negative entries for all of the constraints, the overall
    ///   intersection is `Never`.
    #[synchronous(simplify_constrained_typevars_sync)]
    #[capabilities(effects = FinalizationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn simplify_constrained_typevars_with<'db, E: FinalizationEffects<'db>>(
        builder: &mut InnerIntersectionBuilder<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let mut to_add = effects.new_additions().await?;
        let mut positive_cursor = 0;
        #[cursor_loop]
        while let Some(ty) = effects.next_positive(builder, &mut positive_cursor).await? {
            let Type::TypeVar(bound_typevar) = ty else {
                continue;
            };
            let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                effects.bound_or_constraints(bound_typevar).await?
            else {
                continue;
            };

            // Determine which constraints appear as negative entries in the intersection.
            let original = effects.constraint_elements(constraints).await?;
            let mut constraints = effects.remaining_constraints(original).await?;
            let mut negative_cursor = 0;
            #[cursor_loop]
            while let Some(negative) = effects.next_negative(builder, &mut negative_cursor).await? {
                // This linear search should be fine as long as we don't encounter typevars with
                // thousands of constraints.
                let mut constraint_cursor = 0;
                #[cursor_loop]
                while let Some(indexed_constraint) = effects.next_constraint(&constraints, &mut constraint_cursor).await? {
                    let (index, constraint) = indexed_constraint;
                    if effects.is_subtype(constraint, negative).await? {
                        effects.exclude_constraint(&mut constraints, index).await?;
                    }
                }
            }

            #[passive_state]
            let mut remaining_constraint = None;
            #[passive_state]
            let mut more_than_one_remaining_constraint = false;
            let mut remaining_cursor = 0;
            #[cursor_loop]
            while let Some(slot) = effects.next_remaining(&constraints, &mut remaining_cursor).await? {
                if let Some(constraint) = slot {
                    if matches!(remaining_constraint, Some(_)) {
                        more_than_one_remaining_constraint = true;
                        break;
                    }
                    remaining_constraint = Some(constraint);
                }
            }
            let Some(remaining_constraint) = remaining_constraint else {
                // All of the typevar constraints have been removed, so the entire intersection is
                // `Never`.
                effects.set_never(builder).await?;
                effects.finish_constraints(constraints).await?;
                effects.finish_additions(to_add).await?;
                return Ok(());
            };

            if more_than_one_remaining_constraint {
                // This typevar cannot be simplified.
                effects.finish_constraints(constraints).await?;
                continue;
            }

            // Only one typevar constraint remains. Adding it as a positive element lets the normal
            // intersection simplification remove any incompatible negatives, while keeping the
            // original typevar in the result.
            effects.queue_addition(&mut to_add, remaining_constraint).await?;
            effects.finish_constraints(constraints).await?;
        }

        let mut addition_cursor = 0;
        #[cursor_loop]
        while let Some(remaining_constraint) = effects.next_addition(&to_add, &mut addition_cursor).await? {
            effects.add_positive(builder, remaining_constraint).await?;
        }
        effects.finish_additions(to_add).await?;
        Ok(())
    }

    #[synchronous(build_sync)]
    #[capabilities(effects = FinalizationEffects, facts = FinalizationFacts)]
    #[passive_values(Type::Never, Type::EnumComplement)]
    pub(in crate::types) async fn build_with<'db, E: FinalizationEffects<'db>>(
        mut builder: InnerIntersectionBuilder<'db>,
        facts: FinalizationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if effects.has_empty_enum_complement(&builder).await? {
            effects.finish(builder).await?;
            return Ok(Type::Never);
        }

        effects.simplify_constrained_typevars(&mut builder).await?;

        // If any typevars are in `builder.positive`, speculatively solve all bounded type variables
        // to their upper bound and all constrained type variables to the union of their constraints.
        // If that speculative intersection simplifies to `Never`, this intersection must also simplify
        // to `Never`.
        #[passive_state]
        let mut should_expand = false;
        let mut positive_cursor = 0;
        #[cursor_loop]
        while let Some(ty) = effects.next_positive(&builder, &mut positive_cursor).await? {
            if matches!(ty, Type::TypeVar(_) | Type::NewTypeInstance(_)) {
                should_expand = true;
                break;
            }
        }
        if should_expand {
            let speculative = effects.expand_typevars_and_newtypes(&builder).await?;
            if facts.is_never(speculative) {
                effects.finish(builder).await?;
                return Ok(Type::Never);
            }

            if let Type::EnumComplement(complement) = speculative
                && effects.is_singleton(complement).await?
            {
                #[passive_state]
                let mut has_newtype = false;
                let mut positive_cursor = 0;
                #[cursor_loop]
                while let Some(positive) = effects.next_positive(&builder, &mut positive_cursor).await? {
                    if matches!(positive, Type::NewTypeInstance(_)) {
                        has_newtype = true;
                        break;
                    }
                }
                if has_newtype {
                    // Preserve the NewType while making its remaining enum member explicit.
                    let remaining = effects.remaining_literal_union(complement).await?;
                    effects.add_positive(&mut builder, remaining).await?;
                }
            }
        }

        if let Some(complement) = effects.enum_complement(&builder).await? {
            effects.finish(builder).await?;
            return Ok(Type::EnumComplement(complement));
        }

        match effects.lengths(&builder).await? {
            (0, 0) => {
                let result = facts.object();
                effects.finish(builder).await?;
                Ok(result)
            }
            (1, 0) => {
                let result = effects.first_positive(&builder).await?;
                effects.finish(builder).await?;
                Ok(result)
            }
            _ => {
                effects.shrink(&mut builder).await?;
                effects.intern(builder).await
            }
        }
    }
}

impl<'db> SynchronousFinalizationEffects<'db> for OrdinaryFinalizationEffects<'_, 'db> {
    type Error = Infallible;

    fn has_empty_enum_complement(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(builder.has_empty_enum_complement(self.db, self.env))
    }

    fn simplify_constrained_typevars(
        &self,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> Result<(), Self::Error> {
        simplify_constrained_typevars_sync(builder, self)
    }

    fn new_additions(&self) -> Result<SmallVec<[Type<'db>; 1]>, Self::Error> {
        Ok(SmallVec::new())
    }

    fn next_positive(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = builder.positive.get_index(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(typevar
            .typevar(self.db)
            .bound_or_constraints(self.db, self.env))
    }

    fn constraint_elements(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(constraints.elements(self.db))
    }

    fn remaining_constraints(
        &self,
        original: &'db [Type<'db>],
    ) -> Result<RemainingConstraints<'db>, Self::Error> {
        Ok(RemainingConstraints::new(original))
    }

    fn next_negative(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = match &builder.negative {
            NegativeIntersectionElements::Empty => None,
            NegativeIntersectionElements::Single(ty) => (*cursor == 0).then_some(*ty),
            NegativeIntersectionElements::Multiple(elements) => {
                elements.get_index(*cursor).copied()
            }
        };
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn next_constraint(
        &self,
        constraints: &RemainingConstraints<'db>,
        cursor: &mut usize,
    ) -> Result<Option<(usize, Type<'db>)>, Self::Error> {
        Ok(constraints.next_original(cursor))
    }

    fn is_subtype(&self, constraint: Type<'db>, negative: Type<'db>) -> Result<bool, Self::Error> {
        Ok(constraint.is_subtype_of(self.db, self.env, negative))
    }

    fn exclude_constraint(
        &self,
        constraints: &mut RemainingConstraints<'db>,
        index: usize,
    ) -> Result<(), Self::Error> {
        constraints.exclude(index);
        Ok(())
    }

    fn next_remaining(
        &self,
        constraints: &RemainingConstraints<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Option<Type<'db>>>, Self::Error> {
        Ok(constraints.next_remaining(cursor))
    }

    fn finish_constraints(
        &self,
        constraints: RemainingConstraints<'db>,
    ) -> Result<(), Self::Error> {
        drop(constraints);
        Ok(())
    }

    fn set_never(&self, builder: &mut InnerIntersectionBuilder<'db>) -> Result<(), Self::Error> {
        builder.reset_to(Type::Never);
        Ok(())
    }

    fn queue_addition(
        &self,
        additions: &mut SmallVec<[Type<'db>; 1]>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        additions.push(ty);
        Ok(())
    }

    fn next_addition(
        &self,
        additions: &SmallVec<[Type<'db>; 1]>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = additions.get(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn finish_additions(&self, additions: SmallVec<[Type<'db>; 1]>) -> Result<(), Self::Error> {
        drop(additions);
        Ok(())
    }

    fn add_positive(
        &self,
        builder: &mut InnerIntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.add_positive(self.db, self.env, ty);
        Ok(())
    }

    fn expand_typevars_and_newtypes(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(expand_intersection_typevars_and_newtypes(
            self.db,
            self.env,
            &builder.positive,
            &builder.negative,
        ))
    }

    fn is_singleton(&self, complement: EnumComplement<'db>) -> Result<bool, Self::Error> {
        Ok(complement.is_singleton(self.db))
    }

    fn remaining_literal_union(
        &self,
        complement: EnumComplement<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(complement.remaining_literal_union(self.db, self.env))
    }

    fn enum_complement(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> Result<Option<EnumComplement<'db>>, Self::Error> {
        Ok(EnumComplement::from_intersection_parts(
            self.db,
            self.env,
            &builder.positive,
            &builder.negative,
        ))
    }

    fn lengths(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> Result<(usize, usize), Self::Error> {
        Ok((builder.positive.len(), builder.negative.len()))
    }

    fn first_positive(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.positive[0])
    }

    fn shrink(&self, builder: &mut InnerIntersectionBuilder<'db>) -> Result<(), Self::Error> {
        builder.shrink_signed();
        Ok(())
    }

    fn intern(&self, builder: InnerIntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Intersection(IntersectionType::new(
            self.db,
            builder.positive,
            builder.negative,
        )))
    }

    fn finish(&self, builder: InnerIntersectionBuilder<'db>) -> Result<(), Self::Error> {
        drop(builder);
        Ok(())
    }
}
