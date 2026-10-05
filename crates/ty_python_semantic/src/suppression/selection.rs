//! Shared interval traversal and precedence for diagnostic suppressions.

#[cfg(feature = "experimental-analysis")]
use std::alloc::Layout;
use std::convert::Infallible;

use ruff_text_size::TextRange;
use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{FileSuppressionId, IntervalEntry, Suppression, SuppressionTarget, Suppressions};
use crate::lint::LintId;

/// Restricts which matching directives can suppress a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionMode {
    All,
    Specific,
    SpecificExcept(FileSuppressionId),
}

/// Pending subtrees in source order; the stack visits them in reverse order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PendingIntervals<'db> {
    first: &'db [IntervalEntry],
    following: Option<[&'db [IntervalEntry]; 2]>,
}

impl PendingIntervals<'_> {
    /// Returns the number of slices this descent appends to the traversal stack.
    pub(crate) const fn len(self) -> usize {
        match self.following {
            Some(_) => 3,
            None => 1,
        }
    }
}

/// One visited file directive or interval subtree, before any stack growth.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SelectionStep<'db> {
    Skipped,
    Descend(PendingIntervals<'db>),
    Candidate(&'db Suppression),
}

/// Storage owned by an interval cursor before a descent appends child slices.
#[derive(Clone, Copy, Debug)]
#[cfg(feature = "experimental-analysis")]
pub(crate) struct SelectionStorage {
    pub(crate) len: usize,
    pub(crate) capacity: usize,
    pub(crate) spilled: bool,
}

/// Requested backing and logical transfers for one pending-stack mutation.
#[derive(Clone, Copy, Debug)]
#[cfg(feature = "experimental-analysis")]
pub(crate) struct DescentStorage {
    pub(crate) appended: usize,
    pub(crate) relocated: usize,
    pub(crate) retired_allocations: usize,
    pub(crate) new_allocations: usize,
    pub(crate) allocation_bytes: usize,
    pub(crate) relocation_bytes: usize,
    pub(crate) appended_bytes: usize,
}

#[cfg(feature = "experimental-analysis")]
impl SelectionStorage {
    /// Quotes the actual `SmallVec::reserve` capacity and pending-slice transfers.
    /// Appended slices and new backing must also have their eventual disposal admitted.
    pub(crate) fn descent(self, intervals: PendingIntervals<'_>) -> Option<DescentStorage> {
        let appended = intervals.len();
        let required = self.len.checked_add(appended)?;
        let grows = required > self.capacity;
        let (allocation_bytes, relocated) = if grows {
            let capacity = required.checked_next_power_of_two()?;
            // An inline spill copies initialized slices. Reallocation receives the old
            // allocation layout, so its relocation bound includes unused backing slots.
            let relocated = if self.spilled { self.capacity } else { self.len };
            (Layout::array::<&[IntervalEntry]>(capacity).ok()?.size(), relocated)
        } else {
            (0, 0)
        };
        Some(DescentStorage {
            appended,
            relocated,
            retired_allocations: usize::from(grows && self.spilled),
            new_allocations: usize::from(grows),
            allocation_bytes,
            relocation_bytes: relocated.checked_mul(size_of::<&[IntervalEntry]>())?,
            appended_bytes: appended.checked_mul(size_of::<&[IntervalEntry]>())?,
        })
    }
}

#[cfg(feature = "experimental-analysis")]
impl DescentStorage {
    /// Quotes reservation, slice transfers and prepaid disposal for one descent.
    /// Fixed callback and result transfers are added by the caller's local-transfer helper.
    pub(crate) fn quote(self) -> Option<(usize, usize)> {
        // Reserve metadata (7), growth arithmetic (5), allocation metadata (23), and
        // descent dispatch (9) are bounded independently of the stack's length.
        let work = 44usize
            .checked_add(self.appended.checked_mul(10)?)?
            .checked_add(self.relocated.checked_mul(2)?)?
            .checked_add(self.retired_allocations)?
            .checked_add(self.new_allocations)?;
        type StackMetadata = (std::ptr::NonNull<&'static [IntervalEntry]>, &'static mut usize, usize);
        let bytes = self.allocation_bytes
            .checked_add(self.relocation_bytes)?
            .checked_add(self.appended_bytes.checked_mul(2)?)?
            .checked_add(self.appended.checked_add(2)?.checked_mul(size_of::<StackMetadata>())?)?
            .checked_add(2 * size_of::<PendingIntervals<'static>>())?
            .checked_add(2 * size_of::<Layout>())?
            .checked_add(4 * size_of::<usize>())?
            .checked_add(2 * size_of::<Option<usize>>())?
            .checked_add(5 * size_of::<bool>())?;
        Some((work, bytes))
    }
}

/// A reverse-source interval traversal whose advance visits exactly one pending slice.
#[derive(Debug)]
pub(super) struct IntervalCursor<'db> {
    query: TextRange,
    wanted: u64,
    // Sixteen slices retain the existing interval iterator's inline stack capacity.
    pending: SmallVec<[&'db [IntervalEntry]; 16]>,
}

impl<'db> IntervalCursor<'db> {
    /// Starts at the root without allocating beyond the inline stack.
    pub(super) fn new(entries: &'db [IntervalEntry], query: TextRange, wanted: u64) -> Self {
        let mut pending = SmallVec::new();
        pending.push(entries);
        Self {
            query,
            wanted,
            pending,
        }
    }

    /// Visits one pending subtree or singleton; skipped subtrees still produce a step.
    pub(super) fn advance(&mut self) -> Option<SelectionStep<'db>> {
        let entries = self.pending.pop()?;
        match entries {
            [entry] => {
                let range = entry.suppression.suppressed_range;
                if entry.subtree_target_mask & self.wanted != 0
                    && range.start() <= self.query.end()
                    && range.end() >= self.query.start()
                {
                    Some(SelectionStep::Candidate(&entry.suppression))
                } else {
                    Some(SelectionStep::Skipped)
                }
            }
            entries => {
                let mid = entries.len() / 2;
                let (left, root_and_right) = entries.split_at(mid);
                let Some((root, right)) = root_and_right.split_first() else {
                    return Some(SelectionStep::Skipped);
                };
                if root.subtree_max_end < self.query.start()
                    || root.subtree_target_mask & self.wanted == 0
                {
                    return Some(SelectionStep::Skipped);
                }
                let following = if root.suppression.suppressed_range.start() > self.query.end() {
                    None
                } else {
                    Some([std::slice::from_ref(root), right])
                };
                Some(SelectionStep::Descend(PendingIntervals {
                    first: left,
                    following,
                }))
            }
        }
    }

    /// Appends already-inspected child slices after reserving their complete capacity.
    pub(super) fn descend(&mut self, intervals: PendingIntervals<'db>) {
        self.pending.reserve(intervals.len());
        // The stack is last-in, first-out, so push in source order to visit the
        // right subtree first.
        self.pending.push(intervals.first);
        if let Some([root, right]) = intervals.following {
            self.pending.push(root);
            self.pending.push(right);
        }
    }

    /// Returns the current stack representation without inspecting pending entries.
    #[cfg(feature = "experimental-analysis")]
    fn storage(&self) -> SelectionStorage {
        SelectionStorage {
            len: self.pending.len(),
            capacity: self.pending.capacity(),
            spilled: self.pending.spilled(),
        }
    }
}

/// Retains the first matching candidate while searching for an opening-line candidate.
#[derive(Debug, Default)]
pub(super) struct Preference<'db> {
    end: Option<&'db Suppression>,
}

impl<'db> Preference<'db> {
    /// Returns the winning candidate once an opening-line candidate resolves precedence.
    pub(super) fn consider(
        &mut self,
        candidate: &'db Suppression,
        diagnostic_range: TextRange,
    ) -> Option<&'db Suppression> {
        let Some(end) = self.end else {
            self.end = Some(candidate);
            return candidate
                .suppressed_range
                .contains(diagnostic_range.start())
                .then_some(candidate);
        };
        if !candidate.suppressed_range.contains(diagnostic_range.start()) {
            return None;
        }
        if candidate.suppressed_range.contains_range(end.suppressed_range) {
            Some(end)
        } else {
            Some(candidate)
        }
    }

    /// Returns the first candidate when no opening-line candidate resolved precedence.
    pub(super) const fn finish(&self) -> Option<&'db Suppression> {
        self.end
    }
}

/// Caller-owned traversal and selection state, retained across admitted operations.
#[derive(Debug)]
pub(crate) struct SelectionCursor<'db> {
    file: &'db [Suppression],
    next_file: usize,
    inline: IntervalCursor<'db>,
    id: LintId,
    mode: SelectionMode,
    preference: Preference<'db>,
}

impl<'db> SelectionCursor<'db> {
    /// Starts diagnostic suppression selection, considering file directives before inline ones.
    pub(crate) fn new(
        suppressions: &'db Suppressions,
        range: TextRange,
        id: LintId,
        mode: SelectionMode,
    ) -> Self {
        Self {
            file: &suppressions.file,
            next_file: 0,
            inline: IntervalCursor::new(
                &suppressions.inline.entries,
                range,
                SuppressionTarget::All.target_mask() | SuppressionTarget::Lint(id).target_mask(),
            ),
            id,
            mode,
            preference: Preference::default(),
        }
    }

    /// Visits one file directive or one interval slice without growing the stack.
    pub(crate) fn advance(&mut self) -> Option<SelectionStep<'db>> {
        if let Some(suppression) = self.file.get(self.next_file) {
            self.next_file += 1;
            return Some(SelectionStep::Candidate(suppression));
        }
        self.inline.advance()
    }

    /// Returns the stack metadata needed to admit a pending descent.
    #[cfg(feature = "experimental-analysis")]
    pub(crate) fn storage(&self) -> SelectionStorage {
        self.inline.storage()
    }

    /// Appends the slices from one traversal step; callers admit storage before calling this.
    pub(crate) fn descend(&mut self, intervals: PendingIntervals<'db>) {
        self.inline.descend(intervals);
    }

    /// Tests target, diagnostic endpoints and the additional directive restriction.
    pub(super) fn accepts(&self, candidate: &Suppression) -> bool {
        candidate.matches(self.id)
            && candidate.applies_to(self.inline.query)
            && match self.mode {
                SelectionMode::All => true,
                SelectionMode::Specific => candidate.target.is_lint(),
                SelectionMode::SpecificExcept(excluded) => {
                    candidate.target.is_lint() && candidate.id() != excluded
                }
            }
    }

    /// Incorporates one eligible candidate and returns a winner when precedence is resolved.
    pub(crate) fn consider(&mut self, candidate: &'db Suppression) -> Option<&'db Suppression> {
        if self.accepts(candidate) {
            self.preference.consider(candidate, self.inline.query)
        } else {
            None
        }
    }

    /// Completes selection after all applicable file and inline directives were considered.
    pub(crate) const fn finish(&self) -> Option<&'db Suppression> {
        self.preference.finish()
    }
}

#[cfg(feature = "experimental-analysis")]
impl SelectionCursor<'_> {
    /// Quotes initial inline storage, cursor carriers and eventual fixed owner disposal.
    /// The caller separately admits its callback and returned cursor representations.
    pub(crate) const fn new_quote() -> (usize, usize) {
        // Cursor fields (6), interval fields (3), empty stack (2), initial push (9),
        // target-mask construction (10), preference (1), and owner retirement (8).
        let work = 6 + 3 + 2 + 9 + 10 + 1 + 8;
        let bytes = size_of::<(&[Suppression], usize, IntervalCursor<'_>, LintId, SelectionMode, Preference<'_>)>()
            + size_of::<(&[IntervalEntry], TextRange, u64)>()
            + size_of::<SmallVec<[&[IntervalEntry]; 16]>>()
            + size_of::<rustc_hash::FxHasher>()
            + 4 * size_of::<u64>()
            + size_of::<&[IntervalEntry]>()
            + 2 * size_of::<(std::ptr::NonNull<&[IntervalEntry]>, &mut usize, usize)>()
            + 2 * size_of::<Vec<&[IntervalEntry]>>();
        (work, bytes)
    }

    /// Quotes one file or interval visit, including skipped slices and shared-loop dispatch.
    pub(crate) const fn advance_quote() -> (usize, usize) {
        // Sum the bounded source branches: file (11), pop (15), slice tag (1),
        // singleton (21), split (13), pruning (12), children (9), descent (4), loop (2).
        let work = 11 + 15 + 1 + 21 + 13 + 12 + 9 + 4 + 2;
        let bytes = size_of::<Option<&Suppression>>()
            + 3 * size_of::<usize>()
            + size_of::<Option<&[IntervalEntry]>>()
            + size_of::<&[IntervalEntry]>()
            + size_of::<TextRange>()
            + 2 * size_of::<u64>()
            + 8 * size_of::<ruff_text_size::TextSize>()
            + 9 * size_of::<bool>()
            + size_of::<(&[IntervalEntry], &[IntervalEntry])>()
            + size_of::<Option<(&IntervalEntry, &[IntervalEntry])>>()
            + size_of::<Option<[&[IntervalEntry]; 2]>>()
            + size_of::<PendingIntervals<'_>>()
            + 3 * size_of::<SelectionStep<'_>>()
            + size_of::<Option<SelectionStep<'_>>>()
            + size_of::<(std::ptr::NonNull<&[IntervalEntry]>, &mut usize, usize)>()
            + size_of::<(*const &[IntervalEntry], usize)>();
        (work, bytes)
    }

    /// Quotes the metadata read and checked construction of a complete descent quotation.
    /// Use this before evaluating `storage().descent(intervals)?.quote()`.
    pub(crate) const fn descent_quote_quote() -> (usize, usize) {
        let work = 20 + 27 + 18;
        let bytes = size_of::<SelectionStorage>()
            + size_of::<DescentStorage>()
            + size_of::<Option<DescentStorage>>()
            + 2 * size_of::<Layout>()
            + 22 * size_of::<usize>()
            + 22 * size_of::<Option<usize>>()
            + size_of::<Option<(usize, usize)>>()
            + 3 * size_of::<bool>();
        (work, bytes)
    }

    /// Quotes target filtering and precedence for one candidate, without scanning other entries.
    pub(crate) const fn consider_quote() -> (usize, usize) {
        // Eligibility (37), dispatch (6), preference branches (37), and shared return (3).
        let work = 37 + 6 + 37 + 3;
        let bytes = size_of::<SelectionMode>()
            + 2 * size_of::<LintId>()
            + 3 * size_of::<SuppressionTarget>()
            + 2 * size_of::<FileSuppressionId>()
            + 6 * size_of::<TextRange>()
            + 6 * size_of::<ruff_text_size::TextSize>()
            + 21 * size_of::<bool>()
            + 4 * size_of::<Option<&Suppression>>();
        (work, bytes)
    }

    /// Quotes copying the retained fallback candidate into the completed selection result.
    pub(crate) const fn finish_quote() -> (usize, usize) {
        (3, size_of::<Option<&Suppression>>())
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousSelectionEffects)]
    pub(crate) trait SelectionEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn advance(&self, cursor: &mut SelectionCursor<'db>) -> Result<Option<SelectionStep<'db>>, Self::Error>;

        #[operation(local)]
        async fn descend(&self, cursor: &mut SelectionCursor<'db>, intervals: PendingIntervals<'db>) -> Result<(), Self::Error>;

        #[operation(local)]
        async fn consider(&self, cursor: &mut SelectionCursor<'db>, candidate: &'db Suppression) -> Result<Option<&'db Suppression>, Self::Error>;

        #[operation(local)]
        async fn finish(&self, cursor: &SelectionCursor<'db>) -> Result<Option<&'db Suppression>, Self::Error>;
    }

    /// Selects the preferred suppression, admitting each scanned interval and stack mutation.
    #[synchronous(select_sync)]
    #[capabilities(effects = SelectionEffects)]
    #[passive_values(SelectionStep::Skipped, SelectionStep::Descend, SelectionStep::Candidate)]
    pub(crate) async fn select_with<'db, E: SelectionEffects<'db>>(
        cursor: &mut SelectionCursor<'db>,
        effects: &E,
    ) -> Result<Option<&'db Suppression>, E::Error> {
        #[cursor_loop]
        while let Some(step) = effects.advance(cursor).await? {
            match step {
                SelectionStep::Skipped => {}
                SelectionStep::Descend(intervals) => effects.descend(cursor, intervals).await?,
                SelectionStep::Candidate(candidate) => {
                    if let Some(selected) = effects.consider(cursor, candidate).await? {
                        return Ok(Some(selected));
                    }
                }
            }
        }
        effects.finish(cursor).await
    }
}

/// Runs the shared selector synchronously for ordinary inference and suppression checks.
pub(super) fn select(
    suppressions: &Suppressions,
    range: TextRange,
    id: LintId,
    mode: SelectionMode,
) -> Option<&Suppression> {
    let mut cursor = SelectionCursor::new(suppressions, range, id, mode);
    match select_sync(&mut cursor, &OrdinarySelectionEffects) {
        Ok(selected) => selected,
        Err(error) => match error {},
    }
}

#[derive(Debug)]
struct OrdinarySelectionEffects;

impl<'db> SynchronousSelectionEffects<'db> for OrdinarySelectionEffects {
    type Error = Infallible;

    fn advance(&self, cursor: &mut SelectionCursor<'db>) -> Result<Option<SelectionStep<'db>>, Self::Error> {
        Ok(cursor.advance())
    }

    fn descend(&self, cursor: &mut SelectionCursor<'db>, intervals: PendingIntervals<'db>) -> Result<(), Self::Error> {
        cursor.descend(intervals);
        Ok(())
    }

    fn consider(&self, cursor: &mut SelectionCursor<'db>, candidate: &'db Suppression) -> Result<Option<&'db Suppression>, Self::Error> {
        Ok(cursor.consider(candidate))
    }

    fn finish(&self, cursor: &SelectionCursor<'db>) -> Result<Option<&'db Suppression>, Self::Error> {
        Ok(cursor.finish())
    }
}
