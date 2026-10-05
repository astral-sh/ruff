//! Canonical pair producers use the same ordered builders as ordinary type construction.

use std::marker::PhantomData;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::types::relation::source::{disjointness_condition, subtyping_condition};
use crate::types::set_theoretic::builder::controlled_union::{
    UnionFacts, add_in_place_with, try_build_with,
};
use crate::types::set_theoretic::builder::intersection_simplification::{
    self, IntersectionSimplificationEffects, SimplificationComparisonEffects,
};
use crate::types::set_theoretic::builder::{IntersectionPolarity, IntersectionSimplification};
use crate::types::set_theoretic::pair_intersection::{self, PairIntersectionEffects};
use crate::types::set_theoretic::pair_union::{PairUnionEffects, produce_with};
use crate::types::{
    IntersectionBuilder, LiteralValueType, LiteralValueTypeKind, Type, TypePair, UnionBuilder,
};
use crate::{Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn type_pair_union(
        &self,
        pair: TypePair<'db>,
    ) -> RunResult<Type<'db>> {
        produce_with(pair, self).await
    }

    pub(in crate::types::infer) async fn type_pair_intersection(
        &self,
        pair: TypePair<'db>,
    ) -> RunResult<Type<'db>> {
        pair_intersection::produce_with(pair, self).await
    }

    pub(in crate::types::infer) async fn type_pair_simplification(
        &self,
        pair: TypePair<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification> {
        intersection_simplification::produce_with(pair, polarity, self).await
    }
}

pub(super) async fn simplify_intersection_pair<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    first: Type<'db>,
    second: Type<'db>,
    polarity: IntersectionPolarity,
) -> RunResult<IntersectionSimplification> {
    intersection_simplification::simplify_with(
        first,
        second,
        polarity,
        &SimplificationComparison {
            access,
            lifetime: PhantomData,
        },
    )
    .await
}

struct SimplificationComparison<'a, 'run, A: ?Sized> {
    access: &'a A,
    lifetime: PhantomData<&'run ()>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SimplificationComparisonEffects<'db>
    for SimplificationComparison<'_, 'run, A>
{
    type Error = RunError;

    async fn kind(&self, literal: LiteralValueType<'db>) -> RunResult<LiteralValueTypeKind<'db>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(literal.kind())
            })
            .await)
    }

    async fn same_kind(
        &self,
        first: LiteralValueType<'db>,
        second: LiteralValueType<'db>,
    ) -> RunResult<bool> {
        self.compare_literals(first, second, || first.kind() == second.kind())
            .await
    }

    async fn same_literal(
        &self,
        first: LiteralValueType<'db>,
        second: LiteralValueType<'db>,
    ) -> RunResult<bool> {
        self.compare_literals(first, second, || first == second)
            .await
    }

    async fn is_promotable(&self, literal: LiteralValueType<'db>) -> RunResult<bool> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(literal.is_promotable())
            })
            .await)
    }

    async fn canonical(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification> {
        self.access
            .canonical_intersection_simplification(first, second, polarity)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SimplificationComparison<'_, 'run, A> {
    async fn compare_literals(
        &self,
        first: LiteralValueType<'db>,
        second: LiteralValueType<'db>,
        compare: impl FnOnce() -> bool,
    ) -> RunResult<bool> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                let work = Type::LiteralValue(first)
                    .inline_payload_bytes()
                    .checked_add(Type::LiteralValue(second).inline_payload_bytes())
                    .and_then(|work| work.checked_add(2))
                    .ok_or(RunError::Contract(
                        "literal simplification quotation overflow",
                    ))?;
                endpoint.admit_work(work)?;
                endpoint.check_completion()?;
                Ok(compare())
            })
            .await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> IntersectionSimplificationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn program(&self, pair: TypePair<'db>) -> RunResult<Program<'db>> {
        PairUnionEffects::program(self, pair).await
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<ProgramEnvironment<'db>> {
        PairUnionEffects::environment(self, program).await
    }

    async fn first(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::first(self, pair).await
    }

    async fn second(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::second(self, pair).await
    }

    async fn redundant(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(env).await?;
        self.access.is_redundant_with(first, second).await
    }

    async fn subtype(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        subtyping_condition(self.db(), env, first, second, self).await
    }

    async fn disjoint(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        disjointness_condition(self.db(), env, first, second, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PairIntersectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn program(&self, pair: TypePair<'db>) -> RunResult<Program<'db>> {
        PairUnionEffects::program(self, pair).await
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<ProgramEnvironment<'db>> {
        PairUnionEffects::environment(self, program).await
    }

    async fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        SourceEffects::new_intersection(self, env).await
    }

    async fn first(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::first(self, pair).await
    }

    async fn second(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::second(self, pair).await
    }

    async fn intersection_add(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_positive(builder, ty).await
    }

    async fn intersection_build(
        &self,
        mut builder: IntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        SourceEffects::intersection_build(self, &mut builder).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PairUnionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn program(&self, pair: TypePair<'db>) -> RunResult<Program<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        let program = self.field(fields.program()).await?;
        self.check_program(program)?;
        Ok(program)
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<ProgramEnvironment<'db>> {
        self.local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
            ProgramEnvironment::from_program(program)
        })
        .await
    }

    async fn new_union(&self, env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>> {
        // The six fields, Cell-backed environment clone, and empty Vec need only fixed work.
        // Prepay empty-owner retirement before a later child can stop with the builder retained.
        let construction_work = 16;
        let cleanup_work = 8;
        self.local_with_fixed_transfers(construction_work + cleanup_work, 0, || {
            UnionBuilder::new(self.db(), env)
        })
        .await
    }

    async fn first(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        self.field(fields.first()).await
    }

    async fn second(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        self.field(fields.second()).await
    }

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        add_in_place_with(builder, ty, UnionFacts, self).await
    }

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        Ok(try_build_with(builder, UnionFacts, self)
            .await?
            .unwrap_or(Type::Never))
    }
}
