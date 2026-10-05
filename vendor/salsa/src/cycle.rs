//! Cycle handling
//!
//! Salsa's default cycle handling is quite simple: if we encounter a cycle (that is, if we attempt
//! to execute a query that is already on the active query stack), we panic.
//!
//! By setting `cycle_fn` and `cycle_initial` arguments to `salsa::tracked`, queries can opt-in to
//! fixed-point iteration instead.
//!
//! We call the query which triggers the cycle (that is, the query that is already on the stack
//! when it is called again) the "cycle head". The cycle head is responsible for managing iteration
//! of the cycle. When a cycle is encountered, if the cycle head has `cycle_fn` and `cycle_initial`
//! set, it will call the `cycle_initial` function to generate an "empty" or "initial" value for
//! fixed-point iteration, which will be returned to its caller. Then each query in the cycle will
//! compute a value normally, but every computed value will track the head(s) of the cycles it is
//! part of. Every query's "cycle heads" are the union of all the cycle heads of all the queries it
//! depends on. A memoized query result with cycle heads is called a "provisional value".
//!
//! For example, if `qa` calls `qb`, and `qb` calls `qc`, and `qc` calls `qa`, then `qa` will call
//! its `cycle_initial` function to get an initial value, and return that as its result to `qc`,
//! marked with `qa` as cycle head. `qc` will compute its own provisional result based on that, and
//! return to `qb` a result also marked with `qa` as cycle head. `qb` will similarly compute and
//! return a provisional value back to `qa`.
//!
//! When a query observes that it has just computed a result which contains itself as a cycle head,
//! it recognizes that it is responsible for resolving this cycle and calls its `cycle_fn` to
//! decide what value to use. The `cycle_fn` function is passed the provisional value just computed
//! for that query and the count of iterations so far, and returns the value to use for this
//! iteration. This can be the computed value itself, or a different value (e.g., a fallback value).
//!
//! If the cycle head ever observes that the value returned by `cycle_fn` is the same as the
//! provisional value from the previous iteration, this cycle has converged. The cycle head will
//! mark that value as final (by removing itself as cycle head) and return it.
//!
//! Other queries in the cycle will still have provisional values recorded, but those values should
//! now also be considered final! We don't eagerly walk the entire cycle to mark them final.
//! Instead, we wait until the next time that provisional value is read, and then we check if all
//! of its cycle heads have a final result, in which case it, too, can be marked final. (This is
//! implemented in `shallow_verify_memo` and `validate_provisional`.)
//!
//! In nested cycle cases, the inner cycles are iterated as part of the outer cycle iteration. This helps
//! to significantly reduce the number of iterations needed to reach a fixpoint. For nested cycles,
//! the inner cycles head will transfer their lock ownership to the outer cycle. This ensures
//! that, over time, the outer cycle will hold all necessary locks to complete the fixpoint iteration.
//! Without this, different threads would compete for the locks of inner cycle heads, leading to potential
//! hangs (but not deadlocks).

use std::iter::FusedIterator;
use thin_vec::{ThinVec, thin_vec};

use crate::key::DatabaseKeyIndex;
use crate::sync::OnceLock;
use crate::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use crate::{Id, Revision};

/// The maximum number of times we'll fixpoint-iterate before panicking.
///
/// Should only be relevant in case of a badly configured cycle recovery.
pub const MAX_ITERATIONS: u8 = 200;

/// Cycle recovery strategy: Is this query capable of recovering from
/// a cycle that results from executing the function? If so, how?
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CycleRecoveryStrategy {
    /// Cannot recover from cycles: panic.
    ///
    /// This is the default.
    Panic,

    /// Recovers from cycles by fixpoint iterating and/or falling
    /// back to a sentinel value.
    ///
    /// This choice is computed by the query's `cycle_recovery`
    /// function and initial value.
    Fixpoint,

    /// Recovers from cycles by inserting a fallback value for all
    /// queries that have a fallback, and ignoring any other query
    /// in the cycle (as if they were not computed).
    FallbackImmediate,
}

/// A "cycle head" is the query at which we encounter a cycle; that is, if A -> B -> C -> A, then A
/// would be the cycle head. It returns an "initial value" when the cycle is encountered (if
/// fixpoint iteration is enabled for that query), and then is responsible for re-iterating the
/// cycle until it converges.
#[derive(Debug)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub struct CycleHead {
    pub(crate) database_key_index: DatabaseKeyIndex,
    #[cfg_attr(feature = "persistence", serde(skip))]
    pub(crate) iteration: AtomicIterationStamp,

    /// Marks a cycle head as removed within its `CycleHeads` container.
    ///
    /// Cycle heads are marked as removed when the memo from the last iteration (a provisional memo)
    /// is used as the initial value for the next iteration. It's necessary to remove all but its own
    /// head from the `CycleHeads` container, because the query might now depend on fewer cycles
    /// (in case of conditional dependencies). However, we can't actually remove the cycle head
    /// within `fetch_cold_cycle` because we only have a readonly memo. That's what `removed` is used for.
    #[cfg_attr(feature = "persistence", serde(skip))]
    removed: AtomicBool,
}

impl CycleHead {
    pub const fn new(database_key_index: DatabaseKeyIndex, iteration: IterationStamp) -> Self {
        Self {
            database_key_index,
            iteration: AtomicIterationStamp(AtomicU16::new(iteration.0)),
            removed: AtomicBool::new(false),
        }
    }
}

impl Clone for CycleHead {
    fn clone(&self) -> Self {
        Self {
            database_key_index: self.database_key_index,
            iteration: self.iteration.load().into(),
            removed: self.removed.load(Ordering::Relaxed).into(),
        }
    }
}

/// A stamp combining the fixpoint iteration with the within-revision cancellation count.
///
/// The lower byte stores the fixpoint iteration and the upper byte stores the number of global
/// cancellations in the current revision. Including both ensures that provisional memos created
/// before a cancellation aren't reused afterwards. The cancellation count resets on a new revision.
///
/// Within a revision, stamps are ordered first by cancellation count and then by fixpoint
/// iteration. Stamps created after a cancellation compare greater than stamps created before it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct IterationStamp(u16);

impl IterationStamp {
    const fn new(iteration: u8, cancellation_count: u8) -> Self {
        Self(u16::from_le_bytes([iteration, cancellation_count]))
    }

    pub(crate) const fn initial(cancellation_count: u8) -> Self {
        Self::new(0, cancellation_count)
    }

    pub(crate) const fn is_default(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn is_initial_iteration(self) -> bool {
        self.iteration() == 0
    }

    pub(crate) const fn increment_iteration(self) -> Option<Self> {
        let next = Self(self.0 + 1);
        if next.iteration() <= MAX_ITERATIONS {
            Some(next)
        } else {
            None
        }
    }

    pub(crate) const fn iteration_as_u32(self) -> u32 {
        self.iteration() as u32
    }

    pub(crate) const fn cancellation_count(self) -> u8 {
        self.0.to_le_bytes()[1]
    }

    pub(crate) const fn iteration(self) -> u8 {
        self.0.to_le_bytes()[0]
    }
}

impl std::fmt::Debug for IterationStamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IterationStamp")
            .field("iteration", &self.iteration())
            .field("cancellation", &self.cancellation_count())
            .finish()
    }
}

#[derive(Debug, Default)]
pub(crate) struct AtomicIterationStamp(AtomicU16);

impl AtomicIterationStamp {
    pub(crate) fn prepare_store(
        &self,
        next: IterationStamp,
    ) -> Result<PreparedIterationStore<'_>, &'static str> {
        let observed = self.load();
        if observed.cancellation_count() != next.cancellation_count() {
            return Err("cycle iteration changed cancellation count");
        }
        Ok(PreparedIterationStore {
            target: self,
            observed,
            next,
        })
    }
    pub(crate) fn load(&self) -> IterationStamp {
        IterationStamp(self.0.load(Ordering::Relaxed))
    }

    pub(crate) fn load_mut(&mut self) -> IterationStamp {
        IterationStamp(*self.0.get_mut())
    }

    pub(crate) fn set_iteration(&mut self, iteration: IterationStamp) {
        debug_assert_eq!(
            self.load_mut().cancellation_count(),
            iteration.cancellation_count()
        );
        *self.0.get_mut() = iteration.0;
    }
}

pub(crate) struct PreparedIterationStore<'a> {
    target: &'a AtomicIterationStamp,
    observed: IterationStamp,
    next: IterationStamp,
}

impl PreparedIterationStore<'_> {
    pub(crate) fn is_current(&self) -> bool {
        self.target.load() == self.observed
    }

    pub(crate) fn publish(self) {
        self.target.0.store(self.next.0, Ordering::Release);
    }
}

impl From<IterationStamp> for AtomicIterationStamp {
    fn from(iteration: IterationStamp) -> Self {
        AtomicIterationStamp(iteration.0.into())
    }
}

/// Any provisional value generated by any query in a cycle will track the cycle head(s) (can be
/// plural in case of nested cycles) representing the cycles it is part of, and the current
/// iteration count for each cycle head. This struct tracks these cycle heads.
#[derive(Clone, Debug, Default)]
pub struct CycleHeads(ThinVec<CycleHead>);

impl CycleHeads {
    pub(crate) fn reserve_additional(&mut self, additional: usize) {
        self.0.reserve_exact(additional);
    }

    /// Bounds a traversal, including entries removed by a later iteration.
    pub(crate) fn storage_len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn storage_capacity(&self) -> usize {
        self.0.capacity()
    }

    pub(crate) fn prepare_iteration_store(
        &self,
        key: DatabaseKeyIndex,
        next: IterationStamp,
    ) -> Result<Option<PreparedIterationStore<'_>>, &'static str> {
        self.0
            .iter()
            .find(|head| head.database_key_index == key)
            .map(|head| head.iteration.prepare_store(next))
            .transpose()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn initial(database_key_index: DatabaseKeyIndex, iteration: IterationStamp) -> Self {
        Self(thin_vec![CycleHead {
            database_key_index,
            iteration: iteration.into(),
            removed: false.into()
        }])
    }

    pub(crate) fn iter(&self) -> CycleHeadsIterator<'_> {
        CycleHeadsIterator {
            inner: self.0.iter(),
        }
    }

    pub(crate) fn ids(&self) -> CycleHeadIdsIterator<'_> {
        CycleHeadIdsIterator { inner: self.iter() }
    }

    /// Iterates over all cycle heads that aren't equal to `own`.
    pub(crate) fn iter_not_eq(
        &self,
        own: DatabaseKeyIndex,
    ) -> impl DoubleEndedIterator<Item = &CycleHead> {
        self.iter()
            .filter(move |head| head.database_key_index != own)
    }

    pub(crate) fn contains(&self, value: &DatabaseKeyIndex) -> bool {
        self.into_iter()
            .any(|head| head.database_key_index == *value)
    }

    /// Removes all cycle heads except `except` by marking them as removed.
    ///
    /// Note that the heads aren't actually removed. They're only marked as removed and will be
    /// skipped when iterating. This is because we might not have a mutable reference.
    pub(crate) fn remove_all_except(&self, except: DatabaseKeyIndex) {
        for head in self.0.iter() {
            if head.database_key_index == except {
                continue;
            }

            head.removed.store(true, Ordering::Release);
        }
    }

    /// Updates the iteration count for the head `cycle_head_index` to `new_iteration`.
    ///
    /// The mutable reference lets unpublished metadata avoid atomic operations.
    pub(crate) fn update_iteration_count_mut(
        &mut self,
        cycle_head_index: DatabaseKeyIndex,
        new_iteration: IterationStamp,
    ) {
        if let Some(cycle_head) = self
            .0
            .iter_mut()
            .find(|cycle_head| cycle_head.database_key_index == cycle_head_index)
        {
            cycle_head.iteration.set_iteration(new_iteration);
        }
    }

    #[inline]
    pub(crate) fn extend(&mut self, other: &Self) {
        if other.is_empty() {
            return;
        }

        self.0.reserve(other.0.len());

        for head in other {
            debug_assert!(!head.removed.load(Ordering::Relaxed));
            self.insert(head.database_key_index, head.iteration.load());
        }
    }

    pub(crate) fn insert(
        &mut self,
        database_key_index: DatabaseKeyIndex,
        iteration: IterationStamp,
    ) -> bool {
        if let Some(existing) = self
            .0
            .iter_mut()
            .find(|candidate| candidate.database_key_index == database_key_index)
        {
            let removed = existing.removed.get_mut();

            if *removed {
                *removed = false;
                existing.iteration = iteration.into();

                true
            } else {
                let existing_iteration = existing.iteration.load_mut();

                assert_eq!(
                    existing_iteration, iteration,
                    "Can't merge cycle heads {:?} with different iterations ({existing_iteration:?}, {iteration:?})",
                    existing.database_key_index
                );

                false
            }
        } else {
            self.0.push(CycleHead::new(database_key_index, iteration));
            true
        }
    }

    #[cfg(feature = "salsa_unstable")]
    pub(crate) fn allocation_size(&self) -> usize {
        std::mem::size_of_val(self.0.as_slice())
    }
}

#[cfg(feature = "persistence")]
impl serde::Serialize for CycleHeads {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;

        let mut seq = serializer.serialize_seq(None)?;
        for e in self {
            if e.removed.load(Ordering::Relaxed) {
                continue;
            }

            seq.serialize_element(e)?;
        }
        seq.end()
    }
}

#[cfg(feature = "persistence")]
impl<'de> serde::Deserialize<'de> for CycleHeads {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: ThinVec<CycleHead> = serde::Deserialize::deserialize(deserializer)?;
        Ok(CycleHeads(vec))
    }
}

impl IntoIterator for CycleHeads {
    type Item = CycleHead;
    type IntoIter = <ThinVec<Self::Item> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Clone)]
pub struct CycleHeadsIterator<'a> {
    inner: std::slice::Iter<'a, CycleHead>,
}

impl<'a> Iterator for CycleHeadsIterator<'a> {
    type Item = &'a CycleHead;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.inner.next()?;

            if next.removed.load(Ordering::Relaxed) {
                continue;
            }

            return Some(next);
        }
    }
}

impl FusedIterator for CycleHeadsIterator<'_> {}
impl DoubleEndedIterator for CycleHeadsIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.inner.next_back()?;

            if next.removed.load(Ordering::Relaxed) {
                continue;
            }

            return Some(next);
        }
    }
}

impl<'a> std::iter::IntoIterator for &'a CycleHeads {
    type Item = &'a CycleHead;
    type IntoIter = CycleHeadsIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl From<CycleHead> for CycleHeads {
    fn from(value: CycleHead) -> Self {
        Self(thin_vec![value])
    }
}

#[inline]
pub(crate) fn empty_cycle_heads() -> &'static CycleHeads {
    static EMPTY_CYCLE_HEADS: OnceLock<CycleHeads> = OnceLock::new();
    EMPTY_CYCLE_HEADS.get_or_init(|| CycleHeads(ThinVec::new()))
}

#[derive(Clone)]
pub struct CycleHeadIdsIterator<'a> {
    inner: CycleHeadsIterator<'a>,
}

impl Iterator for CycleHeadIdsIterator<'_> {
    type Item = crate::Id;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|head| head.database_key_index.key_index())
    }
}

/// One stored cycle-head entry, observed when its cursor advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleHeadCandidate {
    /// The head has been removed from this cycle-head list.
    Removed,
    /// The head is still present in this cycle-head list.
    Present(Id),
}

/// A borrowed cursor that visits one stored cycle-head entry per step.
///
/// Removed entries are returned explicitly so callers can account for traversal progress.
#[derive(Clone)]
pub struct CycleHeadCandidates<'a> {
    inner: std::slice::Iter<'a, CycleHead>,
}

impl Iterator for CycleHeadCandidates<'_> {
    type Item = CycleHeadCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        let head = self.inner.next()?;
        Some(if head.removed.load(Ordering::Relaxed) {
            CycleHeadCandidate::Removed
        } else {
            CycleHeadCandidate::Present(head.database_key_index.key_index())
        })
    }
}

/// The context that the cycle recovery function receives when a query cycle occurs.
pub struct Cycle<'a> {
    pub(crate) head_ids: CycleHeadIdsIterator<'a>,
    pub(crate) id: Id,
    pub(crate) iteration: u32,
}

impl Cycle<'_> {
    /// An iterator that outputs the [`Id`]s of the current cycle heads.
    /// This always contains the [`Id`] of the current query but it can contain additional cycle head [`Id`]s
    /// if this query is nested in an outer cycle or if it has nested cycles.
    pub fn head_ids(&self) -> CycleHeadIdsIterator<'_> {
        self.head_ids.clone()
    }

    /// Returns a cursor over stored cycle-head entries, including removed heads.
    ///
    /// Each advance examines at most one entry and reads its removal flag at that time.
    pub fn head_candidates(&self) -> CycleHeadCandidates<'_> {
        CycleHeadCandidates {
            inner: self.head_ids.inner.inner.clone(),
        }
    }

    /// The [`Id`] of the query that the current cycle recovery function is processing.
    pub fn id(&self) -> Id {
        self.id
    }

    /// The counter of the current fixed point iteration.
    pub fn iteration(&self) -> u32 {
        self.iteration
    }
}

#[derive(Debug)]
pub(crate) enum ProvisionalStatus {
    Incomplete,
    Provisional,
    /// A provisional memo whose value was cleared while unwinding from a panic.
    Poisoned {
        iteration: IterationStamp,
        verified_at: Revision,
    },
    Final,
}

#[cfg(all(test, not(feature = "shuttle")))]
mod tests {
    use super::*;
    use crate::IngredientIndex;

    fn key(index: u32) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(IngredientIndex::new(0), Id::from_bits(u64::from(index) + 1))
    }

    fn heads(removed: &[bool]) -> CycleHeads {
        CycleHeads(
            removed
                .iter()
                .zip(0..)
                .map(|(&removed, index)| CycleHead {
                    database_key_index: key(index),
                    iteration: IterationStamp::initial(0).into(),
                    removed: removed.into(),
                })
                .collect(),
        )
    }

    fn cycle(heads: &CycleHeads) -> Cycle<'_> {
        Cycle {
            head_ids: heads.ids(),
            id: key(0).key_index(),
            iteration: 0,
        }
    }

    #[test]
    fn head_candidates_visit_removed_slots_in_order() {
        let heads = heads(&[true, true, false, true, false, true]);
        let cycle = cycle(&heads);
        let mut candidates = cycle.head_candidates();

        for expected in [
            CycleHeadCandidate::Removed,
            CycleHeadCandidate::Removed,
            CycleHeadCandidate::Present(key(2).key_index()),
            CycleHeadCandidate::Removed,
            CycleHeadCandidate::Present(key(4).key_index()),
            CycleHeadCandidate::Removed,
        ] {
            assert_eq!(candidates.next(), Some(expected));
        }
        assert_eq!(candidates.next(), None);
        assert_eq!(candidates.next(), None);
    }

    #[test]
    fn head_candidates_observe_removal_when_visited() {
        let heads = heads(&[false, false, false]);
        let cycle = cycle(&heads);
        let mut candidates = cycle.head_candidates();
        assert_eq!(
            candidates.next(),
            Some(CycleHeadCandidate::Present(key(0).key_index()))
        );

        heads.remove_all_except(key(0));
        assert_eq!(candidates.next(), Some(CycleHeadCandidate::Removed));
        assert_eq!(candidates.next(), Some(CycleHeadCandidate::Removed));
        assert_eq!(candidates.next(), None);
    }

    #[test]
    fn head_candidates_match_filtered_ids_for_stable_heads() {
        for removed in [
            &[][..],
            &[false][..],
            &[true][..],
            &[true, false, true, false, true][..],
            &[true, true, true][..],
        ] {
            let heads = heads(removed);
            let cycle = cycle(&heads);
            let present: Vec<_> = cycle
                .head_candidates()
                .filter_map(|candidate| match candidate {
                    CycleHeadCandidate::Removed => None,
                    CycleHeadCandidate::Present(id) => Some(id),
                })
                .collect();
            assert_eq!(present, cycle.head_ids().collect::<Vec<_>>());
        }
    }
}
