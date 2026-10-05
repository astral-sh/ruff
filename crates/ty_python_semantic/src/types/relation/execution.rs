//! Depth-first execution of semantic operations with independently owned constraint builders.
//!
//! Each child retains its typed reply inside a task. Only operational completion is erased,
//! so a root relation and a nested call can share the stack without exchanging constraint IDs.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

pub(in crate::types) mod resources;

#[cfg(test)]
pub(in crate::types) mod attempt;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SchedulingFailure {
    Cancelled,
    MissingChild,
    ConcurrentChildren,
    UnexpectedChild,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ExecutionWork {
    Task { retained_bytes: usize },
    Resource { retained_bytes: usize },
    Allocation { requested_payload_bytes: usize },
    Work { units: usize },
    Poll,
}

/// Admission precedes allocation and polling; semantic dependencies make their own admissions.
pub(in crate::types) trait ExecutionAdmission {
    type Error;

    fn admit(&self, work: ExecutionWork) -> Result<(), Self::Error>;

    fn scheduling_failure(&self, failure: SchedulingFailure) -> Self::Error;

    fn refuse(&self, reason: Self::Error) -> Self::Error {
        reason
    }
}

type Task<'run, E> = Pin<Box<dyn Future<Output = Result<(), E>> + 'run>>;

struct PendingTask<'run, E> {
    task: Task<'run, E>,
    active: Rc<Cell<bool>>,
}

struct Queue<'run, E> {
    pending: RefCell<Vec<PendingTask<'run, E>>>,
    cancelled: Cell<bool>,
}

/// Endpoints enqueue children; the driver alone polls them and owns their active stack.
pub(in crate::types) struct TaskEndpoint<'run, E> {
    queue: Rc<Queue<'run, E>>,
    admission: &'run dyn ExecutionAdmission<Error = E>,
}

impl<E> Clone for TaskEndpoint<'_, E> {
    fn clone(&self) -> Self {
        Self {
            queue: Rc::clone(&self.queue),
            admission: self.admission,
        }
    }
}

struct Reply<T, E> {
    answer: Option<Result<T, E>>,
}

async fn completing_task<T, E, F: Future<Output = Result<T, E>>, M: FnOnce() -> F>(
    make: M,
    active: Rc<Cell<bool>>,
    reply: Rc<RefCell<Reply<T, E>>>,
) -> Result<(), E> {
    if !active.get() {
        return Ok(());
    }
    let result = make().await;
    if !active.get() {
        return Ok(());
    }
    // Operational failure stops the driver before it can resume any parent.
    let value = result?;
    reply.borrow_mut().answer = Some(Ok(value));
    Ok(())
}

fn result_size<A, B, C, R>(_factory: impl FnOnce(A, B, C) -> R) -> usize {
    size_of::<R>()
}

pub(in crate::types) struct Demand<T, E> {
    reply: Rc<RefCell<Reply<T, E>>>,
    active: Rc<Cell<bool>>,
}

impl<T, E> Future for Demand<T, E> {
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.reply.borrow_mut().answer.take() {
            Some(result) => Poll::Ready(result),
            None => Poll::Pending,
        }
    }
}

impl<T, E> Drop for Demand<T, E> {
    fn drop(&mut self) {
        self.active.set(false);
    }
}

impl<'run, E: 'run> TaskEndpoint<'run, E> {
    pub(in crate::types) fn demand<
        T: 'run,
        F: Future<Output = Result<T, E>> + 'run,
        M: FnOnce() -> F + 'run,
    >(
        &self,
        make: M,
    ) -> Result<Demand<T, E>, E> {
        if self.queue.cancelled.get() {
            return Err(self
                .admission
                .scheduling_failure(SchedulingFailure::Cancelled));
        }
        // Measure the wrapper without constructing it or calling the semantic future factory.
        // The two vectors initially allocate at least four entries each; the per-child charge
        // also covers their subsequent geometric growth. Semantic allocations are separate.
        let retained_bytes = result_size(completing_task::<T, E, F, M>)
            .saturating_add(size_of::<RefCell<Reply<T, E>>>())
            .saturating_add(size_of::<Cell<bool>>())
            .saturating_add(8 * size_of::<PendingTask<'run, E>>())
            .saturating_add(4 * size_of::<usize>())
            .saturating_add(align_of::<RefCell<Reply<T, E>>>().max(align_of::<usize>()))
            .saturating_add(align_of::<usize>());
        self.admission
            .admit(ExecutionWork::Task { retained_bytes })?;
        let active = Rc::new(Cell::new(true));
        let reply = Rc::new(RefCell::new(Reply { answer: None }));
        let task = Box::pin(completing_task(make, Rc::clone(&active), Rc::clone(&reply)));
        self.queue.pending.borrow_mut().push(PendingTask {
            task,
            active: Rc::clone(&active),
        });
        Ok(Demand { reply, active })
    }

    pub(in crate::types) fn admit_resource<T>(&self) -> Result<(), E> {
        if self.queue.cancelled.get() {
            return Err(self
                .admission
                .scheduling_failure(SchedulingFailure::Cancelled));
        }
        // Arena chunks double and retain old chunks; charge spare slots and chunk headers
        // before insertion as well as the initialized value itself.
        self.admission.admit(ExecutionWork::Resource {
            retained_bytes: size_of::<T>()
                .saturating_mul(4)
                .saturating_add(size_of::<Vec<T>>().saturating_mul(4)),
        })
    }
}

/// A single run owns cancellation. Futures only retain endpoints, never this driver itself.
pub(in crate::types) struct TaskDriver<'run, E> {
    endpoint: TaskEndpoint<'run, E>,
    stack: Vec<PendingTask<'run, E>>,
}

impl<'run, E: 'run> TaskDriver<'run, E> {
    pub(in crate::types) fn new(
        admission: &'run dyn ExecutionAdmission<Error = E>,
    ) -> Result<Self, E> {
        admission.admit(ExecutionWork::Resource {
            retained_bytes: size_of::<Queue<'run, E>>().saturating_add(2 * size_of::<usize>()),
        })?;
        Ok(Self {
            endpoint: TaskEndpoint {
                queue: Rc::new(Queue {
                    pending: RefCell::new(Vec::new()),
                    cancelled: Cell::new(false),
                }),
                admission,
            },
            stack: Vec::new(),
        })
    }

    pub(in crate::types) fn endpoint(&self) -> TaskEndpoint<'run, E> {
        self.endpoint.clone()
    }

    pub(in crate::types) fn run<T: 'run, F: Future<Output = Result<T, E>> + 'run>(
        mut self,
        make_root: impl FnOnce() -> F + 'run,
    ) -> Result<T, E> {
        self.run_inner(make_root)
            .map_err(|reason| self.endpoint.admission.refuse(reason))
    }

    fn run_inner<T: 'run, F: Future<Output = Result<T, E>> + 'run>(
        &mut self,
        make_root: impl FnOnce() -> F + 'run,
    ) -> Result<T, E> {
        let mut root = self.endpoint.demand(make_root)?;
        let mut cx = Context::from_waker(Waker::noop());
        self.push_child()?;
        while !self.stack.is_empty() {
            self.endpoint.admission.admit(ExecutionWork::Poll)?;
            let Some(current) = self.stack.last_mut() else {
                break;
            };
            let outcome = if current.active.get() {
                current.task.as_mut().poll(&mut cx)
            } else {
                Poll::Ready(Ok(()))
            };
            match outcome {
                Poll::Ready(result) => {
                    result?;
                    if !self.endpoint.queue.pending.borrow().is_empty() {
                        return Err(self
                            .endpoint
                            .admission
                            .scheduling_failure(SchedulingFailure::UnexpectedChild));
                    }
                    self.stack.pop();
                }
                Poll::Pending => self.push_child()?,
            }
        }
        match Pin::new(&mut root).poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(self
                .endpoint
                .admission
                .scheduling_failure(SchedulingFailure::MissingChild)),
        }
    }

    fn push_child(&mut self) -> Result<(), E> {
        let mut pending = self.endpoint.queue.pending.borrow_mut();
        if pending.len() > 1 {
            return Err(self
                .endpoint
                .admission
                .scheduling_failure(SchedulingFailure::ConcurrentChildren));
        }
        let child = pending.pop().ok_or_else(|| {
            self.endpoint
                .admission
                .scheduling_failure(SchedulingFailure::MissingChild)
        })?;
        self.stack.push(child);
        Ok(())
    }
}

impl<E> Drop for TaskDriver<'_, E> {
    fn drop(&mut self) {
        self.endpoint.queue.cancelled.set(true);
        // Take queued children out before dropping them: a child can own another endpoint.
        let mut pending = std::mem::take(&mut *self.endpoint.queue.pending.borrow_mut());
        while pending.pop().is_some() {}
        while self.stack.pop().is_some() {}
    }
}
