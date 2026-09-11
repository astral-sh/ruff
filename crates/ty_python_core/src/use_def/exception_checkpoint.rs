use crate::reachability_constraints::ScopedReachabilityConstraintId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Revision(u64);

/// The provenance of the visible bindings and the call gates applied to them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ExceptionCheckpointSnapshot {
    bindings: Revision,
    calls: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckpointFlow {
    Normalized(ScopedReachabilityConstraintId),
    Conservative(Revision),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExceptionCheckpointKey {
    bindings: Revision,
    flow: CheckpointFlow,
}

/// Tracks state changes that normalized scope-wide reachability cannot distinguish.
///
/// Binding identities are restored with flow snapshots. Joining different identities creates a
/// fresh one, even if both paths have ambiguous reachability. Calls do not immediately change the
/// checkpoint key, since a later straight-line call cannot expose additional bindings. Their
/// identities still matter when restoring or joining paths: catching an exception from a
/// `NoReturn` call can make previously unreachable bindings visible again.
#[derive(Debug, Default)]
pub(super) struct ExceptionCheckpointState {
    current: ExceptionCheckpointSnapshot,
    next_revision: Revision,
    control_flow_revision: Revision,
}

impl ExceptionCheckpointState {
    fn fresh_revision(&mut self) -> Revision {
        self.next_revision.0 += 1;
        self.next_revision
    }

    pub(super) fn record_binding_change(&mut self) {
        self.current.bindings = self.fresh_revision();
    }

    pub(super) fn record_call_gate(&mut self) {
        self.current.calls = self.fresh_revision();
    }

    pub(super) fn snapshot(&self) -> ExceptionCheckpointSnapshot {
        self.current
    }

    pub(super) fn restore(&mut self, snapshot: ExceptionCheckpointSnapshot) {
        let calls_changed = self.current.calls != snapshot.calls;
        self.control_flow_revision = self.fresh_revision();
        self.current = snapshot;
        if calls_changed {
            self.current.bindings = self.control_flow_revision;
        }
    }

    pub(super) fn merge(&mut self, snapshot: ExceptionCheckpointSnapshot) {
        self.control_flow_revision = self.fresh_revision();
        if self.current != snapshot {
            if self.current.calls != snapshot.calls {
                self.current.calls = self.control_flow_revision;
            }
            self.current.bindings = self.control_flow_revision;
        }
    }

    /// Uses the conservative control-flow revision when the reachability arena is saturated.
    pub(super) fn key(
        &self,
        normalized_flow: Option<ScopedReachabilityConstraintId>,
    ) -> ExceptionCheckpointKey {
        ExceptionCheckpointKey {
            bindings: self.current.bindings,
            flow: normalized_flow.map_or(
                CheckpointFlow::Conservative(self.control_flow_revision),
                CheckpointFlow::Normalized,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: Option<ScopedReachabilityConstraintId> =
        Some(ScopedReachabilityConstraintId::AMBIGUOUS);

    #[test]
    fn unchanged_branches_preserve_binding_identity() {
        let mut state = ExceptionCheckpointState::default();
        state.record_binding_change();
        state.record_call_gate();
        let snapshot = state.snapshot();
        let key = state.key(FLOW);

        state.restore(snapshot);
        state.merge(snapshot);
        assert_eq!(state.key(FLOW), key);
    }

    #[test]
    fn conservative_keys_still_coalesce_straight_line_calls() {
        let mut state = ExceptionCheckpointState::default();
        state.record_binding_change();
        let key = state.key(None);
        assert_ne!(key, state.key(FLOW));
        state.record_call_gate();
        state.record_call_gate();
        assert_eq!(state.key(None), key);

        let snapshot = state.snapshot();
        state.restore(snapshot);
        let restored_key = state.key(None);
        assert_ne!(restored_key, key);
        state.merge(snapshot);
        assert_ne!(state.key(None), restored_key);
    }
}
