//! Local suspension keeps borrowed work inside its owning task. Only the driver can issue the
//! receipt that permits that task to resume; polling the same future again cannot manufacture it.

use std::any::Any;
use std::cell::Cell;
use std::convert::Infallible;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::{Endpoint, ExecutionWork, PendingTask, Queue, RunError, RunResult, explicit_reads};
use crate::attempt_probe::{self, Incomplete};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PollGeneration(NonZeroUsize);

#[derive(Clone)]
pub(super) struct PollIdentity {
    pub(super) wanted: Rc<Cell<bool>>,
    generation: PollGeneration,
}

impl PollIdentity {
    pub(super) fn is_current(&self, queue: &Queue<'_>) -> bool {
        queue.active_poll.borrow().as_ref().is_some_and(|poll| {
            Rc::ptr_eq(&poll.identity.wanted, &self.wanted) && poll.identity.generation == self.generation
        })
    }
}

pub(super) struct ActivePoll {
    pub(super) identity: PollIdentity,
    resume_checkpoint: Option<PollGeneration>,
    local_requested: bool,
    terminal: Option<TerminalDisposition>,
}

pub(super) enum TerminalDisposition {
    Error(RunError),
    Panic(Box<dyn Any + Send>),
}

impl ActivePoll {
    pub(super) fn has_unconsumed_resume(&self) -> bool {
        self.resume_checkpoint.is_some()
    }

    pub(super) fn requested_checkpoint(&self) -> Option<PollGeneration> {
        if self.local_requested {
            Some(self.identity.generation)
        } else {
            None
        }
    }

    pub(super) fn take_terminal(&mut self) -> Option<TerminalDisposition> {
        self.terminal.take()
    }
}

pub(super) struct PollGuard<'a, 'run> {
    queue: &'a Queue<'run>,
}

impl<'a, 'run> PollGuard<'a, 'run> {
    pub(super) fn enter(
        endpoint: &'a Endpoint<'run, '_>,
        task: &mut PendingTask<'run>,
    ) -> RunResult<Self> {
        let queue = endpoint.queue.as_ref();
        queue.check_runnable()?;
        let generation = queue
            .last_poll_generation
            .get()
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .map(PollGeneration)
            .ok_or(RunError::Contract("execution poll identity exhausted"))?;
        let mut active = queue.active_poll.borrow_mut();
        if active.is_some() {
            return Err(RunError::Contract("nested execution task poll"));
        }
        *active = Some(ActivePoll {
            identity: PollIdentity {
                wanted: task.wanted.clone(),
                generation,
            },
            resume_checkpoint: task.resume_checkpoint.take(),
            local_requested: false,
            terminal: None,
        });
        queue.last_poll_generation.set(generation.0.get());
        Ok(Self { queue })
    }

    pub(super) fn finish(self) -> RunResult<ActivePoll> {
        self.queue.check_runnable()?;
        let poll = self.queue.active_poll.borrow_mut().take();
        poll.ok_or(RunError::Contract("execution poll lost its owner"))
    }

    pub(super) fn take_terminal(&self) -> RunResult<Option<TerminalDisposition>> {
        self.queue.check_runnable()?;
        self.queue
            .active_poll
            .borrow_mut()
            .as_mut()
            .map(ActivePoll::take_terminal)
            .ok_or(RunError::Contract("execution poll lost its owner"))
    }
}

impl Drop for PollGuard<'_, '_> {
    fn drop(&mut self) {
        self.queue.active_poll.borrow_mut().take();
    }
}

#[derive(Clone, Copy)]
enum CheckpointState {
    Fresh,
    Waiting,
    Done,
}

pub(super) struct Checkpoint<'a, 'run, 'db: 'run> {
    endpoint: &'a Endpoint<'run, 'db>,
    identity: PollIdentity,
    state: CheckpointState,
}

/// Expected failure leaves the surrounding future alive until the driver drains its children.
pub(super) struct TerminalFailure<'a, 'run, 'db: 'run> {
    endpoint: &'a Endpoint<'run, 'db>,
    identity: PollIdentity,
    error: RunError,
}

impl Future for TerminalFailure<'_, '_, '_> {
    type Output = RunResult<Infallible>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(error) = self.endpoint.check_terminal_live() {
            return Poll::Ready(Err(error));
        }
        let first = {
            let mut active = self.endpoint.queue.active_poll.borrow_mut();
            let Some(poll) = active.as_mut().filter(|poll| {
                poll.identity.wanted.get()
                    && Rc::ptr_eq(&poll.identity.wanted, &self.identity.wanted)
                    && poll.identity.generation == self.identity.generation
            }) else {
                return Poll::Ready(Err(RunError::Contract(
                    "terminal failure has a stale task poll",
                )));
            };
            if poll.terminal.is_none() {
                poll.terminal = Some(TerminalDisposition::Error(self.error));
                true
            } else {
                false
            }
        };
        if first {
            // Mark the owning query before any cleanup. The exact error stays in ActivePoll;
            // an earlier incomplete reason can differ and must remain the attempt's reason.
            self.endpoint.context.refuse(match self.error {
                RunError::Refused(reason) => reason,
                RunError::Contract(_) | RunError::RequiresFetch | RunError::Preparation(_) => {
                    Incomplete::Interrupted
                }
            });
        }
        Poll::Pending
    }
}

/// Authority captured before callbacks can cancel the revision or abandon their own demand.
/// It authorizes only this queue's current poll; it cannot survive a driver suspension.
pub(super) struct CallbackPoll<'a, 'run, 'db: 'run> {
    endpoint: &'a Endpoint<'run, 'db>,
    identity: PollIdentity,
}

impl CallbackPoll<'_, '_, '_> {
    pub(super) fn identity(&self) -> PollIdentity { self.identity.clone() }

    fn matches(&self, poll: &ActivePoll) -> bool {
        Rc::ptr_eq(&poll.identity.wanted, &self.identity.wanted)
            && poll.identity.generation == self.identity.generation
    }

    pub(super) fn check_completion(&self) -> RunResult<()> {
        explicit_reads::check_acceptance();
        self.endpoint.check_terminal_live()?;
        if let Some(reason) = self
            .endpoint
            .context
            .reason
            .get()
            .or_else(|| self.endpoint.context.support.reason())
        {
            return Err(RunError::Refused(reason));
        }
        let active = self.endpoint.queue.active_poll.borrow();
        let poll = active
            .as_ref()
            .filter(|poll| self.matches(poll))
            .ok_or(RunError::Contract("callback lost its task poll"))?;
        if !poll.identity.wanted.get() || poll.terminal.is_some() {
            return Err(RunError::Contract("callback completed in a stopped task"));
        }
        super::check_completed_poll(poll, &self.endpoint.queue)
    }

    fn can_write_terminal(&self) -> bool {
        self.endpoint.queue.check_runnable().is_ok()
            && self.endpoint.context.support.owns_current_session(self.endpoint.context.db.zalsa())
            && (attempt_probe::stack_depths().0 == 0
                || attempt_probe::current_operation_policy().allows_incomplete())
    }

    pub(super) fn error(&self, error: RunError) -> bool {
        explicit_reads::check_acceptance();
        if !self.can_write_terminal() { return false; }
        let first = {
            let mut active = self.endpoint.queue.active_poll.borrow_mut();
            let Some(poll) = active.as_mut().filter(|poll| self.matches(poll)) else {
                return false;
            };
            if poll.terminal.is_none() {
                poll.terminal = Some(TerminalDisposition::Error(error));
                true
            } else {
                false
            }
        };
        if first {
            self.endpoint.context.refuse(match error {
                RunError::Refused(reason) => reason,
                RunError::Contract(_) | RunError::RequiresFetch | RunError::Preparation(_) => {
                    Incomplete::Interrupted
                }
            });
        }
        true
    }

    pub(super) fn try_panic(&self, payload: Box<dyn Any + Send>) -> Result<(), Box<dyn Any + Send>> {
        if !self.can_write_terminal() { return Err(payload); }
        let mut active = self.endpoint.queue.active_poll.borrow_mut();
        let Some(poll) = active.as_mut().filter(|poll| self.matches(poll)) else {
            return Err(payload);
        };
        if !matches!(poll.terminal, Some(TerminalDisposition::Panic(_))) {
            // A native panic keeps its payload even if the callback first reported refusal.
            // No cancellation or admission callback may intervene before the driver unwinds.
            // Repeated private polling cannot replace the first native payload either.
            poll.terminal = Some(TerminalDisposition::Panic(payload));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn panic(&self, payload: Box<dyn Any + Send>) {
        if let Err(payload) = self.try_panic(payload) {
            std::panic::resume_unwind(payload);
        }
    }
}

impl<'run, 'db: 'run> Endpoint<'run, 'db> {
    pub(super) fn local_call_is_eligible(&self) -> bool {
        self.queue.check_runnable().is_ok()
            // Cancellation of this session must reach the caught native resume check. Only a
            // different session/database makes this endpoint unable to report a local failure.
            && self
                .context
                .support
                .owns_current_session(self.context.db.zalsa())
            && (attempt_probe::stack_depths().0 == 0
                || attempt_probe::current_operation_policy().allows_incomplete())
    }

    pub(super) fn callback_poll(&self) -> Option<CallbackPoll<'_, 'run, 'db>> {
        self.queue.check_runnable().ok()?;
        let identity = self.queue.active_poll.borrow().as_ref()?.identity.clone();
        Some(CallbackPoll {
            endpoint: self,
            identity,
        })
    }
    fn check_terminal_live(&self) -> RunResult<()> {
        self.queue.check_runnable()?;
        // Failure transport must work after refusal and must not invoke cancellation callbacks.
        // An expired endpoint cannot record a failure in another attempt or revision.
        if !self.context.support.is_current(self.context.db.zalsa()) {
            return Err(RunError::Contract(
                "terminal failure has a foreign attempt or revision",
            ));
        }
        if attempt_probe::stack_depths().0 != 0
            && !attempt_probe::current_operation_policy().allows_incomplete()
        {
            return Err(RunError::Contract(
                "terminal failure crossed a complete-only operation",
            ));
        }
        Ok(())
    }

    pub(super) fn suspend_error(
        &self,
        error: RunError,
    ) -> RunResult<TerminalFailure<'_, 'run, 'db>> {
        self.check_terminal_live()?;
        let identity = self
            .queue
            .active_poll
            .borrow()
            .as_ref()
            .filter(|poll| poll.identity.wanted.get())
            .map(|poll| poll.identity.clone())
            .ok_or(RunError::Contract(
                "terminal failure requires an active task poll",
            ))?;
        Ok(TerminalFailure {
            endpoint: self,
            identity,
            error,
        })
    }

    fn check_live(&self) -> RunResult<()> {
        let access = self.queue.begin_access()?;
        self.context.check_resume()?;
        access.check()
    }

    fn progress_error<T>(&self, message: &'static str) -> RunResult<T> {
        // An expired endpoint must not report into a later, unrelated attempt.
        self.check_live()?;
        self.context.observe(Err(RunError::Contract(message)))
    }

    pub(super) fn checkpoint(&self) -> RunResult<Checkpoint<'_, 'run, 'db>> {
        self.check_live()?;
        let identity = self
            .queue
            .active_poll
            .borrow()
            .as_ref()
            .filter(|poll| poll.identity.wanted.get())
            .map(|poll| poll.identity.clone());
        let Some(identity) = identity else {
            return self.progress_error("checkpoint requires an active task poll");
        };
        Ok(Checkpoint {
            endpoint: self,
            identity,
            state: CheckpointState::Fresh,
        })
    }

    /// Checks a completed local computation without charging or scheduling more work.
    pub(super) fn check_completion(&self) -> RunResult<()> {
        self.check_live()?;
        let clean = self
            .queue
            .active_poll
            .borrow()
            .as_ref()
            .is_some_and(|poll| {
                poll.identity.wanted.get()
                    && poll.resume_checkpoint.is_none()
                    && !poll.local_requested
            })
            && self.queue.pending.borrow().is_empty();
        if !clean {
            return self.progress_error("local completion has pending work or no active task");
        }
        Ok(())
    }
}

impl Checkpoint<'_, '_, '_> {
    fn can_issue(&self) -> bool {
        self.endpoint.queue.check_runnable().is_ok()
            && self.endpoint
            .queue
            .active_poll
            .borrow()
            .as_ref()
            .is_some_and(|poll| {
                Rc::ptr_eq(&poll.identity.wanted, &self.identity.wanted)
                    && poll.identity.wanted.get()
                    && poll.identity.generation == self.identity.generation
                    && poll.resume_checkpoint.is_none()
                    && !poll.local_requested
            })
            && self.endpoint.queue.pending.borrow().is_empty()
    }

    fn advance(&mut self) -> RunResult<Poll<()>> {
        let access = self.endpoint.queue.begin_access()?;
        self.endpoint.check_live()?;
        access.check()?;
        match self.state {
            CheckpointState::Fresh => {
                if !self.can_issue() {
                    return self
                        .endpoint
                        .progress_error("checkpoint has a stale or busy owner");
                }
                self.endpoint.admit(ExecutionWork::Work { units: 1 })?;
                access.check()?;
                // Admission can execute user code. Recheck before installing a request, and
                // never retain an active-poll or child-queue borrow across that callback.
                if !self.can_issue() {
                    return self
                        .endpoint
                        .progress_error("checkpoint owner changed during admission");
                }
                let issued = self
                    .endpoint
                    .queue
                    .active_poll
                    .borrow_mut()
                    .as_mut()
                    .map(|poll| {
                        poll.local_requested = true;
                    });
                if issued.is_none() {
                    return self
                        .endpoint
                        .progress_error("checkpoint lost its active task");
                }
                self.state = CheckpointState::Waiting;
                Ok(Poll::Pending)
            }
            CheckpointState::Waiting => {
                let acknowledged = {
                    let mut active = self.endpoint.queue.active_poll.borrow_mut();
                    if let Some(poll) = active.as_mut()
                        && Rc::ptr_eq(&poll.identity.wanted, &self.identity.wanted)
                        && poll.identity.wanted.get()
                        && poll.identity.generation.0 > self.identity.generation.0
                        && poll.resume_checkpoint == Some(self.identity.generation)
                        && !poll.local_requested
                    {
                        poll.resume_checkpoint = None;
                        true
                    } else {
                        false
                    }
                };
                if !acknowledged {
                    return self
                        .endpoint
                        .progress_error("checkpoint has no matching resume receipt");
                }
                self.state = CheckpointState::Done;
                Ok(Poll::Ready(()))
            }
            CheckpointState::Done => self.endpoint.progress_error("checkpoint already completed"),
        }
    }
}

impl Future for Checkpoint<'_, '_, '_> {
    type Output = RunResult<()>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.advance() {
            Ok(progress) => progress.map(Ok),
            Err(error) => {
                self.state = CheckpointState::Done;
                Poll::Ready(Err(error))
            }
        }
    }
}

#[cfg(test)]
pub(super) fn snapshot(queue: &Queue<'_>) -> Option<(usize, usize, bool, bool, bool)> {
    queue.active_poll.borrow().as_ref().map(|poll| (
        Rc::as_ptr(&poll.identity.wanted).addr(),
        poll.identity.generation.0.get(),
        poll.local_requested,
        poll.resume_checkpoint.is_some(),
        poll.terminal.is_some(),
    ))
}
