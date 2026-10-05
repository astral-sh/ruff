use super::{ExecutionAdmission, ExecutionWork, RunContext, RunError, RunResult};
use crate::Database;
use crate::attempt_probe::{self, AttemptSupport, ExecutionBudget};

/// The root fixes admission authority; registries carry no independent allowance.
#[derive(Clone, Copy)]
pub(super) enum Admission<'run> {
    Legacy(&'run dyn ExecutionAdmission),
    Budget,
}

impl<'run> Admission<'run> {
    pub(super) fn legacy(
        db: &dyn Database,
        admission: &'run dyn ExecutionAdmission,
    ) -> RunResult<Self> {
        let support = attempt_probe::current().ok_or(RunError::Contract(
            "execution run requires an installed attempt",
        ))?;
        let admission = Self::Legacy(admission);
        admission.check_support(db, &support)?;
        Ok(admission)
    }

    pub(super) fn budget(db: &dyn Database, budget: &ExecutionBudget<'_>) -> RunResult<Self> {
        if !budget.is_current(db) {
            return Err(RunError::Contract(
                "execution budget belongs to another root or worker",
            ));
        }
        Ok(Self::Budget)
    }

    pub(super) fn native_budget(db: &dyn Database, support: &AttemptSupport) -> RunResult<Self> {
        Self::Budget.check_support(db, support)?;
        Ok(Self::Budget)
    }

    fn check_support(self, db: &dyn Database, support: &AttemptSupport) -> RunResult<()> {
        if support.admission_is_budget(db.zalsa()) != Some(matches!(self, Self::Budget)) {
            return Err(RunError::Contract(
                "execution admission does not match the current root",
            ));
        }
        Ok(())
    }

    pub(super) fn admit(self, context: &RunContext<'_>, work: ExecutionWork) -> RunResult<()> {
        self.check_support(context.db, &context.support)?;
        match self {
            Self::Legacy(admission) => admission.admit(work),
            Self::Budget => match work {
                ExecutionWork::Task { requested_bytes }
                | ExecutionWork::Resource { requested_bytes } => {
                    attempt_probe::charge_requested_bytes(context.db, requested_bytes)
                        .map_err(RunError::Refused)
                }
                // Work producers debit through `charge` before reporting; Poll is observation only.
                ExecutionWork::Work { .. } | ExecutionWork::Poll => Ok(()),
            },
        }
    }
}
