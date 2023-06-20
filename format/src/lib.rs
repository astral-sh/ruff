#[cfg(feature = "malachite-bigint")]
pub use malachite_bigint as bigint;
#[cfg(all(feature = "num-bigint", not(feature = "malachite-bigint")))]
pub use num_bigint as bigint;

pub use crate::format::*;

pub mod cformat;
mod format;
