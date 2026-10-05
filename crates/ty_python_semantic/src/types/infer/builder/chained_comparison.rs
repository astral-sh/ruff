//! Boolean and comparison chains share operand traversal and result aggregation.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Truthiness;

use super::{TypeInferenceBuilder, is_collection_literal, prefer_collection_literal_peer_context};
use crate::types::bool::BoolError;
use crate::types::diagnostic::report_unsupported_comparison;
use crate::types::infer::comparisons::{self, UnsupportedComparisonError};
use crate::types::set_theoretic::builder::intersection_assembly;
use crate::types::{
    IntersectionBuilder, KnownClass, Type, TypeContext, UnionAccumulator, UnionType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum ChainedComparisonOperation {
    Dispatch,
    TypeComparison,
    PeerPreference,
    PeerInference,
    PeerAccumulation,
    PeerUnion,
    Truthiness,
    TruthinessDiagnostic,
    GuardedIntersection,
    ResultUnion,
    UnsupportedDiagnostic,
    BooleanType,
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum ChainInput<'db, 'expr> {
    Boolean {
        expression: &'expr ast::ExprBoolOp,
        context: TypeContext<'db>,
    },
    Comparison(&'expr ast::ExprCompare),
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum ChainOperand<'db, 'expr> {
    Boolean {
        expression: &'expr ast::Expr,
        context: TypeContext<'db>,
    },
    Comparison {
        left: &'expr ast::Expr,
        op: ast::CmpOp,
        right: &'expr ast::Expr,
    },
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) struct ChainItem<'db, 'expr> {
    pub operand: ChainOperand<'db, 'expr>,
    pub last: bool,
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum OperandMode {
    Comparison,
    BooleanEarlier,
    BooleanLast,
}

pub(in crate::types::infer) struct ChainState<'db> {
    pub position: usize,
    pub op: ast::BoolOp,
    pub done: bool,
    /// Combined truthiness of all operands except the last.
    ///
    /// For `a < b < c`, evaluating the chain as a value tests `a < b` to decide whether to
    /// short-circuit, but returns `b < c` without testing it if evaluation continues.
    /// Keeping the preceding checks separate lets comparison inference combine this result
    /// with the final comparison's truthiness when analyzing the chain as a condition.
    /// Using the value type instead would model testing the returned object again, which can
    /// give a different answer when a comparison returns an object with mutable truthiness.
    pub preceding_truthiness: Truthiness,
    pub last_type: Type<'db>,
    pub track_peer_types: bool,
    pub peer_types: Option<UnionAccumulator<'db>>,
}

impl<'db> ChainState<'db> {
    pub(in crate::types::infer) fn new(op: ast::BoolOp, track_peer_types: bool) -> Self {
        Self {
            position: 0,
            op,
            done: false,
            preceding_truthiness: Truthiness::from(op.is_and()),
            last_type: Type::unknown(),
            track_peer_types,
            peer_types: None,
        }
    }

    pub(in crate::types::infer) fn next<'expr>(
        &mut self,
        input: ChainInput<'db, 'expr>,
    ) -> Option<ChainItem<'db, 'expr>> {
        let item = match input {
            ChainInput::Boolean {
                expression,
                context,
            } => ChainItem {
                operand: ChainOperand::Boolean {
                    expression: expression.values.get(self.position)?,
                    context,
                },
                last: self.position + 1 == expression.values.len(),
            },
            ChainInput::Comparison(expression) => {
                let left = expression.operands.get(self.position)?;
                let op = *expression.ops.get(self.position)?;
                let right = expression.operands.get(self.position + 1)?;
                ChainItem {
                    operand: ChainOperand::Comparison { left, op, right },
                    last: self.position + 1 == expression.iter().len(),
                }
            }
        };
        self.position += 1;
        Some(item)
    }
}

pub(in crate::types::infer) struct ChainFacts;
pub(super) struct OrdinaryChainedComparisonEffects;

shared_semantic_family! {
    #[synchronous(SynchronousGuardedTypeEffects)]
    pub(in crate::types::infer) trait GuardedTypeEffects<'db> {
        type Error;

        #[operation(child)]
        async fn new_intersection(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Result<IntersectionBuilder<'db>, Self::Error>;
        #[operation(child)]
        async fn add_positive(&self, builder: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn add_negative(&self, builder: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn build(&self, builder: &mut IntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(guarded_type_sync)]
    #[capabilities(effects = GuardedTypeEffects)]
    #[passive_values(Type::AlwaysTruthy, Type::AlwaysFalsy)]
    pub(in crate::types::infer) async fn guarded_type_with<'db, E: GuardedTypeEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        op: ast::BoolOp,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut builder = effects.new_intersection(db, env).await?;
        effects.add_positive(&mut builder, ty).await?;
        effects.add_negative(&mut builder, match op {
            ast::BoolOp::And => Type::AlwaysTruthy,
            ast::BoolOp::Or => Type::AlwaysFalsy,
        }).await?;
        effects.build(&mut builder).await
    }
}

impl<'db> SynchronousGuardedTypeEffects<'db> for OrdinaryChainedComparisonEffects {
    type Error = Infallible;

    fn new_intersection(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<IntersectionBuilder<'db>, Infallible> {
        Ok(IntersectionBuilder::new(db, env))
    }

    fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.add_positive_in_place(ty);
        Ok(())
    }

    fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.add_negative_in_place(ty);
        Ok(())
    }

    fn build(&self, builder: &mut IntersectionBuilder<'db>) -> Result<Type<'db>, Infallible> {
        Ok(intersection_assembly::build(builder))
    }
}

#[cfg(test)]
pub(in crate::types::infer) mod guarded_observations {
    use std::cell::Cell;

    use crate::Db;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types::infer) enum Stage {
        AfterPositive,
        AfterNegative,
    }

    thread_local! {
        static REMAINING: Cell<[Option<usize>; 2]> = const { Cell::new([None; 2]) };
        static CANCEL: Cell<Option<Stage>> = const { Cell::new(None) };
    }

    pub(in crate::types::infer) fn reset(cancel: Option<Stage>) {
        REMAINING.set([None; 2]);
        CANCEL.set(cancel);
    }

    pub(in crate::types::infer) fn remaining() -> [Option<usize>; 2] {
        REMAINING.get()
    }

    pub(in crate::types::infer) fn observe(stage: Stage, db: &dyn Db) {
        let index = match stage {
            Stage::AfterPositive => 0,
            Stage::AfterNegative => 1,
        };
        let mut remaining = REMAINING.get();
        if remaining[index].is_none() {
            remaining[index] = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            REMAINING.set(remaining);
        }
        if CANCEL.get() == Some(stage) {
            CANCEL.set(None);
            db.cancellation_token().cancel();
        }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousChainedComparisonEffects)]
    pub(in crate::types::infer) trait ChainedComparisonEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        async fn has_peer_literal(&self, expression: &ast::ExprBoolOp) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn prefer_peer_context(&self, builder: &TypeInferenceBuilder<'db, 'ast>, context: TypeContext<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn state(&self, op: ast::BoolOp, track_peer_types: bool) -> Result<ChainState<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next<'expr>(&self, input: ChainInput<'db, 'expr>, state: &mut ChainState<'db>) -> Result<Option<ChainItem<'db, 'expr>>, Self::Error>;
        #[operation(child)]
        async fn aggregate(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, input: ChainInput<'db, '_>, state: &mut ChainState<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn infer_operand(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, context: TypeContext<'db>, peer: Option<Type<'db>>, mode: OperandMode) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn stored_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn compare_types(&self, builder: &TypeInferenceBuilder<'db, 'ast>, left: Type<'db>, op: ast::CmpOp, right: Type<'db>, range: TextRange) -> Result<Result<Type<'db>, UnsupportedComparisonError<'db>>, Self::Error>;
        #[operation(source)]
        async fn report_unsupported(&self, builder: &TypeInferenceBuilder<'db, 'ast>, error: UnsupportedComparisonError<'db>, range: TextRange, left: &ast::Expr, right: &ast::Expr, left_type: Type<'db>, right_type: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn boolean_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn try_bool(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Result<Truthiness, BoolError<'db>>, Self::Error>;
        #[operation(source)]
        async fn report_bool_error(&self, builder: &TypeInferenceBuilder<'db, 'ast>, error: &BoolError<'db>, range: TextRange) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn bool(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(local)]
        async fn record_type(&self, state: &mut ChainState<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn record_prefix(&self, state: &mut ChainState<'db>, truthiness: Truthiness) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn mark_done(&self, state: &mut ChainState<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn peer_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, state: &mut ChainState<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn add_peer(&self, builder: &TypeInferenceBuilder<'db, 'ast>, state: &mut ChainState<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn guarded_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, op: ast::BoolOp) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn store_truthiness(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprCompare, truthiness: Option<Truthiness>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ChainFacts {
        fn op(&self, input: ChainInput<'_, '_>) -> ast::BoolOp {
            match input {
                ChainInput::Boolean { expression, .. } => expression.op,
                ChainInput::Comparison(_) => ast::BoolOp::And,
            }
        }

        fn first<'expr>(&self, expression: &'expr ast::ExprCompare) -> &'expr ast::Expr {
            expression.first_operand()
        }

        fn default_context<'db>(&self) -> TypeContext<'db> {
            TypeContext::default()
        }

        fn needs_peer(&self, item: ChainItem<'_, '_>) -> bool {
            match item.operand {
                ChainOperand::Boolean { expression, .. } => is_collection_literal(expression),
                ChainOperand::Comparison { .. } => false,
            }
        }

        fn range(&self, item: ChainItem<'_, '_>) -> TextRange {
            match item.operand {
                ChainOperand::Boolean { expression, .. } => expression.range(),
                ChainOperand::Comparison { left, right, .. } => TextRange::new(left.start(), right.end()),
            }
        }

        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn never<'db>(&self) -> Type<'db> { Type::Never }
        fn fallback(&self, error: &BoolError<'_>) -> Truthiness { error.fallback_truthiness() }
        fn combine(&self, state: &ChainState<'_>, truthiness: Truthiness) -> Truthiness {
            match state.op {
                ast::BoolOp::And => state.preceding_truthiness.and(truthiness),
                ast::BoolOp::Or => state.preceding_truthiness.or(truthiness),
            }
        }
        fn condition(&self, prefix: Truthiness, last: Truthiness) -> Truthiness { prefix.and(last) }
        fn prefix_is_false(&self, state: &ChainState<'_>) -> bool { state.preceding_truthiness == Truthiness::AlwaysFalse }
        fn truthiness_differs(&self, left: Truthiness, right: Truthiness) -> bool { left != right }
        fn has_multiple_comparisons(&self, expression: &ast::ExprCompare) -> bool { expression.ops.len() > 1 }
    }

    #[synchronous(infer_chain_sync)]
    #[capabilities(effects = ChainedComparisonEffects, facts = ChainFacts)]
    #[passive_values(OperandMode::Comparison, Truthiness::AlwaysFalse)]
    pub(in crate::types::infer) async fn infer_chain_with<'db, 'ast, E: ChainedComparisonEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        input: ChainInput<'db, '_>,
        facts: ChainFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if let ChainInput::Comparison(expression) = input {
            effects.infer_operand(builder, facts.first(expression), facts.default_context(), None, OperandMode::Comparison).await?;
        }
        let track_peer_types = match input {
            ChainInput::Boolean { expression, context } => {
                effects.has_peer_literal(expression).await?
                    && effects.prefer_peer_context(builder, context).await?
            }
            ChainInput::Comparison(_) => false,
        };
        let mut state = effects.state(facts.op(input), track_peer_types).await?;
        let ty = effects.aggregate(builder, input, &mut state).await?;

        if let ChainInput::Comparison(expression) = input
            && facts.has_multiple_comparisons(expression)
        {
            // The final comparison is returned without a bool conversion. Its truthiness as a
            // condition can differ from testing the chain's result type a second time.
            let truthiness = if facts.prefix_is_false(&state) {
                Truthiness::AlwaysFalse
            } else {
                let last = effects.bool(builder, state.last_type).await?;
                facts.condition(state.preceding_truthiness, last)
            };
            let value_truthiness = effects.bool(builder, ty).await?;
            let retained = if facts.truthiness_differs(truthiness, value_truthiness) { Some(truthiness) } else { None };
            effects.store_truthiness(builder, expression, retained).await?;
        }
        Ok(ty)
    }

    #[synchronous(next_chain_type_sync)]
    #[capabilities(effects = ChainedComparisonEffects, facts = ChainFacts)]
    #[passive_values(OperandMode::Comparison, OperandMode::BooleanEarlier, OperandMode::BooleanLast)]
    pub(in crate::types::infer) async fn next_chain_type_with<'db, 'ast, E: ChainedComparisonEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        input: ChainInput<'db, '_>,
        state: &mut ChainState<'db>,
        facts: ChainFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let Some(item) = effects.next(input, state).await? else { return Ok(None); };
        let peer = if state.done || !state.track_peer_types || !facts.needs_peer(item) {
            None
        } else {
            effects.peer_type(builder, state).await?
        };
        let range = facts.range(item);
        let ty = match item.operand {
            ChainOperand::Boolean { expression, context } => {
                let mode = if item.last { OperandMode::BooleanLast } else { OperandMode::BooleanEarlier };
                effects.infer_operand(builder, expression, context, peer, mode).await?
            }
            ChainOperand::Comparison { left, op, right } => {
                let left_type = effects.stored_type(builder, left).await?;
                let right_type = effects.infer_operand(builder, right, facts.default_context(), None, OperandMode::Comparison).await?;
                match effects.compare_types(builder, left_type, op, right_type, range).await? {
                    Ok(ty) => ty,
                    Err(error) => {
                        effects.report_unsupported(builder, error, range, left, right, left_type, right_type).await?;
                        match op {
                            ast::CmpOp::In | ast::CmpOp::NotIn | ast::CmpOp::Is | ast::CmpOp::IsNot => effects.boolean_type(builder).await?,
                            _ => facts.unknown(),
                        }
                    }
                }
            }
        };
        effects.record_type(state, ty).await?;

        if item.last {
            return Ok(Some(if state.done { facts.never() } else { ty }));
        }

        // Continue inferring and checking every operand after a short circuit so that stored
        // expression types and diagnostics are the same as for the ordinary expression walk.
        let truthiness = match effects.try_bool(builder, ty).await? {
            Ok(truthiness) => truthiness,
            Err(error) => {
                effects.report_bool_error(builder, &error, range).await?;
                facts.fallback(&error)
            }
        };
        let prefix = facts.combine(state, truthiness);
        effects.record_prefix(state, prefix).await?;
        if state.done {
            return Ok(Some(facts.never()));
        }
        let contribution = match (truthiness, state.op) {
            (Truthiness::AlwaysTrue, ast::BoolOp::And)
            | (Truthiness::AlwaysFalse, ast::BoolOp::Or) => facts.never(),
            (Truthiness::AlwaysFalse, ast::BoolOp::And)
            | (Truthiness::AlwaysTrue, ast::BoolOp::Or) => {
                effects.mark_done(state).await?;
                ty
            }
            (Truthiness::Ambiguous, _) => {
                if state.track_peer_types {
                    effects.add_peer(builder, state, ty).await?;
                }
                effects.guarded_type(builder, ty, state.op).await?
            }
        };
        Ok(Some(contribution))
    }
}

impl<'db, 'ast> SynchronousChainedComparisonEffects<'db, 'ast>
    for OrdinaryChainedComparisonEffects
{
    type Error = Infallible;

    fn has_peer_literal(&self, expression: &ast::ExprBoolOp) -> Result<bool, Infallible> {
        Ok(expression.values.iter().skip(1).any(is_collection_literal))
    }

    fn prefer_peer_context(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        context: TypeContext<'db>,
    ) -> Result<bool, Infallible> {
        Ok(prefer_collection_literal_peer_context(
            builder.db(),
            builder.program_environment(),
            context,
        ))
    }

    fn state(
        &self,
        op: ast::BoolOp,
        track_peer_types: bool,
    ) -> Result<ChainState<'db>, Infallible> {
        Ok(ChainState::new(op, track_peer_types))
    }

    fn next<'expr>(
        &self,
        input: ChainInput<'db, 'expr>,
        state: &mut ChainState<'db>,
    ) -> Result<Option<ChainItem<'db, 'expr>>, Infallible> {
        Ok(state.next(input))
    }

    fn aggregate(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        input: ChainInput<'db, '_>,
        state: &mut ChainState<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let db = builder.db();
        let env = builder.program_environment().clone();
        let elements = std::iter::from_fn(|| {
            match next_chain_type_sync(builder, input, state, ChainFacts, self) {
                Ok(element) => element,
                Err(never) => match never {},
            }
        });
        Ok(UnionType::from_elements(db, &env, elements))
    }

    fn infer_operand(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
        peer: Option<Type<'db>>,
        mode: OperandMode,
    ) -> Result<Type<'db>, Infallible> {
        Ok(match mode {
            OperandMode::Comparison => builder.infer_expression(expression, context),
            OperandMode::BooleanEarlier => builder
                .infer_maybe_standalone_expression_with_collection_literal_peer_context(
                    expression, context, peer,
                ),
            OperandMode::BooleanLast => builder
                .infer_expression_with_collection_literal_peer_context(expression, context, peer),
        })
    }

    fn stored_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.expression_type(expression))
    }

    fn compare_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
        range: TextRange,
    ) -> Result<Result<Type<'db>, UnsupportedComparisonError<'db>>, Infallible> {
        Ok(comparisons::infer_binary_type_comparison(
            &builder.context,
            left,
            op,
            right,
            range,
        ))
    }

    fn report_unsupported(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        error: UnsupportedComparisonError<'db>,
        range: TextRange,
        left: &ast::Expr,
        right: &ast::Expr,
        left_type: Type<'db>,
        right_type: Type<'db>,
    ) -> Result<(), Infallible> {
        report_unsupported_comparison(
            &builder.context,
            &error,
            range,
            left,
            right,
            left_type,
            right_type,
        );
        Ok(())
    }

    fn boolean_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Bool.to_instance(builder.db(), builder.program_environment()))
    }

    fn try_bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Result<Truthiness, BoolError<'db>>, Infallible> {
        Ok(ty.try_bool(builder.db(), builder.program_environment()))
    }

    fn report_bool_error(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        error: &BoolError<'db>,
        range: TextRange,
    ) -> Result<(), Infallible> {
        error.report_diagnostic(&builder.context, range);
        Ok(())
    }

    fn bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Truthiness, Infallible> {
        Ok(ty.bool(builder.db(), builder.program_environment()))
    }

    fn record_type(&self, state: &mut ChainState<'db>, ty: Type<'db>) -> Result<(), Infallible> {
        state.last_type = ty;
        Ok(())
    }

    fn record_prefix(
        &self,
        state: &mut ChainState<'db>,
        truthiness: Truthiness,
    ) -> Result<(), Infallible> {
        state.preceding_truthiness = truthiness;
        Ok(())
    }

    fn mark_done(&self, state: &mut ChainState<'db>) -> Result<(), Infallible> {
        state.done = true;
        Ok(())
    }

    fn peer_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut ChainState<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(state
            .peer_types
            .as_mut()
            .map(|peers| peers.get_or_build(builder.db(), builder.program_environment())))
    }

    fn add_peer(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut ChainState<'db>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        match &mut state.peer_types {
            Some(peers) => peers.add(builder.db(), builder.program_environment(), ty),
            None => state.peer_types = Some(UnionAccumulator::new(ty)),
        }
        Ok(())
    }

    fn guarded_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        op: ast::BoolOp,
    ) -> Result<Type<'db>, Infallible> {
        guarded_type_sync(builder.db(), builder.program_environment(), ty, op, self)
    }

    fn store_truthiness(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::ExprCompare,
        truthiness: Option<Truthiness>,
    ) -> Result<(), Infallible> {
        let expression = ast::ExprRef::Compare(expression).into();
        match truthiness {
            Some(truthiness) => {
                builder.comparison_truthiness.insert(expression, truthiness);
            }
            None => {
                builder.comparison_truthiness.remove(&expression);
            }
        }
        Ok(())
    }
}
