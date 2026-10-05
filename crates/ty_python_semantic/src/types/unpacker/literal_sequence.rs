//! Literal precision counting and shape construction share their traversal order.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{literal_iterable_length, literal_sequence_elements, sequence_elts};
use crate::types::tuple::{Tuple, TupleBuilder};

/// Limit literal precision in large tuple expressions to avoid pathological inference costs.
pub(in crate::types) const MAX_TUPLE_LENGTH_FOR_UNANNOTATED_LITERAL_INFERENCE: usize = 64;

pub(in crate::types) struct LiteralSequenceFacts;
pub(super) struct OrdinaryLiteralPrecisionEffects;

pub(super) struct OrdinaryLiteralSequenceEffects<'effects, Element, Spread, Concat> {
    pub(super) element: &'effects Element,
    pub(super) spread: &'effects Spread,
    pub(super) concat: &'effects Concat,
}

shared_semantic_family! {
    #[synchronous(SynchronousLiteralPrecisionEffects)]
    pub(in crate::types) trait LiteralPrecisionEffects {
        type Error;
        #[operation(local)]
        #[progress]
        async fn next_value<'expr>(&self, values: &'expr [ast::Expr], cursor: &mut usize) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(source)]
        async fn expanded_values<'expr>(&self, starred: &'expr ast::ExprStarred) -> Result<Option<&'expr [ast::Expr]>, Self::Error>;
        #[operation(source)]
        async fn remaining_budget(&self, values: &[ast::Expr], remaining: usize) -> Result<Option<usize>, Self::Error>;
    }

    #[synchronous(SynchronousLiteralSequenceEffects)]
    pub(in crate::types) trait LiteralSequenceEffects<'expr, T, V> {
        type Error;
        #[operation(local)]
        async fn builder(&self, capacity: usize) -> Result<TupleBuilder<T, V>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_value(&self, values: &'expr [ast::Expr], cursor: &mut usize) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(source)]
        async fn literal_elements(&self, expression: &'expr ast::Expr, promote: bool) -> Result<Option<(&'expr [ast::Expr], bool)>, Self::Error>;
        #[operation(source)]
        async fn sequence(&self, values: &'expr [ast::Expr], promote: bool) -> Result<Tuple<T, V>, Self::Error>;
        #[operation(local)]
        async fn iterable_length(&self, expression: &ast::Expr) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn spread(&self, expression: &'expr ast::Expr, promote: bool, known_length: Option<usize>) -> Result<Tuple<T, V>, Self::Error>;
        #[operation(source)]
        async fn concat(&self, builder: &mut TupleBuilder<T, V>, unpacked: &Tuple<T, V>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn element(&self, expression: &'expr ast::Expr, promote: bool) -> Result<T, Self::Error>;
        #[operation(local)]
        async fn push(&self, builder: &mut TupleBuilder<T, V>, element: T) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn build(&self, builder: TupleBuilder<T, V>) -> Result<Tuple<T, V>, Self::Error>;
    }

    #[finite_capability]
    impl LiteralSequenceFacts {
        fn consume_position(&self, remaining: usize) -> Option<usize> {
            remaining.checked_sub(1)
        }

        fn len(&self, values: &[ast::Expr]) -> usize {
            values.len()
        }
    }

    #[synchronous(remaining_budget_sync)]
    #[capabilities(effects = LiteralPrecisionEffects, facts = LiteralSequenceFacts)]
    #[passive_values()]
    pub(in crate::types) async fn remaining_budget_with<E: LiteralPrecisionEffects>(
        values: &[ast::Expr], remaining: usize, facts: LiteralSequenceFacts, effects: &E,
    ) -> Result<Option<usize>, E::Error> {
        #[passive_state]
        let mut remaining = remaining;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(value) = effects.next_value(values, &mut cursor).await? {
            let expanded = if let ast::Expr::Starred(starred) = value {
                effects.expanded_values(starred).await?
            } else {
                None
            };
            let next = if let Some(values) = expanded {
                effects.remaining_budget(values, remaining).await?
            } else {
                facts.consume_position(remaining)
            };
            let Some(next) = next else {
                return Ok(None);
            };
            remaining = next;
        }
        Ok(Some(remaining))
    }

    #[synchronous(sequence_from_literal_elements_sync)]
    #[capabilities(effects = LiteralSequenceEffects, facts = LiteralSequenceFacts)]
    #[passive_values()]
    pub(in crate::types) async fn sequence_from_literal_elements_with<'expr, T, V, E: LiteralSequenceEffects<'expr, T, V>>(
        values: &'expr [ast::Expr], promote: bool, facts: LiteralSequenceFacts, effects: &E,
    ) -> Result<Tuple<T, V>, E::Error> {
        let mut builder = effects.builder(facts.len(values)).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(value) = effects.next_value(values, &mut cursor).await? {
            if let ast::Expr::Starred(starred) = value {
                let unpacked = if let Some((values, promote)) = effects.literal_elements(&starred.value, promote).await? {
                    effects.sequence(values, promote).await?
                } else {
                    let known_length = effects.iterable_length(&starred.value).await?;
                    effects.spread(value, promote, known_length).await?
                };
                effects.concat(&mut builder, &unpacked).await?;
            } else {
                let element = effects.element(value, promote).await?;
                effects.push(&mut builder, element).await?;
            }
        }
        effects.build(builder).await
    }
}

impl SynchronousLiteralPrecisionEffects for OrdinaryLiteralPrecisionEffects {
    type Error = Infallible;

    fn next_value<'expr>(
        &self,
        values: &'expr [ast::Expr],
        cursor: &mut usize,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(next_value(values, cursor))
    }

    fn expanded_values<'expr>(
        &self,
        starred: &'expr ast::ExprStarred,
    ) -> Result<Option<&'expr [ast::Expr]>, Infallible> {
        Ok(sequence_elts(starred.value.expression_value()))
    }

    fn remaining_budget(
        &self,
        values: &[ast::Expr],
        remaining: usize,
    ) -> Result<Option<usize>, Infallible> {
        remaining_budget_sync(values, remaining, LiteralSequenceFacts, self)
    }
}

pub(in crate::types) fn next_value<'expr>(
    values: &'expr [ast::Expr],
    cursor: &mut usize,
) -> Option<&'expr ast::Expr> {
    let value = values.get(*cursor)?;
    *cursor += 1;
    Some(value)
}

impl<'expr, T, V, Element, Spread, Concat> SynchronousLiteralSequenceEffects<'expr, T, V>
    for OrdinaryLiteralSequenceEffects<'_, Element, Spread, Concat>
where
    Element: Fn(&'expr ast::Expr, bool) -> T,
    Spread: Fn(&'expr ast::Expr, bool, Option<usize>) -> Tuple<T, V>,
    Concat: Fn(TupleBuilder<T, V>, &Tuple<T, V>) -> TupleBuilder<T, V>,
{
    type Error = Infallible;

    fn builder(&self, capacity: usize) -> Result<TupleBuilder<T, V>, Infallible> {
        Ok(TupleBuilder::with_capacity(capacity))
    }

    fn next_value(
        &self,
        values: &'expr [ast::Expr],
        cursor: &mut usize,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(next_value(values, cursor))
    }

    fn literal_elements(
        &self,
        expression: &'expr ast::Expr,
        promote: bool,
    ) -> Result<Option<(&'expr [ast::Expr], bool)>, Infallible> {
        Ok(literal_sequence_elements(expression, promote))
    }

    fn sequence(
        &self,
        values: &'expr [ast::Expr],
        promote: bool,
    ) -> Result<Tuple<T, V>, Infallible> {
        sequence_from_literal_elements_sync(values, promote, LiteralSequenceFacts, self)
    }

    fn iterable_length(&self, expression: &ast::Expr) -> Result<Option<usize>, Infallible> {
        Ok(literal_iterable_length(expression))
    }

    fn spread(
        &self,
        expression: &'expr ast::Expr,
        promote: bool,
        known_length: Option<usize>,
    ) -> Result<Tuple<T, V>, Infallible> {
        Ok((self.spread)(expression, promote, known_length))
    }

    fn concat(
        &self,
        builder: &mut TupleBuilder<T, V>,
        unpacked: &Tuple<T, V>,
    ) -> Result<(), Infallible> {
        let previous = std::mem::replace(builder, TupleBuilder::with_capacity(0));
        *builder = (self.concat)(previous, unpacked);
        Ok(())
    }

    fn element(&self, expression: &'expr ast::Expr, promote: bool) -> Result<T, Infallible> {
        Ok((self.element)(expression, promote))
    }

    fn push(&self, builder: &mut TupleBuilder<T, V>, element: T) -> Result<(), Infallible> {
        builder.push(element);
        Ok(())
    }

    fn build(&self, builder: TupleBuilder<T, V>) -> Result<Tuple<T, V>, Infallible> {
        Ok(builder.build())
    }
}
