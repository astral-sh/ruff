//! Structural producers run with their own query stack while semantic tasks retain their claims.
//! The protected callback restores every semantic owner before it transports a failure to the driver.

use super::progress::ActivePoll;
use super::{
    Endpoint, ExecutionWork, Queue, RunError, RunResult, callback, explicit_reads, native_source,
};
use crate::attempt_probe::StructuralPreparation;
use crate::prepared_source_probe::Stamp;
use crate::zalsa_local::ParkedQueryStack;

struct ParkedPoll<'queue, 'run> {
    queue: &'queue Queue<'run>,
    poll: Option<ActivePoll>,
    epoch: usize,
}

impl<'queue, 'run> ParkedPoll<'queue, 'run> {
    fn enter(endpoint: &'queue Endpoint<'run, '_>) -> RunResult<Self> {
        let receipt = endpoint.callback_poll().ok_or(RunError::Contract(
            "structural preparation requires an active task poll",
        ))?;
        receipt.check_completion()?;
        let queue = endpoint.queue.as_ref();
        let epoch = queue
            .pause_epoch
            .get()
            .checked_add(1)
            .ok_or(RunError::Contract(
                "structural preparation pause identity exhausted",
            ))?;
        let poll = queue
            .active_poll
            .try_borrow_mut()
            .map_err(|_| RunError::Contract("structural preparation has a borrowed poll"))?
            .take();
        if poll.is_none() {
            return Err(RunError::Contract("structural preparation lost its poll"));
        }
        queue.pause_epoch.set(epoch);
        queue.paused.set(true);
        Ok(Self { queue, poll, epoch })
    }

    fn check_finished(&self) -> RunResult<()> {
        if !self.queue.paused.get()
            || self.queue.pause_epoch.get() != self.epoch
            || native_source::current_driver() != Some(self.queue.driver)
            || self
                .queue
                .active_poll
                .try_borrow()
                .map_or(true, |poll| poll.is_some())
            || self
                .queue
                .pending
                .try_borrow()
                .map_or(true, |pending| !pending.is_empty())
        {
            return Err(RunError::Contract(
                "structural preparation changed its suspended driver",
            ));
        }
        Ok(())
    }
}

impl Drop for ParkedPoll<'_, '_> {
    fn drop(&mut self) {
        *self.queue.active_poll.borrow_mut() = self.poll.take();
        self.queue.paused.set(false);
    }
}

pub(super) async fn prepare<'call, 'run: 'call, 'db: 'run, T, M>(
    endpoint: &'call Endpoint<'run, 'db>,
    make: M,
) -> T
where
    M: FnOnce() -> RunResult<T> + 'call,
    T: 'call,
{
    // Cancellation or a failed restoration check must leave the returned value owned by the
    // suspended future. Its destructor can then run under the driver's normal cleanup rules.
    let mut returned = None;
    callback::local_call(endpoint, || {
        let context = &endpoint.context;
        if context.native_entry.is_some()
            || context.initial_query_depth != 0
            || context.initial_depths != (0, 0)
        {
            return Err(RunError::Contract(
                "structural preparation requires a root execution driver",
            ));
        }
        // Each guard extracts saved state, passes through a helper result, binds locally, checks
        // completion and restores its state. Prepay those five fixed operations for all four guards.
        // Transfers move collection headers without visiting retained semantic entries. Include the
        // full helper results, their error-converted results and three guard-width transfers in the
        // separate byte quotation. The two error conversions also have one work unit each.
        // Structural producers retain their ordinary allocation policy; no new semantic task or
        // budget is created here.
        endpoint.admit_work(4 * 5 + 2)?;
        let requested_bytes = size_of::<ParkedPoll>()
            .checked_add(size_of::<StructuralPreparation>())
            .and_then(|bytes| bytes.checked_add(size_of::<ParkedQueryStack>()))
            .and_then(|bytes| bytes.checked_add(size_of::<explicit_reads::ParkedBoundary>()))
            .and_then(|bytes| bytes.checked_mul(3))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<ParkedPoll>>()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<Result<StructuralPreparation, &'static str>>())
            })
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<StructuralPreparation>>()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<Result<ParkedQueryStack, &'static str>>())
            })
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<ParkedQueryStack>>()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<RunResult<explicit_reads::ParkedBoundary>>())
            })
            .ok_or(RunError::Contract(
                "structural preparation carrier size overflow",
            ))?;
        endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
        let stamp = Stamp::current(context.db);
        let poll = ParkedPoll::enter(endpoint)?;
        let attempt = StructuralPreparation::park(context.db, &context.support)
            .map_err(RunError::Contract)?;
        let queries = context
            .db
            .zalsa_local()
            .park_for_structural_preparation()
            .map_err(RunError::Contract)?;
        let boundary = explicit_reads::ParkedBoundary::enter()?;

        context.db.unwind_if_revision_cancelled();
        returned = Some(make());
        context.db.unwind_if_revision_cancelled();
        if !stamp.belongs_to(context.db) {
            return Err(RunError::Contract(
                "structural preparation changed its database stamp",
            ));
        }
        boundary.check_finished()?;
        queries.check_finished().map_err(RunError::Contract)?;
        attempt.check_complete().map_err(RunError::Contract)?;
        poll.check_finished()?;
        drop(boundary);
        drop(queries);
        drop(attempt);
        drop(poll);
        match returned.as_ref() {
            Some(Ok(_)) => Ok(()),
            Some(Err(error)) => Err(*error),
            None => Err(RunError::Contract("structural preparation lost its result")),
        }
    })
    .await;
    callback::local_call(endpoint, || {
        returned
            .take()
            .ok_or(RunError::Contract("structural preparation lost its result"))?
    })
    .await
}
