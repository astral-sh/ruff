//! Applies the execution allowance to structural work and requested collection payloads.

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{ExecutionWork as RuntimeWork, RunError, RunResult, TaskEndpoint};

use super::{TddControl, TddWork};
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::types::relation::execution::SchedulingFailure;
use crate::types::relation::execution::{ExecutionAdmission, ExecutionWork};

pub(in crate::types) struct ExecutionControl<'a, A: ?Sized> {
    admission: &'a A,
}

impl<'a, A: ExecutionAdmission + ?Sized> ExecutionControl<'a, A> {
    pub(in crate::types) fn new(admission: &'a A) -> Self {
        Self { admission }
    }
}

impl<A: ExecutionAdmission + ?Sized> TddControl for ExecutionControl<'_, A> {
    type Error = A::Error;

    fn admit(&mut self, work: TddWork) -> Result<(), Self::Error> {
        self.admission.admit(ExecutionWork::Work {
            units: work.work_units(),
        })?;
        let requested_payload_bytes = work.requested_payload_bytes();
        if requested_payload_bytes != 0 {
            self.admission.admit(ExecutionWork::Allocation {
                requested_payload_bytes,
            })?;
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct EndpointAdmission<'a, 'run, 'db>(
    pub(in crate::types) &'a TaskEndpoint<'run, 'db>,
);

#[cfg(any(test, feature = "experimental-analysis"))]
impl ExecutionAdmission for EndpointAdmission<'_, '_, '_> {
    type Error = RunError;

    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.0.admit(match work {
            ExecutionWork::Task { retained_bytes } => RuntimeWork::Task {
                requested_bytes: retained_bytes,
            },
            ExecutionWork::Resource { retained_bytes } => RuntimeWork::Resource {
                requested_bytes: retained_bytes,
            },
            ExecutionWork::Allocation {
                requested_payload_bytes,
            } => RuntimeWork::Resource {
                requested_bytes: requested_payload_bytes,
            },
            ExecutionWork::Work { units } => return self.0.admit_work(units),
            ExecutionWork::Poll => RuntimeWork::Poll,
        })
    }

    fn scheduling_failure(&self, failure: SchedulingFailure) -> RunError {
        RunError::Contract(match failure {
            SchedulingFailure::Cancelled => "semantic task was cancelled",
            SchedulingFailure::MissingChild => "semantic task suspended without a child",
            SchedulingFailure::ConcurrentChildren => "semantic task requested concurrent children",
            SchedulingFailure::UnexpectedChild => "semantic task completed with a pending child",
        })
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use super::ExecutionControl;
    use crate::db::tests::setup_db;
    use crate::types::constraints::control::{
        AllocationKind, TableKind, TddControl, TddError, TddWork, hash_access, reserve_vec,
    };
    use crate::types::constructor::expansion_probe::{self, Incomplete};
    use crate::types::relation::execution::attempt::AttemptAdmission;

    #[test]
    fn hash_access_cost_is_independent_of_slots_but_scans_keep_their_cost() {
        let db = setup_db();
        let admission = AttemptAdmission { db: &db };
        for slots in [2, 65_536] {
            let (result, _) = expansion_probe::run(&db, 1, || {
                ExecutionControl::new(&admission).admit(TddWork::HashAccess {
                    table: TableKind::And,
                    slots,
                })
            });
            assert_eq!(result, Ok(Ok(())));
        }
        for (allowance, expected) in [(7, Err(Incomplete::Allowance)), (8, Ok(Ok(())))] {
            let (result, _) = expansion_probe::run(&db, allowance, || {
                ExecutionControl::new(&admission).admit(TddWork::OverlayScan { slots: 8 })
            });
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn repeated_real_hits_exhaust_progress_independently_of_table_width() {
        let db = setup_db();
        for width in [1, 128] {
            let table: FxHashMap<usize, usize> = (0..width).map(|key| (key, key + 1)).collect();
            let admission = AttemptAdmission { db: &db };
            let mut completed = 0;
            let (result, _) = expansion_probe::run(&db, 3, || {
                let mut control = ExecutionControl::new(&admission);
                for _ in 0..8 {
                    hash_access(&mut control, TableKind::Nodes, table.capacity())?;
                    assert_eq!(table.get(&0), Some(&1));
                    completed += 1;
                }
                Ok::<_, TddError<Incomplete>>(())
            });
            assert_eq!(result, Err(Incomplete::Allowance));
            assert_eq!(completed, 3);
            let (retry, _) = expansion_probe::run(&db, 1, || {
                hash_access(
                    &mut ExecutionControl::new(&admission),
                    TableKind::Nodes,
                    table.capacity(),
                )?;
                Ok::<_, TddError<Incomplete>>(table.get(&0).copied())
            });
            assert_eq!(retry, Ok(Ok(Some(1))));
            assert_eq!(table.len(), width);
        }
    }

    #[test]
    fn refused_payload_reservation_leaves_storage_unchanged_and_can_retry() {
        let db = setup_db();
        let admission = AttemptAdmission { db: &db };
        let mut values = Vec::with_capacity(64);
        values.extend([1u64, 2, 3, 4]);
        let before_capacity = values.capacity();
        let additional = before_capacity + 1;

        // The relocation work fits, but the requested backing payload does not. Both checks
        // precede the reservation, so refusal retains the original buffer and its contents.
        let (result, _) = expansion_probe::run(&db, before_capacity, || {
            let mut control = ExecutionControl::new(&admission);
            assert_eq!(
                reserve_vec(
                    &mut values,
                    additional,
                    AllocationKind::Frames,
                    &mut control
                ),
                Err(TddError::Refused(Incomplete::Allowance))
            );
        });
        assert_eq!(result, Err(Incomplete::Allowance));
        assert_eq!(values, [1, 2, 3, 4]);
        assert_eq!(values.capacity(), before_capacity);

        let (result, _) = expansion_probe::run(&db, 10_000, || {
            reserve_vec(
                &mut values,
                additional,
                AllocationKind::Frames,
                &mut ExecutionControl::new(&admission),
            )
        });
        assert_eq!(result, Ok(Ok(())));
        assert_eq!(values, [1, 2, 3, 4]);
        assert!(values.capacity() >= values.len() + additional);
    }
}
