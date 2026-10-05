use std::marker::PhantomData;
use std::ptr;
use std::ptr::NonNull;

use crate::function::Configuration;
use crate::function::memo::Memo;
use crate::sync::atomic::{AtomicPtr, Ordering};

#[cfg(test)]
mod tests;

#[cfg(all(test, not(feature = "shuttle")))]
pub(crate) use tests::{Allocations, allocations as observe_allocations};

/// Stores the list of memos that have been deleted so they can be freed
/// once the next revision starts. See the comment on the field
/// `deleted_entries` of [`IngredientImpl`](super::IngredientImpl) for more details.
pub(super) struct DeletedEntries<C: Configuration> {
    memos: boxcar::Vec<RetirementEntry<C>>,
}

impl<C: Configuration> Default for DeletedEntries<C> {
    fn default() -> Self {
        Self {
            memos: Default::default(),
        }
    }
}

impl<C: Configuration> DeletedEntries<C> {
    /// # Safety
    ///
    /// The memo must be valid and safe to free when the `DeletedEntries` list is cleared or dropped.
    pub(super) unsafe fn push(&self, memo: NonNull<Memo<C>>) {
        self.memos.push(RetirementEntry {
            memo: AtomicPtr::new(memo.as_ptr()),
            owned: PhantomData,
        });
    }

    /// Allocates a stable retirement slot before a table replacement can become visible.
    pub(super) fn prepare(&self) -> PreparedRetirement<'_, C> {
        let index = self.memos.push(RetirementEntry {
            memo: AtomicPtr::new(ptr::null_mut()),
            owned: PhantomData,
        });
        PreparedRetirement {
            entry: &self.memos[index],
        }
    }

    pub(super) const fn entry_size() -> usize {
        size_of::<RetirementEntry<C>>()
    }

    /// Free all deleted memos, keeping the list available for reuse.
    pub(super) fn clear(&mut self) {
        self.memos.clear();
    }
}

/// A null entry owns no allocation. Assigned entries own replaced memos until revision cleanup.
struct RetirementEntry<C: Configuration> {
    memo: AtomicPtr<Memo<C>>,
    owned: PhantomData<Box<Memo<C>>>,
}

/// Consuming this token assigns exactly one retired allocation to its stable slot.
pub(super) struct PreparedRetirement<'a, C: Configuration> {
    entry: &'a RetirementEntry<C>,
}

impl<C: Configuration> PreparedRetirement<'_, C> {
    /// # Safety
    /// The replaced memo must remain valid until this list is cleared and must not be owned by
    /// another retirement entry or the memo table. A null replacement leaves the slot unused.
    pub(super) unsafe fn finish(self, replaced: Option<NonNull<Memo<C>>>) {
        if let Some(replaced) = replaced {
            self.entry.memo.store(replaced.as_ptr(), Ordering::Relaxed);
        }
    }
}

impl<C: Configuration> Drop for RetirementEntry<C> {
    fn drop(&mut self) {
        if let Some(memo) = NonNull::new(*self.memo.get_mut()) {
            // SAFETY: `push` or the unique prepared token transferred ownership of this Box.
            unsafe { drop(Box::from_raw(memo.as_ptr())) };
        }
    }
}
