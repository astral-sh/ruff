use crate::text_size::{TextRange, TextSize};

pub use crate::builtin::*;

pub trait Ranged {
    fn range(&self) -> TextRange;

    fn start(&self) -> TextSize {
        self.range().start()
    }

    fn end(&self) -> TextSize {
        self.range().end()
    }
}

impl Ranged for TextRange {
    fn range(&self) -> TextRange {
        *self
    }
}

impl<T> Ranged for &T
where
    T: Ranged,
{
    fn range(&self) -> TextRange {
        T::range(self)
    }
}

include!("gen/ranged.rs");
