use std::future::ready;

use super::callback::CallbackOwner;
use super::registration::TaskEndpoint;
use super::{ExecutionWork, RunContext, RunError, RunOperation, RunResult, callback};
#[cfg(test)]
use super::{TraceEvent, record_validation_trace};
use crate::DatabaseKeyIndex;
use crate::attempt_probe::{self, Incomplete, LocalOwnershipReceipt};
use crate::function::Configuration;
use crate::function::fetch::selection::{ColdCycleInitial, ColdCycleReady};
use crate::function::maybe_changed_after::validation::{Verification, VerifiedMemo};
use crate::sync::thread;
use crate::zalsa_local::ActiveQueryGuard;

/// Fetch and validation may claim a query without pushing its frame. The current caller remains owned
/// by an ancestor task; a root request has no caller at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Caller {
    pub(super) key: Option<DatabaseKeyIndex>,
    pub(super) depth: usize,
}

impl Caller {
    pub(super) fn for_active_query(query: &ActiveQueryGuard<'_>, depth: usize) -> Self {
        Self {
            key: Some(query.database_key_index),
            depth,
        }
    }

    pub(super) fn capture(operation: &RunOperation<'_>) -> RunResult<Self> {
        Self::capture_context(&operation.context)
    }

    fn capture_context(context: &RunContext<'_>) -> RunResult<Self> {
        Self::capture_db(context.db)
    }

    pub(super) fn capture_db(db: &dyn crate::Database) -> RunResult<Self> {
        db.zalsa_local()
            .try_with_query_stack(|stack| Self {
                key: stack.last().map(|query| query.database_key_index),
                depth: stack.len(),
            })
            .ok_or(RunError::Contract(
                "query stack is borrowed while suspending a frame-free query",
            ))
    }

    pub(super) fn is_current(self, operation: &RunOperation<'_>) -> bool {
        operation.is_current() && self.is_current_context(&operation.context)
    }

    fn is_current_context(self, context: &RunContext<'_>) -> bool {
        self.is_current_db(context.db)
    }

    pub(super) fn is_current_db(self, db: &dyn crate::Database) -> bool {
        db.zalsa_local().try_with_query_stack(|stack| {
            stack.len() == self.depth
                && stack.last().map(|query| query.database_key_index) == self.key
        }) == Some(true)
    }

    pub(super) fn check_resume(self, operation: &RunOperation<'_>) -> RunResult<()> {
        operation.context.check_resume()?;
        if !self.is_current(operation) {
            return Err(RunError::Contract(
                "frame-free query resumed beneath its child",
            ));
        }
        Ok(())
    }
}

/// Entry and fixed leaves borrow their actual enclosing scope without installing an operation.
pub(super) struct EntryScope<'a, 'db> {
    context: &'a RunContext<'db>,
    caller: Caller,
    operation_depth: usize,
    last_operation: Option<usize>,
    attempt_depths: (usize, usize),
    ownership: LocalOwnershipReceipt,
}

impl<'a, 'db> EntryScope<'a, 'db> {
    pub(super) fn capture(context: &'a RunContext<'db>) -> RunResult<Self> {
        let caller = Caller::capture_context(context)?;
        let ownership = context
            .support
            .local_ownership(context.db.zalsa())
            .ok_or(RunError::Contract("execution entry has a foreign attempt"))?;
        let operations = context
            .operations
            .try_borrow()
            .map_err(|_| RunError::Contract("execution operation stack is borrowed"))?;
        Ok(Self {
            context,
            caller,
            operation_depth: operations.len(),
            last_operation: operations.last().copied(),
            attempt_depths: attempt_probe::stack_depths(),
            ownership,
        })
    }
}

impl CallbackOwner for EntryScope<'_, '_> {
    fn check_resume(&self) -> RunResult<()> {
        self.context.check_resume()?;
        if !self.is_current() {
            return Err(RunError::Contract(
                "execution entry changed its enclosing scope",
            ));
        }
        Ok(())
    }

    fn is_current(&self) -> bool {
        self.context
            .operations
            .try_borrow()
            .is_ok_and(|operations| {
                operations.len() == self.operation_depth
                    && operations.last().copied() == self.last_operation
            })
            && self.caller.is_current_context(self.context)
            && attempt_probe::stack_depths() == self.attempt_depths
            && self
                .context
                .support
                .local_ownership(self.context.db.zalsa())
                == Some(self.ownership)
    }
}

pub(super) async fn enter_operation<'db, C: Configuration>(
    endpoint: &TaskEndpoint<'_, 'db>,
    policy: crate::attempt_probe::OperationPolicy,
) -> (RunOperation<'db>, Caller) {
    let prepared = {
        let scope = match EntryScope::capture(&endpoint.inner.context) {
            Ok(scope) => scope,
            Err(error) => match callback::reject(&endpoint.inner, error, ()).await {},
        };
        callback::complete(
            &endpoint.inner,
            &scope,
            callback::CallbackKind::Canonical,
            || {
                ready(
                    endpoint
                        .admit(ExecutionWork::Resource {
                            requested_bytes: 4 * size_of::<usize>(),
                        })
                        .and_then(|()| RunOperation::prepare::<C>(&endpoint.inner.context, policy)),
                )
            },
        )
        .await
    };
    let operation =
        match RunOperation::enter_admitted::<C>(endpoint.inner.context.clone(), prepared) {
            Ok(operation) => operation,
            Err(error) => match callback::reject(&endpoint.inner, error, ()).await {},
        };
    let caller = match Caller::capture(&operation) {
        Ok(caller) => caller,
        Err(error) => match callback::reject(&endpoint.inner, error, operation).await {},
    };
    (operation, caller)
}

/// Preserve the existing Work admission and following caller check under one retained owner.
pub(super) async fn admit_phase(endpoint: &TaskEndpoint<'_, '_>, owner: &impl CallbackOwner) {
    callback::complete(
        &endpoint.inner,
        owner,
        callback::CallbackKind::Canonical,
        || {
            ready(
                endpoint
                    .admit(ExecutionWork::Work { units: 1 })
                    .and_then(|()| owner.check_resume()),
            )
        },
    )
    .await;
}

const WAIT_RETRY_WORK: usize = 1;

/// Keep the request outside the callback while the canonical wait consumes its own locks.
pub(super) async fn wait_for_query<'call, 'run: 'call, 'db: 'run, O>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    owner: &'call O,
    running: crate::runtime::Running<'db>,
) where
    O: callback::CallbackOwner + ?Sized,
{
    callback::complete(
        &endpoint.inner,
        owner,
        callback::CallbackKind::Canonical,
        || {
            // Completion and peer cancellation both require a new canonical probe. All wait
            // guards retire before cancellation callbacks or the participant's retry debit.
            let _ = running.block_on(endpoint.inner.context.db.zalsa());
            ready(
                owner
                    .check_resume()
                    .and_then(|()| endpoint.admit_work(WAIT_RETRY_WORK)),
            )
        },
    )
    .await;
}

/// Borrowed callouts may mutate their request while the wrapper retains the actual guards.
/// This view checks the original stack position; it owns no frame or claim of its own.
pub(super) struct CallbackScope<'a, 'db> {
    operation: &'a RunOperation<'db>,
    caller: Caller,
}

impl<'a, 'db> CallbackScope<'a, 'db> {
    pub(super) fn new(operation: &'a RunOperation<'db>, caller: Caller) -> Self {
        Self { operation, caller }
    }
}

impl callback::CallbackOwner for CallbackScope<'_, '_> {
    fn check_resume(&self) -> RunResult<()> {
        self.caller.check_resume(self.operation)
    }

    fn is_current(&self) -> bool {
        self.caller.is_current(self.operation)
    }
}

pub(super) trait FrameFreeRequest {
    fn abort(self);

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller);
}

#[cfg(test)]
pub(super) fn trace_owner(
    phase: &'static str,
    key: Option<DatabaseKeyIndex>,
    operation: &RunOperation<'_>,
    caller: Caller,
) {
    record_validation_trace(TraceEvent::Outer {
        phase,
        key,
        operation: operation.ordinal,
        owner: caller.key,
        query_depth: caller.depth,
        operation_depth: crate::attempt_probe::stack_depths().0,
    });
}

pub(super) struct OwnedFrameFree<'a, 'db, Q: FrameFreeRequest> {
    step: Option<Q>,
    operation: &'a RunOperation<'db>,
    caller: Caller,
}

impl<'a, 'db, Q: FrameFreeRequest> OwnedFrameFree<'a, 'db, Q> {
    pub(super) fn new(step: Q, operation: &'a RunOperation<'db>, caller: Caller) -> Self {
        Self {
            step: Some(step),
            operation,
            caller,
        }
    }

    pub(super) fn take_admitted(self) -> Result<Q, (RunError, Self)> {
        #[cfg(test)]
        let (operation, caller) = (self.operation, self.caller);
        let step = self.into_admitted()?;
        #[cfg(test)]
        step.trace(operation, caller);
        Ok(step)
    }

    /// Transfer an accepted request without another callback or phase trace.
    pub(super) fn into_admitted(mut self) -> Result<Q, (RunError, Self)> {
        if !callback::CallbackOwner::is_current(&self) {
            return Err((
                RunError::Contract("frame-free query resumed beneath its child"),
                self,
            ));
        }
        match self.step.take() {
            Some(step) => Ok(step),
            None => Err((
                RunError::Contract("frame-free request already consumed"),
                self,
            )),
        }
    }

    pub(super) fn admitted(&self) -> RunResult<&Q> {
        self.step
            .as_ref()
            .ok_or(RunError::Contract("frame-free request already consumed"))
    }
    /// The caller has already admitted this phase and checked the current owner.
    #[cfg(test)]
    pub(super) fn admitted_mut(&mut self) -> RunResult<&mut Q> {
        self.step
            .as_mut()
            .ok_or(RunError::Contract("frame-free request already consumed"))
    }

    pub(super) fn split(&mut self) -> RunResult<(&mut Q, CallbackScope<'a, 'db>)> {
        let step = self
            .step
            .as_mut()
            .ok_or(RunError::Contract("frame-free request already consumed"))?;
        Ok((step, CallbackScope::new(self.operation, self.caller)))
    }

    #[cfg(test)]
    pub(super) fn trace(&self) {
        if let Some(step) = &self.step {
            step.trace(self.operation, self.caller);
        }
    }
}

impl<Q: FrameFreeRequest> callback::CallbackOwner for OwnedFrameFree<'_, '_, Q> {
    fn check_resume(&self) -> RunResult<()> {
        self.caller.check_resume(self.operation)
    }

    fn is_current(&self) -> bool {
        self.step.is_some() && self.caller.is_current(self.operation)
    }
}

impl<'db> OwnedFrameFree<'_, 'db, Verification<'db>> {
    pub(super) fn complete_admitted(mut self) -> Result<VerifiedMemo<'db>, (RunError, Self)> {
        if !self.is_current() {
            return Err((
                RunError::Contract("verification completed beneath its child"),
                self,
            ));
        }
        let Some(verification) = self.step.take() else {
            return Err((
                RunError::Contract("frame-free request already consumed"),
                self,
            ));
        };
        match verification.complete() {
            Ok(verified) => Ok(verified),
            Err(verification) => {
                // Restore the claim before returning through the owner's marked-abort path.
                self.step = Some(verification);
                Err((RunError::Contract("verification has not completed"), self))
            }
        }
    }
}

impl<Q: FrameFreeRequest> Drop for OwnedFrameFree<'_, '_, Q> {
    fn drop(&mut self) {
        let Some(step) = self.step.take() else {
            return;
        };
        if thread::panicking() {
            // No query frame was pushed here. Existing claim destructors preserve panic release.
            #[cfg(test)]
            trace_owner("panic.drop", None, self.operation, self.caller);
            drop(step);
            return;
        }
        assert!(
            self.caller.is_current(self.operation),
            "aborting frame-free query beneath its child"
        );
        self.operation.context.refuse(Incomplete::Interrupted);
        #[cfg(test)]
        trace_owner("abort.marked", None, self.operation, self.caller);
        step.abort();
        #[cfg(test)]
        trace_owner("abort.released", None, self.operation, self.caller);
    }
}

impl<'a, 'db, C: Configuration> OwnedFrameFree<'a, 'db, ColdCycleInitial<'db, C>> {
    /// Transfer an accepted callback output without another admission or cancellation callback.
    pub(super) fn returned(
        mut self,
        value: C::Output<'db>,
    ) -> Result<OwnedFrameFree<'a, 'db, ColdCycleReady<'db, C>>, (RunError, C::Output<'db>, Self)>
    {
        let Some(request) = self.step.take() else {
            return Err((
                RunError::Contract("cold initializer already consumed"),
                value,
                self,
            ));
        };
        Ok(OwnedFrameFree::new(
            request.returned(value),
            self.operation,
            self.caller,
        ))
    }
}
