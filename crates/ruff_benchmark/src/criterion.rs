//! This module re-exports the criterion API but picks the right backend depending on whether
//! the benchmarks are built to run locally or with codspeed

#[cfg(not(codspeed))]
pub use criterion::*;

#[cfg(not(codspeed))]
pub type BenchmarkGroup<'a> = criterion::BenchmarkGroup<'a, measurement::WallTime>;

#[cfg(codspeed)]
pub use codspeed_criterion_compat::*;
