use crate::Db;
use crate::types::{DivergentType, Type};

use super::equation::{CycleRootSet, ProjectionSolutions, root_candidate_from_previous};
use super::evidence::ProjectionEvidenceSet;

/// A type slot participating in result-level projection cycle recovery.
#[derive(Debug, Clone, Copy)]
pub(super) struct ProjectionRecoverySlot<'db> {
    pub(super) previous: Option<Type<'db>>,
    pub(super) joined: Type<'db>,
    pub(super) role: ProjectionRecoverySlotRole,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ProjectionRecoverySlotRole {
    DemandOnly,
    Candidate {
        /// The cycle root this result slot represented in the previous iteration, if unique.
        root_hint: Option<DivergentType>,
    },
}

/// Cycle-recovery-time accumulator for result-wide projection solving.
pub(crate) struct ProjectionRecoveryBuilder<'db> {
    roots: CycleRootSet,
    slots: Vec<ProjectionRecoverySlot<'db>>,
}

impl<'db> ProjectionRecoveryBuilder<'db> {
    pub(crate) fn new(cycle: &salsa::Cycle) -> Self {
        Self {
            roots: CycleRootSet::from_cycle(cycle),
            slots: Vec::new(),
        }
    }

    /// Cycle-recovery-time API: records a joined result slot that can contain projection demands.
    pub(crate) fn push(&mut self, previous: Option<Type<'db>>, joined: Type<'db>) -> Type<'db> {
        self.slots.push(ProjectionRecoverySlot {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::DemandOnly,
        });
        joined
    }

    /// Cycle-recovery-time API: records a joined result slot that can also act as a root candidate.
    pub(crate) fn push_candidate(
        &mut self,
        db: &'db dyn Db,
        previous: Option<Type<'db>>,
        joined: Type<'db>,
    ) -> Type<'db> {
        let root_hint =
            previous.and_then(|previous| root_candidate_from_previous(db, previous, &self.roots));
        self.slots.push(ProjectionRecoverySlot {
            previous,
            joined,
            role: ProjectionRecoverySlotRole::Candidate { root_hint },
        });
        joined
    }

    /// Cycle-recovery-time API: solves all projection variables visible in the recorded slots.
    pub(crate) fn finish(
        &self,
        db: &'db dyn Db,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<ProjectionSolutions<'db>> {
        ProjectionSolutions::from_recovery_slots(db, &self.roots, &self.slots, evidence)
    }
}
