//! Admission of structural work and requested collection payloads.

use std::alloc::Layout;
use std::convert::Infallible;
use std::hash::Hash;

use rustc_hash::FxHashMap;
use smallvec::{Array, SmallVec};

use super::ConstraintId;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod attempt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TableKind {
    #[cfg(test)]
    Typevars,
    #[cfg(test)]
    NeverCache,
    #[cfg(test)]
    DepthCache,
    Constraints,
    Nodes,
    SourceOrders,
    And,
    Or,
    Negate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum AllocationKind {
    TypeWalkPending,
    TypeWalkSeen,
    TypeWalkActive,
    TypeWalkReceiverSeen,
    PathFrames,
    PathSequents,
    PathAssignments,
    PathPositiveIndices,
    PathNegativeIndices,
    PathFuelUndo,
    PathDiscovered,
    PathElaboratedPairs,
    PathSingleReplay,
    PathPairReplay,
    PathIndependentTypevars,
    PathDependentTypevars,
    PathQueue,
    PathNewAssignments,
    PathReplayIds,
    UniqueConstraintStack,
    UniqueConstraintSeen,
    UniqueConstraintOutput,
    SourceOrderScanStack,
    SourceOrderScanSeen,
    SourceOrderScanResult,
    #[cfg(test)]
    Typevars,
    #[cfg(test)]
    Constraints,
    #[cfg(test)]
    ConstraintSupports,
    Frames,
    SupportWords,
    Supports,
    Nodes,
    NodeSupports,
    SourceOrders,
    FoldAccumulator,
    Table(TableKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct GrowthPlan {
    pub(in crate::types) requested_capacity: usize,
    /// Bytes requested for logical payload slots. This excludes allocator and hash
    /// control metadata, and does not claim an upper bound on actual retained allocation.
    pub(in crate::types) requested_payload_bytes: usize,
    pub(in crate::types) relocation_units: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TddWork {
    TypeWalkAccess {
        units: usize,
    },
    Path {
        work: PathWork,
        units: usize,
        requested_payload_bytes: usize,
    },
    Advance,
    SourceOrderAdvance,
    SourceOrderCommit,
    #[cfg(any(test, feature = "experimental-analysis"))]
    CombinationAdvance,
    FoldAdvance,
    FoldCommit {
        removed: usize,
        appended: bool,
    },
    CoverageReduction,
    SupportWords {
        words: usize,
    },
    OverlayScan {
        slots: usize,
    },
    HashAccess {
        table: TableKind,
        /// Table-layout metadata; a logical access costs one progress unit.
        slots: usize,
    },
    Grow {
        allocation: AllocationKind,
        plan: GrowthPlan,
    },
    /// Supplements receiver-cursor growth with hash storage and prepaid backing cleanup.
    ReceiverSeenStorage {
        units: usize,
        requested_bytes: usize,
    },
    Commit,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl TddWork {
    pub(in crate::types) fn work_units(self) -> usize {
        match self {
            Self::TypeWalkAccess { units } => units,
            Self::Path { units, .. } => units,
            Self::ReceiverSeenStorage { units, .. } => units,
            Self::Advance
            | Self::Commit
            | Self::SourceOrderAdvance
            | Self::SourceOrderCommit
            | Self::FoldAdvance
            | Self::HashAccess { .. } => 1,
            // FoldAdvance covers fixed bookkeeping and a single append. This event also
            // accounts for the consumed suffix before the accepted accumulator is changed.
            #[cfg(any(test, feature = "experimental-analysis"))]
            Self::CombinationAdvance => 1,
            Self::FoldCommit { removed, .. } => removed,
            // At most 64 nontrivial coverage visits, each with at most five child checks, plus
            // the two initial calls and fixed local reductions.
            Self::CoverageReduction => 384,
            Self::SupportWords { words } => words,
            Self::OverlayScan { slots } => slots,
            Self::Grow { plan, .. } => plan.relocation_units,
        }
    }

    pub(in crate::types) fn requested_payload_bytes(self) -> usize {
        match self {
            Self::Grow { plan, .. } => plan.requested_payload_bytes,
            Self::Path {
                requested_payload_bytes,
                ..
            } => requested_payload_bytes,
            Self::ReceiverSeenStorage {
                requested_bytes, ..
            } => requested_bytes,
            _ => 0,
        }
    }
}

pub(in crate::types) trait TddControl {
    type Error;

    fn admit(&mut self, work: TddWork) -> Result<(), Self::Error>;

    #[inline]
    fn admit_hash_access(
        &mut self,
        table: TableKind,
        capacity: usize,
    ) -> Result<(), TddError<Self::Error>> {
        self.admit(TddWork::HashAccess {
            table,
            slots: hash_slots::<Self::Error>(capacity)?,
        })?;
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::types) enum TddError<E> {
    Refused(E),
    CapacityExhausted,
}

impl<E> From<E> for TddError<E> {
    fn from(error: E) -> Self {
        Self::Refused(error)
    }
}

pub(in crate::types) struct Unrestricted;

impl TddControl for Unrestricted {
    type Error = Infallible;

    #[inline]
    fn admit(&mut self, _: TddWork) -> Result<(), Infallible> {
        Ok(())
    }

    // Cache entries occupy at least eight bytes, so a representable allocation cannot overflow
    // the slot estimate. Skip accounting without an allowance; retain layout and ID checks.
    #[inline]
    fn admit_hash_access(&mut self, _: TableKind, _: usize) -> Result<(), TddError<Infallible>> {
        Ok(())
    }
}

#[inline]
pub(in crate::types) fn unrestricted<T>(result: Result<T, TddError<Infallible>>) -> T {
    match result {
        Ok(value) => value,
        Err(TddError::Refused(never)) => match never {},
        Err(TddError::CapacityExhausted) => panic!("constraint storage capacity exhausted"),
    }
}

// The pinned hashbrown backend fits an insert/clear-only table within this capacity multiple,
// including the smallest control group. Deletion tombstones invalidate that relationship.
// Callers use fixed-size cache keys; this does not bound arbitrary Hash/Eq implementations.
pub(in crate::types) fn hash_slots<E>(capacity: usize) -> Result<usize, TddError<E>> {
    capacity
        .checked_add(1)
        .and_then(|slots| slots.checked_mul(4))
        .and_then(|slots| slots.checked_add(32))
        .ok_or(TddError::CapacityExhausted)
}

#[inline]
pub(super) fn hash_access<C: TddControl>(
    control: &mut C,
    table: TableKind,
    capacity: usize,
) -> Result<(), TddError<C::Error>> {
    control.admit_hash_access(table, capacity)
}

pub(in crate::types) fn sequence_growth<T, E>(
    capacity: usize,
    required: usize,
) -> Result<GrowthPlan, TddError<E>> {
    let requested_capacity = capacity
        .checked_mul(2)
        .map(|doubled| doubled.max(required).max(4))
        .ok_or(TddError::CapacityExhausted)?;
    let requested_payload_bytes = requested_capacity
        .checked_mul(size_of::<T>())
        .ok_or(TddError::CapacityExhausted)?;
    if requested_payload_bytes > isize::MAX as usize {
        return Err(TddError::CapacityExhausted);
    }
    Ok(GrowthPlan {
        requested_capacity,
        requested_payload_bytes,
        relocation_units: capacity,
    })
}

/// Quotes receiver-cursor hash storage beyond a growth plan's logical ID payload.
///
/// This insert-only set contains four-byte `ConstraintId` keys and uses the standard allocator.
/// On the pinned hashbrown backend, twice the requested capacity bounds replacement capacity;
/// `hash_slots` also covers empty buckets, control bytes and the smallest control group.
/// The quotation includes old-table scanning and retirement, replacement initialization, and
/// prepaid replacement retirement on completion or interruption. Work counts bounded table-slot
/// operations; requested bytes include the ID and control-byte layout. The growth plan separately
/// quotes entry relocation. This supplement does not account for the borrowed constraint set's
/// storage.
pub(super) fn receiver_seen_storage<E>(
    capacity: usize,
    plan: GrowthPlan,
) -> Result<TddWork, TddError<E>> {
    let maximum_capacity = plan
        .requested_capacity
        .checked_mul(2)
        .ok_or(TddError::CapacityExhausted)?;
    let new_slots = hash_slots::<E>(maximum_capacity)?;
    let old_slots = if capacity == 0 {
        0
    } else {
        hash_slots::<E>(capacity)?
    };
    let entry_and_control_bytes = size_of::<ConstraintId>()
        .checked_add(1)
        .ok_or(TddError::CapacityExhausted)?;
    let new_bytes = new_slots
        .checked_mul(entry_and_control_bytes)
        .ok_or(TddError::CapacityExhausted)?;
    let old_bytes = old_slots
        .checked_mul(entry_and_control_bytes)
        .ok_or(TddError::CapacityExhausted)?;
    Layout::from_size_align(new_bytes, align_of::<ConstraintId>())
        .map_err(|_| TddError::CapacityExhausted)?;
    Layout::from_size_align(old_bytes, align_of::<ConstraintId>())
        .map_err(|_| TddError::CapacityExhausted)?;
    let requested_bytes = new_bytes
        .checked_sub(plan.requested_payload_bytes)
        .ok_or(TddError::CapacityExhausted)?;
    // Charge the old slot bound for scanning and again for retirement, without refunding its
    // prepaid cleanup. Charge the replacement slot bound for initialization and for cleanup
    // after completion or interruption. Only the seen table is owned; the constraint set is borrowed.
    let units = old_slots
        .checked_add(new_slots)
        .and_then(|slots| slots.checked_mul(2))
        .ok_or(TddError::CapacityExhausted)?;
    Ok(TddWork::ReceiverSeenStorage {
        units,
        requested_bytes,
    })
}

/// Plans growth for insert-only maps with the standard global allocator.
/// The table-size bound assumes entries occupy at least four bytes on the pinned backend.
/// The work quote covers structural progress, not collision-dependent hashing or equality.
/// The generic key and value types do not establish these layout assumptions.
pub(in crate::types) fn map_growth<K, V, E>(
    len: usize,
    capacity: usize,
    required: usize,
) -> Result<GrowthPlan, TddError<E>> {
    let mut plan = sequence_growth::<(K, V), E>(capacity, required)?;
    let maximum_capacity = plan
        .requested_capacity
        .checked_mul(2)
        .ok_or(TddError::CapacityExhausted)?;
    let new_slots = hash_slots::<E>(maximum_capacity)?;
    let conservative_layout = size_of::<(K, V)>()
        .checked_add(1)
        .and_then(|entry| entry.checked_mul(new_slots))
        .ok_or(TddError::CapacityExhausted)?;
    if conservative_layout > isize::MAX as usize {
        return Err(TddError::CapacityExhausted);
    }
    let old_slots = if capacity == 0 {
        0
    } else {
        hash_slots::<E>(capacity)?
    };
    // Growth scans the old table, moves its entries and initializes the replacement table.
    // Native probes can do more work under collisions; they do not weight every entry by
    // the full replacement capacity in the semantic-progress allowance.
    plan.relocation_units = old_slots
        .checked_add(len)
        .and_then(|rehash| rehash.checked_add(new_slots))
        .ok_or(TddError::CapacityExhausted)?;
    Ok(plan)
}

pub(super) fn reserve_vec<T, C: TddControl>(
    values: &mut Vec<T>,
    additional: usize,
    allocation: AllocationKind,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(TddError::CapacityExhausted)?;
    if required > values.capacity() {
        let plan = sequence_growth::<T, C::Error>(values.capacity(), required)?;
        control.admit(TddWork::Grow { allocation, plan })?;
        values.reserve_exact(plan.requested_capacity - values.len());
    }
    Ok(())
}

pub(in crate::types) fn reserve_smallvec<A: Array, C: TddControl>(
    values: &mut SmallVec<A>,
    additional: usize,
    allocation: AllocationKind,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(TddError::CapacityExhausted)?;
    if required > values.capacity() {
        let plan = sequence_growth::<A::Item, C::Error>(values.capacity(), required)?;
        control.admit(TddWork::Grow { allocation, plan })?;
        values.reserve_exact(plan.requested_capacity - values.len());
    }
    Ok(())
}

pub(super) fn reserve_map<K: Eq + Hash, V, C: TddControl>(
    values: &mut FxHashMap<K, V>,
    table: TableKind,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    if values.len() == values.capacity() {
        let required = values
            .len()
            .checked_add(1)
            .ok_or(TddError::CapacityExhausted)?;
        let plan = map_growth::<K, V, C::Error>(values.len(), values.capacity(), required)?;
        control.admit(TddWork::Grow {
            allocation: AllocationKind::Table(table),
            plan,
        })?;
        values.reserve(plan.requested_capacity - values.len());
    }
    hash_access(control, table, values.capacity())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PathAdvance {
    Satisfaction,
    SimpleConjunction,
    Initialization,
    Traversal,
    Edge,
    Queue,
    Assignment,
    Discovery,
    Pair,
    ImportGroup,
    ImportSequent,
    SequentCheck,
    Replay,
    UniqueNode,
    SourceOrder,
    ConjunctionShape,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PathTable {
    NeverCache,
    Assignments,
    Discovered,
    ElaboratedPairs,
    SingleReplay,
    PairReplay,
    NewAssignments,
    IndependentTypevars,
    DependentTypevars,
    UniqueNodes,
    SourceOrderSeen,
    SourceOrderResult,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PathWork {
    Advance(PathAdvance),
    Access(PathTable),
    FramePush,
    FramePop,
    FillIndices {
        appended: usize,
    },
    RestoreEdge {
        assignments: usize,
        retained_assignments: usize,
        undo: usize,
        retained_undo: usize,
        queued: usize,
        assignment_capacity: usize,
    },
    ClearNewAssignments {
        entries: usize,
        reported_capacity: usize,
    },
    DrainNewAssignments {
        entries: usize,
        reported_capacity: usize,
    },
    ReplayScan {
        entries: usize,
    },
    RetainIndependent {
        candidates: usize,
        candidate_capacity: usize,
    },
    SourceOrderSort {
        entries: usize,
    },
    SeedDiscovered {
        entries: usize,
    },
    SupportScan {
        words: usize,
        typevars: usize,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PathTypevarSet {
    Independent,
    Dependent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PathReserve {
    Sequents,
    Assignments,
    PositiveIndices { required: usize },
    NegativeIndices { required: usize },
    FuelUndo,
    Discovered,
    ElaboratedPairs,
    SingleReplay,
    PairReplay,
    Queue { additional: usize },
    NewAssignments,
}

pub(super) fn admit_path_work<C: TddControl>(
    work: PathWork,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    let mut requested_payload_bytes = 0;
    let quantities: [usize; 5] = match work {
        PathWork::Advance(_) | PathWork::Access(_) | PathWork::FramePush | PathWork::FramePop => {
            [1, 0, 0, 0, 0]
        }
        PathWork::FillIndices { appended } => [appended, 0, 0, 0, 0],
        PathWork::RestoreEdge {
            assignments,
            retained_assignments,
            undo,
            retained_undo,
            queued,
            assignment_capacity,
        } => [
            assignments
                .checked_sub(retained_assignments)
                .ok_or(TddError::CapacityExhausted)?,
            undo.checked_sub(retained_undo)
                .ok_or(TddError::CapacityExhausted)?,
            queued,
            assignments,
            assignment_capacity,
        ],
        PathWork::ClearNewAssignments {
            entries,
            reported_capacity,
        }
        | PathWork::DrainNewAssignments {
            entries,
            reported_capacity,
        } => [entries, reported_capacity, 0, 0, 0],
        PathWork::ReplayScan { entries } => [entries, 0, 0, 0, 0],
        PathWork::RetainIndependent {
            candidates,
            candidate_capacity,
        } => [candidates, candidate_capacity, 0, 0, 0],
        PathWork::SupportScan { words, typevars } => [words, typevars, 0, 0, 0],
        PathWork::SourceOrderSort { entries } => {
            requested_payload_bytes = entries
                .checked_mul(size_of::<super::ConstraintId>())
                .ok_or(TddError::CapacityExhausted)?;
            let log = (usize::BITS - (entries.max(2) - 1).leading_zeros()) as usize;
            let units = entries
                .checked_mul(log)
                .ok_or(TddError::CapacityExhausted)?;
            control.admit(TddWork::Path {
                work,
                units: units.max(1),
                requested_payload_bytes,
            })?;
            return Ok(());
        }
        PathWork::SeedDiscovered { entries } => {
            requested_payload_bytes = entries
                .checked_mul(size_of::<(super::ConstraintId, bool)>())
                .ok_or(TddError::CapacityExhausted)?;
            [entries, 0, 0, 0, 0]
        }
    };
    let units = quantities
        .iter()
        .try_fold(0usize, |total, next| total.checked_add(*next))
        .ok_or(TddError::CapacityExhausted)?
        .max(1);
    control.admit(TddWork::Path {
        work,
        units,
        requested_payload_bytes,
    })?;
    Ok(())
}
