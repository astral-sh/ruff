//! Literal shapes read the expression types retained by the preceding inference pass.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::Type;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::source_expression::SourceExpressionOperation;
use crate::types::tuple::{TupleSpec, TupleSpecBuilder, VariableSegment};
use crate::types::unpacker::literal_sequence::{
    LiteralPrecisionEffects, LiteralSequenceEffects, LiteralSequenceFacts,
    MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE, next_value, remaining_budget_with,
    sequence_from_literal_elements_with,
};

fn buffer_quote(len: usize, capacity: usize, element_size: usize) -> RunResult<(usize, usize)> {
    let bytes = capacity
        .checked_mul(element_size)
        .ok_or(RunError::Contract("literal sequence allocation overflow"))?;
    let work = len
        .checked_mul(element_size)
        .and_then(|copy| bytes.checked_mul(2).and_then(|work| work.checked_add(copy)))
        .and_then(|work| work.checked_add(4))
        .ok_or(RunError::Contract("literal sequence work overflow"))?;
    Ok((work, bytes))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn tuple_literal_needs_promotion(
        &self,
        values: &[ast::Expr],
    ) -> RunResult<bool> {
        Ok(remaining_budget_with(
            values,
            MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE,
            LiteralSequenceFacts,
            self,
        )
        .await?
        .is_none())
    }

    pub(super) async fn sequence_from_literal_elements(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        values: &[ast::Expr],
        promote: bool,
    ) -> RunResult<TupleSpec<'db>> {
        sequence_from_literal_elements_with(
            values,
            promote,
            LiteralSequenceFacts,
            &ControlledLiteralSequenceEffects {
                effects: self,
                inference: builder,
            },
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LiteralPrecisionEffects
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next_value<'expr>(
        &self,
        values: &'expr [ast::Expr],
        cursor: &mut usize,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local(3, 0, || next_value(values, cursor)).await
    }

    async fn expanded_values<'expr>(
        &self,
        _starred: &'expr ast::ExprStarred,
    ) -> RunResult<Option<&'expr [ast::Expr]>> {
        self.unavailable(SourceOperation::Expression(
            SourceExpressionOperation::ExpressionKind,
        ))
        .await
    }

    async fn remaining_budget(
        &self,
        _values: &[ast::Expr],
        _remaining: usize,
    ) -> RunResult<Option<usize>> {
        self.unavailable(SourceOperation::Expression(
            SourceExpressionOperation::ExpressionKind,
        ))
        .await
    }
}

struct ControlledLiteralSequenceEffects<'effects, 'access, 'builder, 'run, 'db: 'run, 'ast, A> {
    effects: &'effects SourceEffects<'access, 'run, 'db, A>,
    inference: &'builder TypeInferenceBuilder<'db, 'ast>,
}

impl<'expr, 'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    LiteralSequenceEffects<'expr, Type<'db>, VariableSegment<'db>>
    for ControlledLiteralSequenceEffects<'_, '_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn builder(&self, capacity: usize) -> RunResult<TupleSpecBuilder<'db>> {
        // Capacity also pays for disposing of the retained elements if a later effect stops.
        let (work, bytes) = buffer_quote(0, capacity, size_of::<Type<'db>>())?;
        self.effects
            .local(work, bytes, || TupleSpecBuilder::with_capacity(capacity))
            .await
    }

    async fn next_value(
        &self,
        values: &'expr [ast::Expr],
        cursor: &mut usize,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.effects
            .local(3, 0, || next_value(values, cursor))
            .await
    }

    async fn literal_elements(
        &self,
        _expression: &'expr ast::Expr,
        _promote: bool,
    ) -> RunResult<Option<(&'expr [ast::Expr], bool)>> {
        self.effects
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ExpressionKind,
            ))
            .await
    }

    async fn sequence(
        &self,
        _values: &'expr [ast::Expr],
        _promote: bool,
    ) -> RunResult<TupleSpec<'db>> {
        self.effects
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ExpressionKind,
            ))
            .await
    }

    async fn iterable_length(&self, _expression: &ast::Expr) -> RunResult<Option<usize>> {
        self.effects
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ExpressionKind,
            ))
            .await
    }

    async fn spread(
        &self,
        _expression: &'expr ast::Expr,
        _promote: bool,
        _known_length: Option<usize>,
    ) -> RunResult<TupleSpec<'db>> {
        self.effects
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ExpressionKind,
            ))
            .await
    }

    async fn concat(
        &self,
        _builder: &mut TupleSpecBuilder<'db>,
        _unpacked: &TupleSpec<'db>,
    ) -> RunResult<()> {
        self.effects
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ExpressionKind,
            ))
            .await
    }

    async fn element(&self, expression: &'expr ast::Expr, promote: bool) -> RunResult<Type<'db>> {
        let work = self
            .inference
            .expressions
            .capacity()
            .checked_mul(4)
            .and_then(|work| work.checked_add(4))
            .ok_or(RunError::Contract(
                "literal sequence expression read overflow",
            ))?;
        let ty = self
            .effects
            .local(work, 0, || self.inference.expression_type(expression))
            .await?;
        if promote {
            self.effects
                .unavailable(SourceOperation::Expression(
                    SourceExpressionOperation::ExpressionKind,
                ))
                .await
        } else {
            Ok(ty)
        }
    }

    async fn push(&self, builder: &mut TupleSpecBuilder<'db>, element: Type<'db>) -> RunResult<()> {
        let TupleSpecBuilder::Fixed(elements) = builder else {
            return self
                .effects
                .unavailable(SourceOperation::Expression(
                    SourceExpressionOperation::ExpressionKind,
                ))
                .await;
        };
        // The initial capacity covers every direct element. Only concatenation can need more
        // storage, and it has its own effect boundary before any following push is reached.
        if elements.len() == elements.capacity() {
            return Err(RunError::Contract(
                "literal sequence exceeded its admitted capacity",
            ));
        }
        self.effects
            .local(size_of::<Type<'db>>() * 2 + 1, 0, || builder.push(element))
            .await
    }

    async fn build(&self, builder: TupleSpecBuilder<'db>) -> RunResult<TupleSpec<'db>> {
        let TupleSpecBuilder::Fixed(elements) = &builder else {
            return self
                .effects
                .unavailable(SourceOperation::Expression(
                    SourceExpressionOperation::ExpressionKind,
                ))
                .await;
        };
        let (work, bytes) =
            buffer_quote(elements.capacity(), elements.len(), size_of::<Type<'db>>())?;
        let requested_bytes = if elements.len() == elements.capacity() {
            0
        } else {
            bytes
        };
        let mut owner = Some(builder);
        self.effects
            .local(work, requested_bytes, || {
                let builder = owner.take().ok_or(RunError::Contract(
                    "literal sequence builder was already consumed",
                ))?;
                let spec = builder.build();
                #[cfg(test)]
                super::observations::observe(
                    self.effects.db(),
                    super::observations::Event::TupleShapeReady,
                );
                Ok(spec)
            })
            .await?
    }
}
