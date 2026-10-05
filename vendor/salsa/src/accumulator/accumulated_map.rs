use std::ops;

use rustc_hash::FxBuildHasher;

use crate::IngredientIndex;
use crate::accumulator::accumulated::Accumulated;
use crate::accumulator::{Accumulator, AnyAccumulated};
use crate::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct AccumulatedMap {
    map: hashbrown::HashMap<IngredientIndex, Box<dyn AnyAccumulated>, FxBuildHasher>,
}

impl std::fmt::Debug for AccumulatedMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccumulatedMap")
            .field("map", &self.map.keys())
            .finish()
    }
}

impl AccumulatedMap {
    pub fn accumulate<A: Accumulator>(&mut self, index: IngredientIndex, value: A) {
        self.map
            .entry(index)
            .or_insert_with(|| <Box<Accumulated<A>>>::default())
            .accumulate(value);
    }

    pub fn extend_with_accumulated<'slf, A: Accumulator>(
        &'slf self,
        index: IngredientIndex,
        output: &mut Vec<&'slf A>,
    ) {
        let Some(a) = self.map.get(&index) else {
            return;
        };

        a.as_dyn_any()
            .downcast_ref::<Accumulated<A>>()
            .unwrap()
            .extend_with_accumulated(output);
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear()
    }

    pub fn allocation_size(&self) -> usize {
        self.map.allocation_size()
    }
}

/// Tracks whether any input read during a query's execution has any accumulated values.
///
/// Knowning whether any input has accumulated values makes aggregating the accumulated values
/// cheaper because we can skip over entire subtrees.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum InputAccumulatedValues {
    /// The query nor any of its inputs have any accumulated values.
    #[default]
    Empty,

    /// The query or any of its inputs have at least one accumulated value.
    Any,
}

impl InputAccumulatedValues {
    pub const fn is_any(self) -> bool {
        matches!(self, Self::Any)
    }

    pub const fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn or_else(self, other: impl FnOnce() -> Self) -> Self {
        if self.is_any() { Self::Any } else { other() }
    }
}

impl ops::BitOr for InputAccumulatedValues {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Empty, Self::Empty) => Self::Empty,
        }
    }
}

impl ops::BitOrAssign for InputAccumulatedValues {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[derive(Debug, Default)]
pub struct AtomicInputAccumulatedValues(AtomicBool);

impl Clone for AtomicInputAccumulatedValues {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.0.load(Ordering::Relaxed)))
    }
}

impl AtomicInputAccumulatedValues {
    pub(crate) fn new(accumulated_inputs: InputAccumulatedValues) -> Self {
        Self(AtomicBool::new(accumulated_inputs.is_any()))
    }

    pub(crate) fn store(&self, accumulated: InputAccumulatedValues) {
        self.0.store(accumulated.is_any(), Ordering::Release);
    }

    pub(crate) fn load(&self) -> InputAccumulatedValues {
        if self.0.load(Ordering::Acquire) {
            InputAccumulatedValues::Any
        } else {
            InputAccumulatedValues::Empty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_input_accumulated_values() {
        let val = AtomicInputAccumulatedValues::new(InputAccumulatedValues::Empty);
        assert_eq!(val.load(), InputAccumulatedValues::Empty);
        val.store(InputAccumulatedValues::Any);
        assert_eq!(val.load(), InputAccumulatedValues::Any);
        let val = AtomicInputAccumulatedValues::new(InputAccumulatedValues::Any);
        assert_eq!(val.load(), InputAccumulatedValues::Any);
        val.store(InputAccumulatedValues::Empty);
        assert_eq!(val.load(), InputAccumulatedValues::Empty);
    }
}
