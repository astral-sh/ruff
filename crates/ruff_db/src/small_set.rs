//! Insertion-ordered sets with inline storage for small collections.

mod indexed;
mod order;

pub use indexed::SmallIndexSet;
pub use order::SmallOrderSet;
