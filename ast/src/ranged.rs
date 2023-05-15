use crate::text_size::{TextRange, TextSize};
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;

pub use crate::builtin::*;
use crate::Stmt;

pub trait Ranged {
    fn range(&self) -> TextRange;

    fn start(&self) -> TextSize {
        self.range().start()
    }

    fn end(&self) -> TextSize {
        self.range().end()
    }
}

pub type Suite<R = TextRange> = Vec<Stmt<R>>;

#[cfg(feature = "all-nodes-with-ranges")]
pub type OptionalRange<R> = R;

#[cfg(not(feature = "all-nodes-with-ranges"))]
pub type OptionalRange<R> = EmptyRange<R>;

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct EmptyRange<R> {
    phantom: PhantomData<R>,
}

impl<R> EmptyRange<R> {
    #[inline(always)]
    pub fn new(_start: TextSize, _end: TextSize) -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<R> Display for EmptyRange<R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("()")
    }
}

impl<R> Debug for EmptyRange<R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<R> Default for EmptyRange<R> {
    fn default() -> Self {
        EmptyRange {
            phantom: PhantomData,
        }
    }
}

include!("gen/ranged.rs");
