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

include!("gen/ranged.rs");
