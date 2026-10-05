//! Pairwise intersection simplification shared by ordinary and controlled inference.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::{IntersectionPolarity, IntersectionSimplification, simplify_intersection_pair_impl};
use crate::types::{LiteralValueType, LiteralValueTypeKind, Type, TypePair};
use crate::{Db, Program, ProgramEnvironment};

shared_semantic_family! {
    #[synchronous(SynchronousSimplificationComparisonEffects)]
    pub(in crate::types) trait SimplificationComparisonEffects<'db> {
        type Error;

        #[operation(local)]
        async fn kind(&self, literal: LiteralValueType<'db>) -> Result<LiteralValueTypeKind<'db>, Self::Error>;
        #[operation(local)]
        async fn same_kind(&self, first: LiteralValueType<'db>, second: LiteralValueType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_literal(&self, first: LiteralValueType<'db>, second: LiteralValueType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn is_promotable(&self, literal: LiteralValueType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn canonical(&self, first: Type<'db>, second: Type<'db>, polarity: IntersectionPolarity) -> Result<IntersectionSimplification, Self::Error>;
    }

    #[synchronous(simplify_sync)]
    #[capabilities(effects = SimplificationComparisonEffects)]
    #[passive_values(IntersectionSimplification::Unchanged, IntersectionSimplification::FirstRedundant, IntersectionSimplification::SecondRedundant, IntersectionSimplification::Disjoint)]
    pub(in crate::types) async fn simplify_with<'db, E: SimplificationComparisonEffects<'db>>(
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
        effects: &E,
    ) -> Result<IntersectionSimplification, E::Error> {
        // Built-in literal values have no inference dependencies, so these simplifications cannot
        // participate in a cycle and do not need an interned pair or a tracked relation query.
        if let (Type::LiteralValue(first), Type::LiteralValue(second)) = (first, second)
            && matches!(
                effects.kind(first).await?,
                LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::String(_)
                    | LiteralValueTypeKind::Bytes(_)
            )
            && matches!(
                effects.kind(second).await?,
                LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::String(_)
                    | LiteralValueTypeKind::Bytes(_)
            )
        {
            return Ok(match (polarity, effects.same_kind(first, second).await?) {
                (IntersectionPolarity::Positive, true) => {
                    // Redundancy depends on promotability and full literal identity, including
                    // the recursive-definition flag. Subtyping only compares the literal values.
                    if effects.same_literal(first, second).await? || effects.is_promotable(first).await? {
                        IntersectionSimplification::SecondRedundant
                    } else if effects.is_promotable(second).await? {
                        IntersectionSimplification::FirstRedundant
                    } else {
                        IntersectionSimplification::Unchanged
                    }
                }
                (IntersectionPolarity::Positive, false) | (IntersectionPolarity::Mixed, true) => {
                    IntersectionSimplification::Disjoint
                }
                (IntersectionPolarity::Negative, true) | (IntersectionPolarity::Mixed, false) => {
                    IntersectionSimplification::SecondRedundant
                }
                (IntersectionPolarity::Negative, false) => IntersectionSimplification::Unchanged,
            });
        }

        effects.canonical(first, second, polarity).await
    }

    #[synchronous(SynchronousIntersectionSimplificationEffects)]
    pub(in crate::types) trait IntersectionSimplificationEffects<'db> {
        type Error;

        #[operation(source)]
        async fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(source)]
        async fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn redundant(&self, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn subtype(&self, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn disjoint(&self, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = IntersectionSimplificationEffects)]
    #[passive_values(IntersectionSimplification::Unchanged, IntersectionSimplification::FirstRedundant, IntersectionSimplification::SecondRedundant, IntersectionSimplification::Disjoint)]
    pub(in crate::types) async fn produce_with<'db, E: IntersectionSimplificationEffects<'db>>(
        pair: TypePair<'db>,
        polarity: IntersectionPolarity,
        effects: &E,
    ) -> Result<IntersectionSimplification, E::Error> {
        let program = effects.program(pair).await?;
        let env = effects.environment(program).await?;
        let first = effects.first(pair).await?;
        let second = effects.second(pair).await?;

        match polarity {
            IntersectionPolarity::Positive => {
                // S & T = S if S <: T.
                if effects.redundant(&env, first, second).await? {
                    return Ok(IntersectionSimplification::SecondRedundant);
                }
                let first_redundant = effects.redundant(&env, second, first).await?;
                if effects.disjoint(&env, second, first).await? {
                    return Ok(IntersectionSimplification::Disjoint);
                }
                if first_redundant {
                    return Ok(IntersectionSimplification::FirstRedundant);
                }
            }
            IntersectionPolarity::Negative => {
                // ~S & ~T = ~T if S <: T; the narrower exclusion is redundant.
                let first_redundant = effects.redundant(&env, first, second).await?;
                if effects.subtype(&env, second, first).await? {
                    return Ok(IntersectionSimplification::SecondRedundant);
                }
                if first_redundant {
                    return Ok(IntersectionSimplification::FirstRedundant);
                }
            }
            IntersectionPolarity::Mixed => {
                // S & ~T = Never if S <: T, and S & ~T = S if S and T are disjoint.
                if effects.subtype(&env, first, second).await? {
                    return Ok(IntersectionSimplification::Disjoint);
                }
                if effects.disjoint(&env, first, second).await? {
                    return Ok(IntersectionSimplification::SecondRedundant);
                }
            }
        }
        Ok(IntersectionSimplification::Unchanged)
    }
}

pub(super) struct OrdinarySimplificationComparison<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousSimplificationComparisonEffects<'db>
    for OrdinarySimplificationComparison<'_, 'db>
{
    type Error = Infallible;

    fn kind(
        &self,
        literal: LiteralValueType<'db>,
    ) -> Result<LiteralValueTypeKind<'db>, Self::Error> {
        Ok(literal.kind())
    }

    fn same_kind(
        &self,
        first: LiteralValueType<'db>,
        second: LiteralValueType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first.kind() == second.kind())
    }

    fn same_literal(
        &self,
        first: LiteralValueType<'db>,
        second: LiteralValueType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first == second)
    }

    fn is_promotable(&self, literal: LiteralValueType<'db>) -> Result<bool, Self::Error> {
        Ok(literal.is_promotable())
    }

    fn canonical(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> Result<IntersectionSimplification, Self::Error> {
        Ok(simplify_intersection_pair_impl(
            self.db,
            TypePair::new(self.db, self.env.program(self.db), first, second),
            polarity,
        ))
    }
}

pub(super) struct OrdinaryIntersectionSimplification<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousIntersectionSimplificationEffects<'db>
    for OrdinaryIntersectionSimplification<'db>
{
    type Error = Infallible;

    fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(pair.program(self.db))
    }

    fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.first(self.db))
    }

    fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.second(self.db))
    }

    fn redundant(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first.is_redundant_with(self.db, env, second))
    }

    fn subtype(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first.is_subtype_of(self.db, env, second))
    }

    fn disjoint(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first.is_disjoint_from(self.db, env, second))
    }
}
