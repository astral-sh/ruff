//! Insertion-ordered sets with inline storage for small collections.

pub mod indexed;
pub mod order;

pub use indexed::SmallIndexSet;
pub use order::SmallOrderSet;
