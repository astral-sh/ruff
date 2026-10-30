//! Provides collections for typed indices and Salsa IDs.
//!
//! Inspired by [rustc_index](https://github.com/rust-lang/rust/blob/master/compiler/rustc_index/src/lib.rs).

mod frozen;
mod idx;
#[cfg(feature = "salsa")]
mod salsa_id_set;
mod slice;
mod vec;

pub use frozen::FrozenIndexVec;
pub use idx::Idx;
pub use ruff_macros::newtype_index;
#[cfg(feature = "salsa")]
pub use salsa_id_set::{ImmutableSalsaIdSet, SalsaIdSet};
pub use slice::IndexSlice;
pub use vec::IndexVec;
