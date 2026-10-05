//! Admitted arena growth for relation owners retained until their storage is dropped.

use std::alloc::Layout;
use std::cell::{Cell, OnceCell};

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use typed_arena::Arena;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct StorageState {
    current_capacity: Option<usize>,
    initialized: usize,
    previous_chunks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    state: StorageState,
    remaining: usize,
}

struct Quote {
    work: usize,
    payload_bytes: usize,
    chunk_list_bytes: usize,
    initialized: usize,
    previous_chunks: usize,
}

impl Quote {
    fn new<T>(snapshot: Snapshot, value_work: usize) -> RunResult<Self> {
        if size_of::<T>() == 0 {
            return Err(RunError::Contract(
                "stable relation storage requires nonzero-sized values",
            ));
        }
        // Insertion, bookkeeping and the arena's eventual visit to this initialized value.
        // The caller supplies the construction and destruction work of the value itself.
        let mut work = checked(value_work.checked_add(4))?;
        let mut previous_chunks = snapshot.state.previous_chunks;
        let mut chunk_list_bytes = 0;
        let payload_bytes = match snapshot.state.current_capacity {
            None => {
                // Creating and eventually disposing of the current chunk and its header.
                work = checked(work.checked_add(4))?;
                array_bytes::<T>(1)?
            }
            Some(capacity) if snapshot.remaining == 0 => {
                let capacity = checked(capacity.checked_mul(2))?;
                previous_chunks = checked(previous_chunks.checked_add(1))?;
                // typed-arena 2.0.2 doubles its current chunk and pushes the old Vec onto a
                // private Vec<Vec<T>>. At a list reallocation its old capacity equals its
                // length. The pinned standard library requests at most this many headers;
                // quote that request on every spill because the list capacity is private.
                let headers = checked(snapshot.state.previous_chunks.checked_mul(2))?
                    .max(previous_chunks)
                    .max(4);
                chunk_list_bytes = array_bytes::<Vec<T>>(headers)?;
                // Existing values stay in place. Only previous chunk headers can be copied.
                // Include the new chunk and chunk-list update/retirement as well.
                work = checked(
                    work.checked_add(snapshot.state.previous_chunks)
                        .and_then(|work| work.checked_add(8)),
                )?;
                array_bytes::<T>(capacity)?
            }
            Some(_) => 0,
        };
        Ok(Self {
            work,
            payload_bytes,
            chunk_list_bytes,
            initialized: checked(snapshot.state.initialized.checked_add(1))?,
            previous_chunks,
        })
    }
}

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract(
        "stable relation storage quotation overflow",
    ))
}

fn array_bytes<T>(capacity: usize) -> RunResult<usize> {
    Layout::array::<T>(capacity)
        .map(|layout| layout.size())
        .map_err(|_| RunError::Contract("stable relation storage allocation layout overflow"))
}

/// The storage outlives its execution run and all references returned by short local calls.
/// Values, including completed relation roots, remain alive until this storage is dropped.
pub(in crate::types) struct StableStorage<T> {
    arena: OnceCell<Arena<T>>,
    state: Cell<StorageState>,
    allocating: Cell<bool>,
}

impl<T> StableStorage<T> {
    pub(in crate::types) fn new() -> Self {
        Self {
            arena: OnceCell::new(),
            state: Cell::new(StorageState::default()),
            allocating: Cell::new(false),
        }
    }

    #[cfg(test)]
    pub(in crate::types) fn retained_payload(&self) -> Option<(usize, usize)> {
        let snapshot = self.snapshot();
        let capacity = snapshot.state.initialized.checked_add(snapshot.remaining)?;
        Some((
            snapshot.state.initialized,
            capacity.checked_mul(size_of::<T>())?,
        ))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.state.get(),
            remaining: self
                .arena
                .get()
                .map_or(0, |arena| arena.uninitialized_array().len()),
        }
    }

    fn check_snapshot(&self, expected: Snapshot) -> RunResult<()> {
        if self.snapshot() != expected {
            return Err(RunError::Contract(
                "stable relation storage changed during allocation",
            ));
        }
        Ok(())
    }

    /// Allocates a value whose address remains stable until this storage is dropped.
    /// Call inside `endpoint.local_call`.
    ///
    /// Each value admits at least `size_of::<T>()` requested bytes, even when reusing spare
    /// arena capacity from an earlier evaluation. This logical allocation quotation covers
    /// fixed representation initialization and copying; arena growth can require more bytes.
    /// `work` covers other construction, failed-construction cleanup and eventual value
    /// destruction; admit constructor-owned allocations separately.
    /// Later mutations must also admit any additional destruction work they introduce.
    ///
    /// The constructor performs no database calls or scheduling, and its captures have passive
    /// or separately admitted cleanup. Recursive allocation into this storage is rejected,
    /// including from admission observers. No arena borrow crosses either callback. Quotations
    /// cover requested payload and structural work, not allocator latency or excess backing.
    /// Zero-sized values are unsupported.
    pub(in crate::types) fn allocate_admitted(
        &self,
        endpoint: &TaskEndpoint<'_, '_>,
        work: usize,
        make: impl FnOnce() -> T,
    ) -> RunResult<&T> {
        if self.allocating.replace(true) {
            return Err(RunError::Contract(
                "recursive stable relation storage allocation",
            ));
        }
        let _allocation = AllocationGuard(&self.allocating);
        let before = self.snapshot();
        let quote = Quote::new::<T>(before, work)?;
        endpoint.admit_work(quote.work)?;
        self.check_snapshot(before)?;
        for requested_bytes in [
            quote.payload_bytes.max(size_of::<T>()),
            quote.chunk_list_bytes,
        ] {
            if requested_bytes != 0 {
                endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                self.check_snapshot(before)?;
            }
        }

        {
            let arena = self.arena.get_or_init(|| Arena::with_capacity(1));
            arena.reserve_extend(1);
            if quote.payload_bytes != 0 {
                // Record actual capacity before construction, which can unwind. A failed
                // constructor leaves this reserved chunk available for a later attempt.
                self.state.set(StorageState {
                    current_capacity: Some(arena.uninitialized_array().len()),
                    previous_chunks: quote.previous_chunks,
                    ..before.state
                });
            }
        }
        let reserved = self.snapshot();
        if reserved.remaining == 0 {
            return Err(RunError::Contract(
                "stable relation storage reservation is empty",
            ));
        }
        let value = make();
        self.check_snapshot(reserved)?;
        let arena = self.arena.get().ok_or(RunError::Contract(
            "stable relation storage lost its reserved arena",
        ))?;
        let value = arena.alloc(value);
        self.state.set(StorageState {
            initialized: quote.initialized,
            ..reserved.state
        });
        Ok(value)
    }
}

struct AllocationGuard<'a>(&'a Cell<bool>);

impl Drop for AllocationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
