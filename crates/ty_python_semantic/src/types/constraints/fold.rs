//! Balanced folds with staged carries and atomic acceptance of each input.

use std::ops::ControlFlow;

use smallvec::SmallVec;

#[cfg(any(test, feature = "experimental-analysis"))]
use super::combination::PendingCombination;
use super::combination::{Combination, Value};
use super::control::{
    AllocationKind, TddControl, TddError, TddWork, Unrestricted, reserve_smallvec, unrestricted,
};
use super::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintSet, ConstraintSetBuilder, NodeId, SourceOrderId,
};

#[derive(Clone, Copy)]
pub(in crate::types) enum ConstraintFoldKind {
    All,
    Any,
}

impl ConstraintFoldKind {
    pub(super) fn identity(self) -> NodeId {
        match self {
            Self::All => ALWAYS_TRUE,
            Self::Any => ALWAYS_FALSE,
        }
    }

    pub(super) fn absorbing(self) -> NodeId {
        match self {
            Self::All => ALWAYS_FALSE,
            Self::Any => ALWAYS_TRUE,
        }
    }
}

/// Combines constraints in a balanced tree while preserving their source order.
///
/// Because the operator is associative, we don't have to combine the nodes left to right; we
/// can instead combine them in a "tree-like" way:
///
/// ```text
/// linear:  (((((a ∨ b) ∨ c) ∨ d) ∨ e) ∨ f) ∨ g
/// tree:    ((a ∨ b) ∨ (c ∨ d)) ∨ ((e ∨ f) ∨ g)
/// ```
///
/// We have to invoke the operator the same number of times. But BDD operators are often much
/// cheaper when the operands are small, and with the tree shape, many more of the invocations
/// are performed on small BDDs.
///
/// The accumulator stores subtrees with distinct depths, so its length is `O(log n)` in the
/// number of inputs. For example, after seven inputs it contains `abcd/2`, `ef/1`, and `g/0`.
/// Eight entries fit inline; the first spill occurs when nine depths are occupied, at 511
/// inputs. Finalization combines these remaining subtrees in their stored order.
///
/// Callers must stop producing inputs when [`Self::push`] returns [`ControlFlow::Break`]. This
/// happens when an input or an intermediate result reaches the absorbing terminal. Producing
/// the next input can perform semantic work, so it must happen after this decision. The fold
/// retains no storage borrow between inputs, allowing their evaluation to suspend.
pub(in crate::types) struct ConstraintFold<'db, 'c> {
    pub(super) builder: &'c ConstraintSetBuilder<'db>,
    pub(super) kind: ConstraintFoldKind,
    pub(super) accumulator: SmallVec<[(NodeId, Option<SourceOrderId>, u8); 8]>,
}

impl<'db, 'c> ConstraintFold<'db, 'c> {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn builder(&self) -> &'c ConstraintSetBuilder<'db> {
        self.builder
    }

    pub(in crate::types) fn new(
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
    ) -> Self {
        Self {
            builder,
            kind,
            accumulator: SmallVec::new(),
        }
    }

    fn push_progress(&self, next: ConstraintSet<'db, 'c>) -> PushProgress {
        next.verify_builder(self.builder);
        PushProgress {
            value: (next.node, next.source_order),
            depth: 0,
            index: self.accumulator.len(),
            carried: false,
        }
    }

    fn finish_progress(&self) -> FinishProgress {
        FinishProgress {
            value: (self.kind.identity(), None),
            index: 0,
        }
    }

    fn drive<P: FoldProgress>(&mut self, progress: &mut P) {
        while let Some(combination) = unrestricted(progress.advance(self, &mut Unrestricted)) {
            let value = combination.finish(&mut self.builder.storage.borrow_mut());
            progress.resume(value);
        }
    }

    pub(in crate::types) fn push(
        &mut self,
        next: ConstraintSet<'db, 'c>,
    ) -> ControlFlow<ConstraintSet<'db, 'c>> {
        let mut progress = self.push_progress(next);
        self.drive(&mut progress);
        progress.result(self)
    }

    pub(in crate::types) fn finish(mut self) -> ConstraintSet<'db, 'c> {
        self.finish_borrowed()
    }

    pub(in crate::types) fn finish_borrowed(&mut self) -> ConstraintSet<'db, 'c> {
        let mut progress = self.finish_progress();
        self.drive(&mut progress);
        progress.constraint(self.builder)
    }

    // These adapters share the ordinary transitions while permitting admission between steps.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn prepare_push<'fold>(
        &'fold mut self,
        next: ConstraintSet<'db, 'c>,
    ) -> PreparedFoldPush<'fold, 'db, 'c> {
        let progress = self.push_progress(next);
        PreparedFoldPush {
            prepared: Prepared {
                fold: self,
                progress,
                child: None,
                completed: false,
            },
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn prepare_finish(&mut self) -> PreparedFoldFinish<'_, 'db, 'c> {
        let progress = self.finish_progress();
        PreparedFoldFinish {
            prepared: Prepared {
                fold: self,
                progress,
                child: None,
                completed: false,
            },
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn begin_push<'fold>(
        &'fold mut self,
        next: ConstraintSet<'db, 'c>,
    ) -> FoldPush<'fold, 'db, 'c> {
        let progress = self.push_progress(next);
        FoldPush {
            cursor: Cursor {
                fold: self,
                progress,
                child: None,
                completed: false,
            },
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn begin_finish(&mut self) -> FoldFinish<'_, 'db, 'c> {
        let progress = self.finish_progress();
        FoldFinish {
            cursor: Cursor {
                fold: self,
                progress,
                child: None,
                completed: false,
            },
        }
    }
}

// Both drivers retain concrete operation state. A completed transition returns no request;
// its final value stays in that state until the caller constructs the public result.
trait FoldProgress {
    fn advance<C: TddControl>(
        &mut self,
        fold: &mut ConstraintFold<'_, '_>,
        control: &mut C,
    ) -> Result<Option<Combination>, TddError<C::Error>>;

    fn resume(&mut self, value: Value);
}

struct PushProgress {
    value: Value,
    depth: u8,
    index: usize,
    carried: bool,
}

impl PushProgress {
    fn result<'db, 'c>(
        &self,
        fold: &ConstraintFold<'db, 'c>,
    ) -> ControlFlow<ConstraintSet<'db, 'c>> {
        if self.value.0 == fold.kind.absorbing() {
            ControlFlow::Break(ConstraintSet::from_node(
                fold.builder,
                self.value.0,
                self.value.1,
            ))
        } else {
            ControlFlow::Continue(())
        }
    }
}

impl FoldProgress for PushProgress {
    // A request leaves its index and depth here. Both drivers resume its completed value
    // before advancing again, so the continuation never travels with the child request.
    #[inline]
    fn advance<C: TddControl>(
        &mut self,
        fold: &mut ConstraintFold<'_, '_>,
        control: &mut C,
    ) -> Result<Option<Combination>, TddError<C::Error>> {
        control.admit(TddWork::FoldAdvance)?;
        let Self {
            value,
            depth,
            index,
            carried,
        } = self;
        if value.0 == fold.kind.absorbing() {
            if *carried {
                control.admit(TddWork::FoldCommit {
                    removed: fold.accumulator.len() - *index,
                    appended: false,
                })?;
                fold.accumulator.truncate(*index);
            }
            return Ok(None);
        }
        let current_depth = if *carried {
            next_depth(*depth)?
        } else {
            *depth
        };
        // Equal-depth subtrees carry like binary digits: a/0 b/0 becomes ab/1,
        // then ab/1 cd/1 becomes abcd/2. Reading by index keeps accepted entries
        // intact until every graph and sidecar child for this input has completed.
        if *index != 0
            && let Some(&(left, source, existing_depth)) = fold.accumulator.get(*index - 1)
            && existing_depth == current_depth
        {
            let combination = Combination::new(fold.kind, (left, source), *value);
            *depth = current_depth;
            *index -= 1;
            return Ok(Some(combination));
        }
        let len = fold.accumulator.len();
        if *index == len && len == fold.accumulator.capacity() {
            reserve_smallvec(
                &mut fold.accumulator,
                1,
                AllocationKind::FoldAccumulator,
                control,
            )?;
        }
        control.admit(TddWork::FoldCommit {
            removed: len - *index,
            appended: true,
        })?;
        if *index != len {
            fold.accumulator.truncate(*index);
        }
        fold.accumulator.push((value.0, value.1, current_depth));
        Ok(None)
    }

    #[inline]
    fn resume(&mut self, value: Value) {
        self.value = value;
        self.carried = true;
    }
}

struct FinishProgress {
    value: Value,
    index: usize,
}

impl FinishProgress {
    fn constraint<'db, 'c>(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_node(builder, self.value.0, self.value.1)
    }
}

impl FoldProgress for FinishProgress {
    #[inline]
    fn advance<C: TddControl>(
        &mut self,
        fold: &mut ConstraintFold<'_, '_>,
        control: &mut C,
    ) -> Result<Option<Combination>, TddError<C::Error>> {
        control.admit(TddWork::FoldAdvance)?;
        let Self { value, index } = self;
        let Some(&(node, source, _)) = fold.accumulator.get(*index) else {
            return Ok(None);
        };
        // Finalization keeps every remaining history, even if an earlier graph
        // combination became terminal. Its operands follow accumulator order.
        let combination = Combination::new(fold.kind, *value, (node, source));
        *index += 1;
        Ok(Some(combination))
    }

    #[inline]
    fn resume(&mut self, value: Value) {
        self.value = value;
    }
}

pub(super) fn next_depth<E>(depth: u8) -> Result<u8, TddError<E>> {
    depth.checked_add(1).ok_or(TddError::CapacityExhausted)
}

#[cfg(any(test, feature = "experimental-analysis"))]
struct Prepared<'fold, 'db, 'c, P> {
    fold: &'fold mut ConstraintFold<'db, 'c>,
    progress: P,
    child: Option<Combination>,
    completed: bool,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'fold, 'db, 'c, P: FoldProgress> Prepared<'fold, 'db, 'c, P> {
    fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<()>, TddError<C::Error>> {
        if self.completed || self.child.is_some() {
            control.admit(TddWork::FoldAdvance)?;
        } else {
            self.child = self.progress.advance(self.fold, control)?;
            self.completed = self.child.is_none();
        }
        Ok(if self.completed {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    }

    fn into_cursor(self) -> Cursor<'fold, 'db, 'c, P> {
        Cursor {
            fold: self.fold,
            progress: self.progress,
            child: self.child.map(PendingCombination::new),
            completed: self.completed,
        }
    }
}

/// Runs a push's first transition without constructing a graph cursor.
/// The outer `Continue` retains the requested combination; transfer it with [`Self::into_cursor`]
/// before advancing further. The outer `Break` contains the push's short-circuit result.
/// A completed push may already have accepted its input.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct PreparedFoldPush<'fold, 'db, 'c> {
    prepared: Prepared<'fold, 'db, 'c, PushProgress>,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'fold, 'db, 'c> PreparedFoldPush<'fold, 'db, 'c> {
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ControlFlow<ConstraintSet<'db, 'c>>>, TddError<C::Error>> {
        Ok(self
            .prepared
            .advance_with(control)?
            .map_break(|()| self.prepared.progress.result(self.prepared.fold)))
    }

    pub(in crate::types) fn into_cursor(self) -> FoldPush<'fold, 'db, 'c> {
        FoldPush {
            cursor: self.prepared.into_cursor(),
        }
    }
}

/// Runs finish's first transition without constructing a graph cursor.
/// `Continue` retains the requested combination; transfer it with [`Self::into_cursor`]
/// before advancing further. The accepted fold remains unchanged.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct PreparedFoldFinish<'fold, 'db, 'c> {
    prepared: Prepared<'fold, 'db, 'c, FinishProgress>,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'fold, 'db, 'c> PreparedFoldFinish<'fold, 'db, 'c> {
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, TddError<C::Error>> {
        Ok(self.prepared.advance_with(control)?.map_break(|()| {
            self.prepared
                .progress
                .constraint(self.prepared.fold.builder)
        }))
    }

    pub(in crate::types) fn into_cursor(self) -> FoldFinish<'fold, 'db, 'c> {
        FoldFinish {
            cursor: self.prepared.into_cursor(),
        }
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
struct Cursor<'fold, 'db, 'c, P> {
    fold: &'fold mut ConstraintFold<'db, 'c>,
    progress: P,
    child: Option<PendingCombination>,
    completed: bool,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<P: FoldProgress> Cursor<'_, '_, '_, P> {
    fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<()>, TddError<C::Error>> {
        if self.completed {
            control.admit(TddWork::FoldAdvance)?;
            return Ok(ControlFlow::Break(()));
        }
        if let Some(child) = &mut self.child {
            let result =
                child.advance_with(&mut self.fold.builder.storage.borrow_mut(), control)?;
            if let ControlFlow::Break(value) = result {
                self.progress.resume(value);
                self.child = None;
            }
            return Ok(ControlFlow::Continue(()));
        }
        match self.progress.advance(self.fold, control)? {
            None => {
                self.completed = true;
                Ok(ControlFlow::Break(()))
            }
            Some(combination) => {
                self.child = Some(PendingCombination::new(combination));
                Ok(ControlFlow::Continue(()))
            }
        }
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct FoldPush<'fold, 'db, 'c> {
    cursor: Cursor<'fold, 'db, 'c, PushProgress>,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'db, 'c> FoldPush<'_, 'db, 'c> {
    /// After an error, drop this cursor and retry the same unaccepted input on the same fold.
    /// Completed storage identities remain valid; accepted accumulator entries are unchanged.
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ControlFlow<ConstraintSet<'db, 'c>>>, TddError<C::Error>> {
        Ok(self
            .cursor
            .advance_with(control)?
            .map_break(|()| self.cursor.progress.result(self.cursor.fold)))
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct FoldFinish<'fold, 'db, 'c> {
    cursor: Cursor<'fold, 'db, 'c, FinishProgress>,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'db, 'c> FoldFinish<'_, 'db, 'c> {
    /// After an error, drop this cursor and begin finish again on the same accepted fold.
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, TddError<C::Error>> {
        Ok(self
            .cursor
            .advance_with(control)?
            .map_break(|()| self.cursor.progress.constraint(self.cursor.fold.builder)))
    }
}
