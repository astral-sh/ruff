//! Passive observations of owned receiver-constraint admission boundaries.

use std::cell::Cell;

/// Identifies the storage operation bracketed by its normal admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    Owner,
    Package,
    Merge,
    Transfer,
}

impl Stage {
    const fn index(self) -> usize {
        match self {
            Self::Owner => 0,
            Self::Package => 1,
            Self::Merge => 2,
            Self::Transfer => 3,
        }
    }
}

/// Counts attempted and completed operations without affecting their admission.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::types) struct Boundary {
    pub(in crate::types) before: usize,
    pub(in crate::types) after: usize,
}

/// Records whether rejected merging reached the ordinary graph-loading operation.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Snapshot {
    boundaries: [Boundary; 4],
    pub(in crate::types) rejected_merges: usize,
    pub(in crate::types) merge_loads: usize,
    pub(in crate::types) drained: Option<Drainage>,
}

/// Captures child state after the driver returns while its retained pools remain in scope.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct Drainage {
    pub(in crate::types) live_children: usize,
    pub(in crate::types) child_events: usize,
}

impl Snapshot {
    const fn new() -> Self {
        Self {
            boundaries: [Boundary {
                before: 0,
                after: 0,
            }; 4],
            rejected_merges: 0,
            merge_loads: 0,
            drained: None,
        }
    }

    pub(in crate::types) const fn boundary(self, stage: Stage) -> Boundary {
        self.boundaries[stage.index()]
    }
}

thread_local! {
    static OBSERVATIONS: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Restricts observations to the current control, including its unwinding cleanup.
#[derive(Debug)]
pub(in crate::types) struct Recording;

impl Recording {
    pub(in crate::types) fn start() -> Self {
        assert!(!ACTIVE.replace(true));
        reset();
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ACTIVE.set(false);
    }
}

/// Clears observations without enabling recording; use `Recording::start` to activate its guard.
/// No owner or constraint is retained by the observations.
pub(in crate::types) fn reset() {
    OBSERVATIONS.set(Snapshot::new());
}

pub(in crate::types) fn snapshot() -> Snapshot {
    OBSERVATIONS.get()
}

/// Records entry before the operation's ordinary admission can refuse it.
pub(in crate::types) fn observe_before(stage: Stage) {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = OBSERVATIONS.get();
    snapshot.boundaries[stage.index()].before += 1;
    OBSERVATIONS.set(snapshot);
}

/// Records completion only after the normal admission and storage operation succeed.
pub(in crate::types) fn observe_after(stage: Stage) {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = OBSERVATIONS.get();
    snapshot.boundaries[stage.index()].after += 1;
    OBSERVATIONS.set(snapshot);
}

/// Records rejection of a nonterminal operand before ordinary loading begins.
pub(in crate::types) fn observe_rejected_merge() {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = OBSERVATIONS.get();
    snapshot.rejected_merges += 1;
    OBSERVATIONS.set(snapshot);
}

/// Records each actual ordinary load invoked by the terminal merge adapter.
pub(in crate::types) fn observe_merge_load() {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = OBSERVATIONS.get();
    snapshot.merge_loads += 1;
    OBSERVATIONS.set(snapshot);
}

/// Records actual driver drainage before the helper leaves the retained pools' lexical scope.
/// This observes their continued lifetime, not the completion of their later deallocation.
pub(in crate::types) fn observe_run_drained() {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = OBSERVATIONS.get();
    snapshot.drained = Some(Drainage {
        live_children: super::retained::observations::progress().0,
        child_events: super::retained::observations::snapshot().count,
    });
    OBSERVATIONS.set(snapshot);
}
