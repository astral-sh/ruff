//! Provides new-type wrappers for collections that are indexed by a [`Idx`] rather
//! than `usize`.
//!
//! Inspired by [rustc_index](https://github.com/rust-lang/rust/blob/master/compiler/rustc_index/src/lib.rs).

mod frozen;
mod idx;
mod slice;
mod vec;

pub use frozen::FrozenIndexVec;
pub use idx::Idx;
pub use ruff_macros::newtype_index;
pub use slice::IndexSlice;
pub use vec::IndexVec;
