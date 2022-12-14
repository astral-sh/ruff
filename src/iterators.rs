#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;

/// Shim that calls `par_iter` except for wasm because there's no wasm support
/// in rayon yet (there is a shim to be used for the web, but it requires js
/// cooperation) Unfortunately, `ParallelIterator` does not implement `Iterator`
/// so the signatures diverge
#[cfg(not(target_family = "wasm"))]
pub fn par_iter<T: Sync>(iterable: &[T]) -> impl ParallelIterator<Item = &T> {
    iterable.par_iter()
}

#[cfg(target_family = "wasm")]
pub fn par_iter<T: Sync>(iterable: &[T]) -> impl Iterator<Item = &T> {
    iterable.iter()
}
