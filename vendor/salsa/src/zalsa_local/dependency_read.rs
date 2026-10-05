use super::ZalsaLocal;
#[cfg(feature = "accumulator")]
use crate::accumulator::accumulated_map::InputAccumulatedValues;
use crate::active_query::ActiveQuery;
use crate::active_query::read_storage::{ReadStoragePlan, ReadStorageWork};
use crate::cycle::CycleHeads;
use crate::{DatabaseKeyIndex, Durability, Revision};

/// The canonical metadata of a read, retained while its recipient's storage is admitted.
pub(crate) enum DependencyRead<'a> {
    Full {
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
        cycle_heads: &'a CycleHeads,
        #[cfg(feature = "accumulator")]
        accumulated_inputs: InputAccumulatedValues,
    },
    Simple {
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
    },
    Revision(Revision),
}

impl DependencyRead<'_> {
    fn storage_requirements(&self) -> (bool, usize) {
        match self {
            Self::Full {
                durability,
                cycle_heads,
                #[cfg(feature = "accumulator")]
                accumulated_inputs,
                ..
            } => (
                ActiveQuery::read_records_input(
                    *durability,
                    cycle_heads,
                    #[cfg(feature = "accumulator")]
                    *accumulated_inputs,
                ),
                cycle_heads.storage_len(),
            ),
            Self::Simple { durability, .. } => (
                cfg!(feature = "persistence") || *durability != Durability::NEVER_CHANGE,
                0,
            ),
            Self::Revision(_) => (false, 0),
        }
    }

    #[inline]
    fn apply(&self, query: &mut ActiveQuery) {
        match self {
            Self::Full {
                input,
                durability,
                changed_at,
                cycle_heads,
                #[cfg(feature = "accumulator")]
                accumulated_inputs,
            } => query.add_read_observed(
                *input,
                *durability,
                *changed_at,
                cycle_heads,
                #[cfg(feature = "accumulator")]
                *accumulated_inputs,
            ),
            Self::Simple {
                input,
                durability,
                changed_at,
            } => query.add_read_simple(*input, *durability, *changed_at),
            Self::Revision(changed_at) => query.add_changed_at(*changed_at),
        }
    }
}

/// A storage quote has authority only while its enclosing runtime owner remains current.
/// It is consumed before that owner's callback can return the selected value.
pub(crate) struct PreparedDependencyRead {
    recipient: Option<(DatabaseKeyIndex, ReadStoragePlan)>,
    depth: usize,
    requirements: (bool, usize),
}

impl PreparedDependencyRead {
    pub(crate) fn work(&self) -> ReadStorageWork {
        self.recipient.as_ref().map_or(
            ReadStorageWork {
                units: 1,
                requested_bytes: 0,
            },
            |(_, plan)| plan.work(),
        )
    }
}

impl ZalsaLocal {
    pub(crate) fn prepare_dependency_read(
        &self,
        read: &DependencyRead<'_>,
    ) -> Result<PreparedDependencyRead, &'static str> {
        let stack = self
            .query_stack
            .try_borrow()
            .map_err(|_| "dependency read recipient is borrowed")?;
        let requirements = read.storage_requirements();
        let recipient = stack
            .last()
            .map(|query| {
                query
                    .read_storage_footprint()
                    .prepare(requirements.0, requirements.1)
                    .map(|plan| (query.database_key_index, plan))
                    .ok_or("dependency read storage size overflow")
            })
            .transpose()?;
        Ok(PreparedDependencyRead {
            recipient,
            depth: stack.len(),
            requirements,
        })
    }

    /// Rechecks and reserves without calling observers or retaining a stack borrow on return.
    /// The caller applies the same read immediately, without an intervening callout.
    pub(crate) fn reserve_dependency_read(
        &self,
        prepared: PreparedDependencyRead,
        read: &DependencyRead<'_>,
    ) -> Result<bool, &'static str> {
        let mut stack = self
            .query_stack
            .try_borrow_mut()
            .map_err(|_| "dependency read recipient is borrowed")?;
        if stack.len() != prepared.depth
            || stack.last().map(|query| query.database_key_index)
                != prepared.recipient.as_ref().map(|(key, _)| *key)
        {
            return Err("dependency read changed its recipient");
        }
        if read.storage_requirements() != prepared.requirements {
            return Ok(false);
        }
        match (stack.last_mut(), prepared.recipient) {
            (Some(query), Some((_, plan))) => Ok(query.reserve_read_storage(&plan)),
            (None, None) => Ok(true),
            _ => Err("dependency read lost its recipient"),
        }
    }

    #[inline(always)]
    pub(crate) fn apply_dependency_read(&self, read: &DependencyRead<'_>) {
        // SAFETY: Captured read metadata and collection updates cannot reenter the query stack.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                if let Some(query) = stack.last_mut() {
                    read.apply(query);
                }
            });
        }
    }
}
