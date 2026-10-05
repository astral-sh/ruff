//! Admission policy for execution inside an experimental relation attempt.

use super::{ExecutionAdmission, ExecutionWork, SchedulingFailure};
use crate::Db;
use crate::types::constructor::expansion_probe::{self, Incomplete};

pub(in crate::types) struct AttemptAdmission<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

impl ExecutionAdmission for AttemptAdmission<'_> {
    type Error = Incomplete;

    fn admit(&self, work: ExecutionWork) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)?;
        self.db.unwind_if_revision_cancelled();
        let units = match work {
            ExecutionWork::Task { retained_bytes } | ExecutionWork::Resource { retained_bytes } => {
                retained_bytes.saturating_add(1)
            }
            ExecutionWork::Work { units } => units,
            ExecutionWork::Allocation {
                requested_payload_bytes,
            } => requested_payload_bytes,
            ExecutionWork::Poll => 1,
        };
        expansion_probe::charge_work(self.db, units)
    }

    fn scheduling_failure(&self, failure: SchedulingFailure) -> Incomplete {
        Incomplete::Scheduling(failure)
    }

    fn refuse(&self, reason: Incomplete) -> Incomplete {
        expansion_probe::refuse(self.db, reason)
    }
}
