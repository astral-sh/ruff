//! A completed future keeps its value until the driver accepts its final poll. Future
//! destruction can request work or refuse the attempt, so it precedes reply publication.

use std::alloc::Layout;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use super::{RunContext, RunError, RunResult};

pub(super) trait RuntimeTask {
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<RunResult<()>>;
    fn retire_future(self: Pin<&mut Self>) -> RunResult<()>;
    fn discard_returned(self: Pin<&mut Self>) -> RunResult<()>;
    fn complete(self: Pin<&mut Self>, decision: RunResult<()>) -> RunResult<()>;
}

pub(super) type Task<'run> = Pin<Box<dyn RuntimeTask + 'run>>;

pin_project! {
    #[project = TaskStateProj]
    enum TaskState<M, F> {
        Factory { make: Option<M> },
        Running { #[pin] future: F },
        Finished,
    }
}

pin_project! {
    pub(super) struct TypedTask<'db, M, F, T> {
        // On rejection, the returned value drops before its remaining future and ancestors.
        returned: Option<RunResult<T>>,
        #[pin]
        state: TaskState<M, F>,
        destination: Rc<RefCell<Option<RunResult<T>>>>,
        context: Rc<RunContext<'db>>,
        delivered: bool,
    }
}

impl<'db, M, F, T> TypedTask<'db, M, F, T> {
    pub(super) fn new(
        make: M,
        destination: Rc<RefCell<Option<RunResult<T>>>>,
        context: Rc<RunContext<'db>>,
    ) -> Self {
        Self {
            returned: None,
            state: TaskState::Factory { make: Some(make) },
            destination,
            context,
            delivered: false,
        }
    }
}

/// Returns the private factory/future state layout for quoting by-value task-state transfers.
/// This is representation metadata and does not construct a state or admit any work.
pub const fn task_state_layout<M, F>() -> Layout {
    Layout::new::<TaskState<M, F>>()
}

/// Returns the complete task layout for quoting construction and transfers before task storage.
/// `Endpoint::demand` separately admits the stored task allocation; callers must not quote that
/// allocation again when using this layout for earlier by-value transfers.
pub const fn task_layout<'db, M, F, T>() -> Layout {
    Layout::new::<TypedTask<'db, M, F, T>>()
}

impl<M, F, T> RuntimeTask for TypedTask<'_, M, F, T>
where
    M: FnOnce() -> F,
    F: Future<Output = RunResult<T>>,
{
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<RunResult<()>> {
        let mut this = self.project();
        if *this.delivered || this.returned.is_some() {
            return Poll::Ready(Err(RunError::Contract("completed task was polled again")));
        }
        if let TaskStateProj::Factory { make } = this.state.as_mut().project() {
            let Some(make) = make.take() else {
                return Poll::Ready(Err(RunError::Contract("task factory was consumed")));
            };
            this.state.set(TaskState::Running { future: make() });
        }
        let TaskStateProj::Running { future } = this.state.as_mut().project() else {
            return Poll::Ready(Err(RunError::Contract("task future was retired")));
        };
        let Poll::Ready(result) = future.poll(cx) else {
            return Poll::Pending;
        };
        *this.returned = Some(result);
        let outcome = this
            .returned
            .as_ref()
            .map_or(Err(RunError::Contract("task lost its result")), |result| {
                result.as_ref().map(|_| ()).map_err(|error| *error)
            });
        // A future's destructor can report another failure. Preserve its original error first,
        // while leaving the future and any successful output for the driver's ordered cleanup.
        Poll::Ready(match outcome {
            Ok(()) => Ok(()),
            Err(error) => this.context.observe(Err(error)),
        })
    }

    fn retire_future(self: Pin<&mut Self>) -> RunResult<()> {
        let mut this = self.project();
        if *this.delivered
            || this.returned.is_none()
            || !matches!(this.state.as_ref().get_ref(), TaskState::Running { .. })
        {
            return Err(RunError::Contract("task cannot retire its future"));
        }
        // Pin::set destroys the completed future in place. The driver keeps its active-poll
        // owner installed and checks work requested by this destructor before publishing.
        this.state.set(TaskState::Finished);
        Ok(())
    }

    fn discard_returned(self: Pin<&mut Self>) -> RunResult<()> {
        let this = self.project();
        if *this.delivered
            || this.returned.is_none()
            || !matches!(this.state.as_ref().get_ref(), TaskState::Finished)
        {
            return Err(RunError::Contract("task cannot discard its result"));
        }
        *this.delivered = true;
        drop(this.returned.take());
        Ok(())
    }

    fn complete(self: Pin<&mut Self>, decision: RunResult<()>) -> RunResult<()> {
        let this = self.project();
        if *this.delivered {
            return Err(RunError::Contract("task result was delivered again"));
        }
        if decision.is_ok()
            && (!matches!(this.state.as_ref().get_ref(), TaskState::Finished)
                || !matches!(this.returned, Some(Ok(_))))
        {
            return Err(RunError::Contract("task result was not ready to publish"));
        }
        let mut destination = this
            .destination
            .try_borrow_mut()
            .map_err(|_| RunError::Contract("task reply is borrowed"))?;
        if destination.is_some() {
            return Err(RunError::Contract("task reply already has a result"));
        }
        match decision {
            Ok(()) => match this.returned.take() {
                Some(Ok(value)) => *destination = Some(Ok(value)),
                other => {
                    *this.returned = other;
                    return Err(RunError::Contract("task lost its completed result"));
                }
            },
            // An error reply never transfers the staged output or destroys the task's future.
            // Both remain on the driver's stack until its pending descendants have been dropped.
            Err(error) => *destination = Some(Err(error)),
        }
        *this.delivered = true;
        Ok(())
    }
}
