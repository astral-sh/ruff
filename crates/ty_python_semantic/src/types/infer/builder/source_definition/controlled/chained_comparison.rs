//! Chain operands borrow the existing expression owner and canonical source access.

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::{ExpressionNodeKey, Truthiness};

use super::storage::{slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::bool::BoolError;
#[cfg(test)]
use crate::types::infer::builder::chained_comparison::guarded_observations;
use crate::types::infer::builder::chained_comparison::{
    ChainFacts, ChainInput, ChainItem, ChainState, ChainedComparisonEffects,
    ChainedComparisonOperation, GuardedTypeEffects, OperandMode, guarded_type_with,
    next_chain_type_with,
};
use crate::types::infer::builder::{TypeInferenceBuilder, is_collection_literal, local};
use crate::types::infer::comparisons::UnsupportedComparisonError;
use crate::types::set_theoretic::assembly::{self, TypeAssemblyEffects, TypeElements};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::{IntersectionBuilder, Type, TypeContext};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GuardedTypeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn new_intersection(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        SourceEffects::new_intersection(self, env).await
    }

    async fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_positive(builder, ty).await?;
        #[cfg(test)]
        guarded_observations::observe(guarded_observations::Stage::AfterPositive, self.db());
        Ok(())
    }

    async fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_negative(builder, ty).await?;
        #[cfg(test)]
        guarded_observations::observe(guarded_observations::Stage::AfterNegative, self.db());
        Ok(())
    }

    async fn build(&self, builder: &mut IntersectionBuilder<'db>) -> RunResult<Type<'db>> {
        self.intersection_build(builder).await
    }
}

struct ChainElements<'builder, 'effects, 'db, 'ast, 'expr, E> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    input: ChainInput<'db, 'expr>,
    state: &'builder mut ChainState<'db>,
    effects: &'effects E,
}

impl<'db, 'ast, E: ChainedComparisonEffects<'db, 'ast>> TypeElements<'db>
    for ChainElements<'_, '_, 'db, 'ast, '_, E>
{
    type Error = E::Error;
    type Item = Type<'db>;

    async fn next(&mut self) -> Result<Option<Type<'db>>, Self::Error> {
        next_chain_type_with(
            self.builder,
            self.input,
            self.state,
            ChainFacts,
            self.effects,
        )
        .await
    }
}

struct ChainAssembly<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeAssemblyEffects<'db>
    for ChainAssembly<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn union<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        let mut builder = PairUnionEffects::new_union(self.source, env).await?;
        PairUnionEffects::union_add(self.source, &mut builder, first.into()).await?;
        PairUnionEffects::union_add(self.source, &mut builder, second.into()).await?;
        while let Some(element) = remaining.next().await? {
            PairUnionEffects::union_add(self.source, &mut builder, element.into()).await?;
        }
        PairUnionEffects::union_build(self.source, builder).await
    }

    async fn intersection<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: I::Item,
        _second: I::Item,
        _remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::ChainedComparison(
                ChainedComparisonOperation::GuardedIntersection,
            ))
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ChainedComparisonEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn has_peer_literal(&self, expression: &ast::ExprBoolOp) -> RunResult<bool> {
        let work = Self::checked(expression.values.len().checked_add(1))?;
        self.local(work, 0, || {
            expression.values.iter().skip(1).any(is_collection_literal)
        })
        .await
    }

    async fn prefer_peer_context(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _context: TypeContext<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::PeerPreference,
        ))
        .await
    }

    async fn state(&self, op: ast::BoolOp, track_peer_types: bool) -> RunResult<ChainState<'db>> {
        // No peer storage exists until an ambiguous contributor is accumulated. That effect
        // must admit its backing and disposal before installing it in this retained state.
        self.local(size_of::<ChainState<'db>>(), 0, || {
            ChainState::new(op, track_peer_types)
        })
        .await
    }

    async fn next<'expr>(
        &self,
        input: ChainInput<'db, 'expr>,
        state: &mut ChainState<'db>,
    ) -> RunResult<Option<ChainItem<'db, 'expr>>> {
        self.local(8, 0, || state.next(input)).await
    }

    async fn aggregate(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        input: ChainInput<'db, '_>,
        state: &mut ChainState<'db>,
    ) -> RunResult<Type<'db>> {
        let db = builder.db();
        let env = builder.program_environment().clone();
        let mut elements = ChainElements {
            builder,
            input,
            state,
            effects: self,
        };
        assembly::union_from_elements(db, &env, &mut elements, &ChainAssembly { source: self })
            .await
    }

    async fn infer_operand(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
        peer: Option<Type<'db>>,
        mode: OperandMode,
    ) -> RunResult<Type<'db>> {
        if peer.is_some() {
            return self
                .unavailable(SourceOperation::ChainedComparison(
                    ChainedComparisonOperation::PeerInference,
                ))
                .await;
        }
        match mode {
            OperandMode::Comparison | OperandMode::BooleanLast => {
                local::source::expression(builder, expression, context, self).await
            }
            OperandMode::BooleanEarlier => {
                local::source::maybe_standalone_expression(builder, expression, context, self).await
            }
        }
    }

    async fn stored_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let work = Self::checked(
            builder
                .expressions
                .capacity()
                .checked_mul(4)
                .and_then(|count| count.checked_add(4)),
        )?;
        self.local(work, 0, || builder.expression_type(expression))
            .await
    }

    async fn compare_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
        range: TextRange,
    ) -> RunResult<Result<Type<'db>, UnsupportedComparisonError<'db>>> {
        SourceEffects::compare_types(self, &builder.context, left, op, right, range).await
    }

    async fn report_unsupported(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _error: UnsupportedComparisonError<'db>,
        _range: TextRange,
        _left: &ast::Expr,
        _right: &ast::Expr,
        _left_type: Type<'db>,
        _right_type: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::UnsupportedDiagnostic,
        ))
        .await
    }

    async fn boolean_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::BooleanType,
        ))
        .await
    }

    async fn try_bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Result<Truthiness, BoolError<'db>>> {
        self.try_type_truthiness(builder.program_environment(), ty)
            .await
    }

    async fn report_bool_error(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _error: &BoolError<'db>,
        _range: TextRange,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::TruthinessDiagnostic,
        ))
        .await
    }

    async fn bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness> {
        self.type_truthiness(builder.program_environment(), ty)
            .await
    }

    async fn record_type(&self, state: &mut ChainState<'db>, ty: Type<'db>) -> RunResult<()> {
        self.local(1, 0, || state.last_type = ty).await
    }

    async fn record_prefix(
        &self,
        state: &mut ChainState<'db>,
        truthiness: Truthiness,
    ) -> RunResult<()> {
        self.local(1, 0, || state.preceding_truthiness = truthiness)
            .await
    }

    async fn mark_done(&self, state: &mut ChainState<'db>) -> RunResult<()> {
        self.local(1, 0, || state.done = true).await
    }

    async fn peer_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _state: &mut ChainState<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::PeerUnion,
        ))
        .await
    }

    async fn add_peer(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _state: &mut ChainState<'db>,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ChainedComparison(
            ChainedComparisonOperation::PeerAccumulation,
        ))
        .await
    }

    async fn guarded_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        op: ast::BoolOp,
    ) -> RunResult<Type<'db>> {
        guarded_type_with(builder.db(), builder.program_environment(), ty, op, self).await
    }

    async fn store_truthiness(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::ExprCompare,
        truthiness: Option<Truthiness>,
    ) -> RunResult<()> {
        let entries = builder.comparison_truthiness.len();
        let capacity = builder.comparison_truthiness.capacity();
        let previous_backing = builder.source_truthiness_backing;
        let (quote, backing) = table_merge::<(ExpressionNodeKey, Truthiness)>(
            entries,
            capacity,
            usize::from(truthiness.is_some()),
            previous_backing,
        )
        .ok_or(RunError::Contract(
            "comparison truthiness storage quotation overflow",
        ))?;
        self.local(quote.work, quote.bytes, || {
            let expression = ast::ExprRef::Compare(expression).into();
            builder.source_truthiness_backing = backing;
            match truthiness {
                Some(truthiness) => {
                    builder.comparison_truthiness.reserve(1);
                    builder.comparison_truthiness.insert(expression, truthiness);
                }
                None => {
                    builder.comparison_truthiness.remove(&expression);
                }
            }
            builder.source_truthiness_backing = slots(builder.comparison_truthiness.capacity())
                .map_or(builder.source_truthiness_backing, |observed| {
                    previous_backing.max(observed)
                });
        })
        .await
    }
}
