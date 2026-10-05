//! Fixed-target unpacking preserves the source's fixed ends and combines its variable segment.

use std::cmp::Ordering;
use std::convert::Infallible;

use super::{FixedLengthTuple, ResizeTupleError, Tuple, VariableLengthTuple};

#[derive(Debug)]
pub(in crate::types) struct FixedUnpackFacts;

#[derive(Debug)]
enum FixedUnpackSource<'a, T, V> {
    Fixed(&'a FixedLengthTuple<T>),
    Variable(&'a VariableLengthTuple<T, V>),
}

/// Supplies ordinary unpacking's segment expansion and element-combination callbacks.
#[derive(Debug)]
pub(super) struct OrdinaryFixedUnpackEffects<'a, Elements, Combine> {
    pub(super) variable_elements: &'a Elements,
    pub(super) combine: &'a Combine,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousFixedUnpackEffects)]
    pub(in crate::types) trait FixedUnpackEffects<T, V> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn clone_fixed(&self, values: &FixedLengthTuple<T>) -> Result<FixedLengthTuple<T>, Self::Error>;
        #[operation(child)]
        async fn variable_elements(&self, segment: &V) -> Result<Vec<T>, Self::Error>;
        #[operation(child)]
        async fn combine(&self, elements: &[T]) -> Result<T, Self::Error>;
        #[operation(local)]
        async fn expand_fixed(&self, values: &VariableLengthTuple<T, V>, variable: T, count: usize) -> Result<FixedLengthTuple<T>, Self::Error>;
    }

    #[finite_capability]
    impl FixedUnpackFacts {
        fn source<'a, T, V>(&self, tuple: &'a Tuple<T, V>) -> FixedUnpackSource<'a, T, V> {
            match tuple {
                Tuple::Fixed(values) => FixedUnpackSource::Fixed(values),
                Tuple::Variable(values) => FixedUnpackSource::Variable(values),
            }
        }

        fn compare_length<T>(&self, values: &FixedLengthTuple<T>, length: usize) -> Ordering {
            values.len().cmp(&length)
        }

        fn variable_count<T, V>(&self, values: &VariableLengthTuple<T, V>, length: usize) -> Option<usize> {
            length.checked_sub(values.len().minimum())
        }

        fn variable<'a, T, V>(&self, values: &'a VariableLengthTuple<T, V>) -> &'a V {
            &values.variable_segment
        }
    }

    /// Unpacks a tuple into exactly `length` positions, reporting incompatible source lengths.
    #[synchronous(unpack_fixed_sync)]
    #[capabilities(effects = FixedUnpackEffects, facts = FixedUnpackFacts)]
    #[passive_values(Err, ResizeTupleError::TooFewValues, ResizeTupleError::TooManyValues)]
    pub(in crate::types) async fn unpack_fixed_with<T, V, E: FixedUnpackEffects<T, V>>(
        source: &Tuple<T, V>, length: usize, facts: FixedUnpackFacts, effects: &E,
    ) -> Result<Result<FixedLengthTuple<T>, ResizeTupleError>, E::Error> {
        effects.checkpoint().await?;
        match facts.source(source) {
            // Both lengths are fixed, as in `a, b = (1, "two")`; every target needs one value.
            FixedUnpackSource::Fixed(values) => match facts.compare_length(values, length) {
                // `a, b = (1,)` leaves a target without a value.
                Ordering::Less => Ok(Err(ResizeTupleError::TooFewValues)),
                // `a, b = (1, 2, 3)` leaves a value without a target.
                Ordering::Greater => Ok(Err(ResizeTupleError::TooManyValues)),
                // `a, b = (1, "two")` pairs both targets with their corresponding values.
                Ordering::Equal => Ok(Ok(effects.clone_fixed(values).await?)),
            },
            // The fixed ends supply `a` and `d`; a successful unpacking must take both
            // `b` and `c` from `items`:
            //
            // ```python
            // def example(items: list[str]):
            //     a, b, c, d = (1, *items, 2)
            // ```
            //
            // The source's length is unknown, but its fixed elements impose a minimum.
            // With `a, b = (1, *items, 2, 3)` instead, even an empty `items` leaves too many values.
            FixedUnpackSource::Variable(values) => {
                let Some(count) = facts.variable_count(values, length) else {
                    return Ok(Err(ResizeTupleError::TooManyValues));
                };
                let variable = {
                    let elements = effects.variable_elements(facts.variable(values)).await?;
                    effects.combine(&elements).await?
                };
                Ok(Ok(effects.expand_fixed(values, variable, count).await?))
            }
        }
    }
}

/// Copies the fixed ends around `count` occurrences of the combined variable element.
pub(in crate::types) fn expand_fixed<T: Clone, V>(
    values: &VariableLengthTuple<T, V>,
    variable: T,
    count: usize,
) -> FixedLengthTuple<T> {
    FixedLengthTuple::from_elements(
        values
            .prefix_elements()
            .iter()
            .cloned()
            .chain(std::iter::repeat_n(variable, count))
            .chain(values.suffix_elements().iter().cloned()),
    )
}

impl<T: Clone, V, Elements, Combine> SynchronousFixedUnpackEffects<T, V>
    for OrdinaryFixedUnpackEffects<'_, Elements, Combine>
where
    Elements: Fn(&V) -> Vec<T>,
    Combine: Fn(&[T]) -> T,
{
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn clone_fixed(&self, values: &FixedLengthTuple<T>) -> Result<FixedLengthTuple<T>, Infallible> {
        Ok(values.clone())
    }

    fn variable_elements(&self, segment: &V) -> Result<Vec<T>, Infallible> {
        Ok((self.variable_elements)(segment))
    }

    fn combine(&self, elements: &[T]) -> Result<T, Infallible> {
        Ok((self.combine)(elements))
    }

    fn expand_fixed(
        &self,
        values: &VariableLengthTuple<T, V>,
        variable: T,
        count: usize,
    ) -> Result<FixedLengthTuple<T>, Infallible> {
        Ok(expand_fixed(values, variable, count))
    }
}
