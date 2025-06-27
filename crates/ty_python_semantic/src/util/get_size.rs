use std::sync::Arc;

use get_size2::GetSize;

/// By default, `Arc<T>: GetSize` requires `T: 'static` to enable tracking references
/// of the `Arc` and avoid double-counting. This method opts out of that behavior and
/// removes the `'static` requirement.
///
/// This method will just return the heap-size of the inner `T`.
pub(crate) fn untracked_arc_size<T>(arc: &Arc<T>) -> usize
where
    T: GetSize,
{
    T::get_heap_size(&**arc)
}
