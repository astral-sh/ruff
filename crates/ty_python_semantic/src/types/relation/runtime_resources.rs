//! Stable call resources with an explicitly supplied capacity for bounded runtime controls.

use std::alloc::Layout;
use std::cell::{Cell, OnceCell};
use std::future::Future;
use std::num::NonZeroUsize;
use std::ops::Deref;

use salsa::attempt_probe::Incomplete;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use typed_arena::Arena;

use super::resources::RelationOwners;
use crate::types::constraints::{
    ConstraintSetBuilder, OwnedConstraintSet, OwnedConstraintSetQuery,
};
use crate::types::{ApplyTypeMappingVisitor, MaterializationEquivalenceVisitor};
use crate::{Program, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(in crate::types) struct CallResourceCapacity {
    pub(in crate::types) calls: NonZeroUsize,
}

enum ConstructionWork {
    Environment,
    Builder,
    RelationOwners,
    MappingVisitor,
}

impl ConstructionWork {
    fn units(self) -> usize {
        match self {
            Self::Environment | Self::Builder | Self::RelationOwners | Self::MappingVisitor => 1,
        }
    }
}

/// Only the first chunk is usable. General arena growth needs separate admission for its chunk
/// list and replacement capacity, so reaching the supplied limit refuses before `alloc` can spill.
struct FixedStorage<T> {
    limit: NonZeroUsize,
    arena: OnceCell<Arena<T>>,
    initialized: Cell<usize>,
    observed_capacity: Cell<Option<usize>>,
}

impl<T> FixedStorage<T> {
    fn with_capacity(capacity: CallResourceCapacity) -> Self {
        Self {
            limit: capacity.calls,
            arena: OnceCell::new(),
            initialized: Cell::new(0),
            observed_capacity: Cell::new(None),
        }
    }

    fn check_capacity(&self) -> RunResult<()> {
        if self.initialized.get() >= self.limit.get() {
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }

    // These callers admit any constructor allocations separately and make no database or user
    // callouts.
    // No arena borrow crosses admission; after the final check construction and insertion cannot
    // reenter this storage. The returned borrow belongs to the pool, independently of the endpoint.
    fn allocate_admitted(
        &self,
        endpoint: &TaskEndpoint<'_, '_>,
        work: ConstructionWork,
        make: impl FnOnce() -> T,
    ) -> RunResult<&T> {
        let requested_bytes = self
            .limit
            .get()
            .checked_mul(size_of::<T>())
            .ok_or(RunError::Contract("call resource capacity size overflow"))?;
        endpoint.admit_work(work.units())?;
        self.check_capacity()?;
        if self.arena.get().is_none() {
            endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
        }
        self.check_capacity()?;
        let arena = self.arena.get_or_init(|| {
            let arena = Arena::with_capacity(self.limit.get());
            self.observed_capacity
                .set(Some(arena.uninitialized_array().len()));
            arena
        });
        if arena.uninitialized_array().len() == 0 {
            return Err(RunError::Contract(
                "call resource backing capacity exhausted",
            ));
        }
        let value = arena.alloc(make());
        self.initialized.set(self.initialized.get() + 1);
        Ok(value)
    }
}

pub(in crate::types) struct CallEnvironments<'db> {
    storage: FixedStorage<ProgramEnvironment<'db>>,
}

impl<'db> CallEnvironments<'db> {
    pub(in crate::types) fn with_capacity(capacity: CallResourceCapacity) -> Self {
        Self {
            storage: FixedStorage::with_capacity(capacity),
        }
    }

    pub(in crate::types) fn allocate<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
    ) -> impl Future<Output = &'pool ProgramEnvironment<'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.storage
                .allocate_admitted(endpoint, ConstructionWork::Environment, || {
                    ProgramEnvironment::from_program(program)
                })
        })
    }
}

pub(in crate::types) struct CallBuilders<'db> {
    storage: FixedStorage<ConstraintSetBuilder<'db>>,
    // Query views own their shared arenas separately from private producer builders. Each store
    // has the supplied capacity, and allocating a view never replaces a borrowed builder.
    query_views: FixedStorage<OwnedConstraintSetQuery<'db>>,
}

impl<'db> CallBuilders<'db> {
    pub(in crate::types) fn with_capacity(capacity: CallResourceCapacity) -> Self {
        Self {
            storage: FixedStorage::with_capacity(capacity),
            query_views: FixedStorage::with_capacity(capacity),
        }
    }

    pub(in crate::types) fn allocate<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
    ) -> impl Future<Output = &'pool ConstraintSetBuilder<'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.storage.allocate_admitted(
                endpoint,
                ConstructionWork::Builder,
                ConstraintSetBuilder::new,
            )
        })
    }

    pub(in crate::types) fn allocate_owned_query<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        owned: &'call OwnedConstraintSet<'db>,
    ) -> impl Future<Output = &'pool OwnedConstraintSetQuery<'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.query_views
                .allocate_admitted(endpoint, ConstructionWork::Builder, || owned.query_view())
        })
    }
}

pub(in crate::types) struct CallRelationOwners<'env, 'c, 'db> {
    storage: FixedStorage<RelationOwners<'env, 'c, 'db>>,
}

impl<'env, 'c, 'db> CallRelationOwners<'env, 'c, 'db> {
    pub(in crate::types) fn with_capacity(capacity: CallResourceCapacity) -> Self {
        Self {
            storage: FixedStorage::with_capacity(capacity),
        }
    }

    pub(in crate::types) fn allocate<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        env: &'env ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> impl Future<Output = &'pool RelationOwners<'env, 'c, 'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.storage
                .allocate_admitted(endpoint, ConstructionWork::RelationOwners, || {
                    RelationOwners::new(env, constraints)
                })
        })
    }
}

pub(in crate::types) struct CallMappingVisitors<'env, 'db> {
    storage: FixedStorage<ApplyTypeMappingVisitor<'env, 'db>>,
}

impl<'env, 'db> CallMappingVisitors<'env, 'db> {
    pub(in crate::types) fn with_capacity(capacity: CallResourceCapacity) -> Self {
        Self {
            storage: FixedStorage::with_capacity(capacity),
        }
    }

    pub(in crate::types) fn allocate<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        env: &'env ProgramEnvironment<'db>,
    ) -> impl Future<Output = &'pool ApplyTypeMappingVisitor<'env, 'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.storage
                .allocate_admitted(endpoint, ConstructionWork::MappingVisitor, || {
                    ApplyTypeMappingVisitor::new(env)
                })
        })
    }

    pub(in crate::types) fn allocate_materialization_root<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        original: &'call ApplyTypeMappingVisitor<'env, 'db>,
    ) -> impl Future<Output = &'pool ApplyTypeMappingVisitor<'env, 'db>> + 'call
    where
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(move || {
            self.storage.check_capacity()?;
            if original.materialization_equivalence.get().is_none() {
                let requested_bytes = Self::materialization_guard_bytes()?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
            }
            self.storage
                .allocate_admitted(endpoint, ConstructionWork::MappingVisitor, || {
                    original.for_new_materialization_root()
                })
        })
    }

    fn materialization_guard_bytes() -> RunResult<usize> {
        // Rc retains strong and weak counts before its value. Include padding both before the
        // detector and at the end of the allocation; its caches start without heap storage.
        Layout::new::<[Cell<usize>; 2]>()
            .extend(Layout::new::<
                <MaterializationEquivalenceVisitor<'db> as Deref>::Target,
            >())
            .map(|(layout, _)| layout.pad_to_align().size())
            .map_err(|_| RunError::Contract("materialization guard size overflow"))
    }
}
