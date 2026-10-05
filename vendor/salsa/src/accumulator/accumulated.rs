use std::any::Any;
use std::fmt::Debug;

use crate::accumulator::Accumulator;

#[derive(Clone, Debug)]
pub(crate) struct Accumulated<A: Accumulator> {
    values: Vec<A>,
}

pub(crate) trait AnyAccumulated: Any + Send + Sync {
    fn as_dyn_any(&self) -> &dyn Any;
    fn as_dyn_any_mut(&mut self) -> &mut dyn Any;
}

impl<A: Accumulator> Accumulated<A> {
    pub fn push(&mut self, value: A) {
        self.values.push(value);
    }

    pub fn extend_with_accumulated<'slf>(&'slf self, values: &mut Vec<&'slf A>) {
        values.extend(&self.values);
    }
}

impl<A: Accumulator> Default for Accumulated<A> {
    fn default() -> Self {
        Self {
            values: Default::default(),
        }
    }
}

impl<A> AnyAccumulated for Accumulated<A>
where
    A: Accumulator,
{
    fn as_dyn_any(&self) -> &dyn Any {
        self
    }

    fn as_dyn_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn AnyAccumulated {
    pub fn accumulate<A: Accumulator>(&mut self, value: A) {
        self.as_dyn_any_mut()
            .downcast_mut::<Accumulated<A>>()
            .unwrap()
            .push(value);
    }
}
