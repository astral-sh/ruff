//! Borrowed constraint operations on the query runtime's current task.

use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::control::TddError;
pub(super) use super::control::attempt::EndpointAdmission;
use super::control::attempt::ExecutionControl;
use super::{
    ConstraintCombination, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::Db;
use crate::types::constructor::expansion_probe::{self, Incomplete};

pub(super) mod satisfaction;
mod signature;
pub(in crate::types) use satisfaction::UnsupportedSatisfactionOperation;

#[cfg(test)]
mod tests;

/// The caller keeps its original builder and local fold while this component drains local work.
/// A completed push may already be accepted when the final completion check fails; that failure
/// requires discarding the enclosing fold, rather than retrying its last input on the same fold.
pub(in crate::types) struct RuntimeStructural<'run, 'db: 'run> {
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    #[cfg(test)]
    observer:
        Option<&'run dyn Fn(&TaskEndpoint<'run, 'db>, tests::StructuralBoundary) -> RunResult<()>>,
    #[cfg(test)]
    cursor_lifetime: Option<&'run tests::CursorLifetime>,
}

impl<'run, 'db: 'run> RuntimeStructural<'run, 'db> {
    // Construct inside the registered run, using the database that created its registry.
    pub(in crate::types) fn new(db: &'db dyn Db, endpoint: TaskEndpoint<'run, 'db>) -> Self {
        Self {
            db,
            endpoint,
            #[cfg(test)]
            observer: None,
            #[cfg(test)]
            cursor_lifetime: None,
        }
    }

    pub(in crate::types) fn combine<'c>(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> impl std::future::Future<Output = RunResult<ConstraintSet<'db, 'c>>> {
        self.combine_with_entry(builder, kind, left, right, || Ok(()))
    }

    pub(in crate::types) async fn combine_with_entry<'c>(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
        enter: impl FnOnce() -> RunResult<()>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let admission = EndpointAdmission(&self.endpoint);
        let mut control = ExecutionControl::new(&admission);
        let cursor = self
            .endpoint
            .local_call(|| {
                enter()?;
                Ok(ConstraintCombination::new(builder, kind, left, right))
            })
            .await;
        #[cfg(test)]
        let cursor = tests::ObservedCursor::new(cursor, self.cursor_lifetime);
        let mut cursor = Some(cursor);
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    let cursor = cursor
                        .as_mut()
                        .ok_or(RunError::Contract("local cursor already retired"))?;
                    let progress = cursor
                        .advance_with(&mut control)
                        .map_err(|error| self.error(error))?;
                    #[cfg(test)]
                    self.observe(tests::StructuralBoundary::AfterAdvanceReturned {
                        operation: tests::StructuralOperation::Combine,
                        outer_complete: progress.is_break(),
                    })?;
                    Ok(progress)
                })
                .await;
            match progress {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(result) => {
                    self.endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            #[cfg(test)]
                            self.observe(tests::StructuralBoundary::BeforeRetirementAcceptance {
                                operation: tests::StructuralOperation::Combine,
                            })?;
                            Ok(())
                        })
                        .await;
                    return Ok(result);
                }
            }
        }
    }

    pub(in crate::types) fn push<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> impl std::future::Future<Output = RunResult<ControlFlow<ConstraintSet<'db, 'c>>>> {
        self.push_with_entry(fold, next, |_| Ok(()))
    }

    pub(in crate::types) async fn push_with_entry<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
        enter: impl FnOnce(&ConstraintFold<'db, 'c>) -> RunResult<()>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        let admission = EndpointAdmission(&self.endpoint);
        let mut control = ExecutionControl::new(&admission);
        let cursor = self
            .endpoint
            .local_call(|| {
                enter(fold)?;
                Ok(fold.begin_push(next))
            })
            .await;
        #[cfg(test)]
        let cursor = tests::ObservedCursor::new(cursor, self.cursor_lifetime);
        let mut cursor = Some(cursor);
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    let cursor = cursor
                        .as_mut()
                        .ok_or(RunError::Contract("local cursor already retired"))?;
                    let progress = cursor
                        .advance_with(&mut control)
                        .map_err(|error| self.error(error))?;
                    #[cfg(test)]
                    self.observe(tests::StructuralBoundary::AfterAdvanceReturned {
                        operation: tests::StructuralOperation::Push,
                        outer_complete: progress.is_break(),
                    })?;
                    Ok(progress)
                })
                .await;
            match progress {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(result) => {
                    self.endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            #[cfg(test)]
                            self.observe(tests::StructuralBoundary::BeforeRetirementAcceptance {
                                operation: tests::StructuralOperation::Push,
                            })?;
                            Ok(())
                        })
                        .await;
                    return Ok(result);
                }
            }
        }
    }

    pub(in crate::types) fn finish<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> impl std::future::Future<Output = RunResult<ConstraintSet<'db, 'c>>> {
        self.finish_with_entry(fold, |_| Ok(()))
    }

    pub(in crate::types) async fn finish_with_entry<'c>(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        enter: impl FnOnce(&ConstraintFold<'db, 'c>) -> RunResult<()>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let admission = EndpointAdmission(&self.endpoint);
        let mut control = ExecutionControl::new(&admission);
        let cursor = self
            .endpoint
            .local_call(|| {
                enter(fold)?;
                Ok(fold.begin_finish())
            })
            .await;
        #[cfg(test)]
        let cursor = tests::ObservedCursor::new(cursor, self.cursor_lifetime);
        let mut cursor = Some(cursor);
        loop {
            let progress = self
                .endpoint
                .local_call(|| {
                    let cursor = cursor
                        .as_mut()
                        .ok_or(RunError::Contract("local cursor already retired"))?;
                    let progress = cursor
                        .advance_with(&mut control)
                        .map_err(|error| self.error(error))?;
                    #[cfg(test)]
                    self.observe(tests::StructuralBoundary::AfterAdvanceReturned {
                        operation: tests::StructuralOperation::Finish,
                        outer_complete: progress.is_break(),
                    })?;
                    Ok(progress)
                })
                .await;
            match progress {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(result) => {
                    self.endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            #[cfg(test)]
                            self.observe(tests::StructuralBoundary::BeforeRetirementAcceptance {
                                operation: tests::StructuralOperation::Finish,
                            })?;
                            Ok(())
                        })
                        .await;
                    return Ok(result);
                }
            }
        }
    }

    #[cfg(test)]
    fn observe(&self, boundary: tests::StructuralBoundary) -> RunResult<()> {
        if let Some(observer) = self.observer {
            observer(&self.endpoint, boundary)?;
        }
        Ok(())
    }

    fn error(&self, error: TddError<RunError>) -> RunError {
        match error {
            TddError::Refused(error) => error,
            TddError::CapacityExhausted => {
                let reason =
                    expansion_probe::refuse(self.db, Incomplete::ConstraintCapacityExhausted);
                RunError::Refused(match reason {
                    Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
                    Incomplete::RequestedAllocation => {
                        salsa::attempt_probe::Incomplete::RequestedAllocation
                    }
                    _ => salsa::attempt_probe::Incomplete::Interrupted,
                })
            }
        }
    }
}
