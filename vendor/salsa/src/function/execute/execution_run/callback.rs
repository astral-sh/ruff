//! Inline callbacks retain their query's owner until the driver accepts their completion.
//! A failed callback suspends with its future and returned value still owned, so queued children
//! are destroyed before the surrounding query frame or claim.

use std::convert::Infallible;
use std::future::{Future, pending, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
#[cfg(test)]
use std::sync::LazyLock;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use super::frame_free::EntryScope;
use super::progress::CallbackPoll;
use super::{Endpoint, ExecutionWork, RunError, RunResult, explicit_reads};

pub(super) trait CallbackOwner {
    fn check_resume(&self) -> RunResult<()>;
    fn is_current(&self) -> bool;
}

#[derive(Clone, Copy)]
pub(super) enum CallbackKind {
    NativeQuotation,
    NativeAdmission,
    NativeCall,
    Execution,
    ColdInitial,
    Canonical,
}

pub(super) async fn local_call<'call, 'run: 'call, 'db: 'run, M, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    make: M,
) -> T
where
    M: FnOnce() -> RunResult<T> + 'call,
    T: 'call,
{
    if !endpoint.local_call_is_eligible() {
        // Unsupported endpoints cannot record a terminal failure in another attempt. Retain the
        // uninvoked action; an actual driver rejects this ordinary malformed Pending.
        let _held = make;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.context) {
        Ok(scope) => scope,
        Err(error) => match reject(endpoint, error, make).await {},
    };
    complete_local_immediate(endpoint, &scope, make).await
}

pub(super) async fn child_call<'call, 'run: 'call, 'db: 'run, M, F, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    make: M,
) -> T
where
    M: FnOnce() -> F + 'call,
    F: Future<Output = RunResult<T>> + 'call,
    T: 'call,
{
    if !endpoint.local_call_is_eligible() {
        let _held = make;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.context) {
        Ok(scope) => scope,
        Err(error) => match reject(endpoint, error, make).await {},
    };
    complete(endpoint, &scope, CallbackKind::Canonical, make).await
}

pin_project! {
    #[project = CallbackStateProj]
    enum CallbackState<M, F> {
        Factory { make: Option<M> },
        Running { #[pin] future: F },
        Retired,
    }
}

pin_project! {
    pub(super) struct CallbackCompletion<'call, 'run, 'db: 'run, O: ?Sized, M, F, T> {
        // Rejection drops this output before the still-live future and enclosing query owner.
        returned: Option<RunResult<T>>,
        #[pin]
        state: CallbackState<M, F>,
        endpoint: &'call Endpoint<'run, 'db>,
        owner: &'call O,
        kind: CallbackKind,
        stopped: bool,
        unaccepted_error: Option<RunError>,
    }
}

pub(super) fn complete<'call, 'run: 'call, 'db: 'run, O, M, F, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    kind: CallbackKind,
    make: M,
) -> CallbackCompletion<'call, 'run, 'db, O, M, F, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> F + 'call,
    F: Future<Output = RunResult<T>> + 'call,
    T: 'call,
{
    #[cfg(test)]
    {
        static LAYOUT_PROBE: LazyLock<bool> =
            LazyLock::new(|| std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some());
        if *LAYOUT_PROBE {
            eprintln!(
                "CALLBACK_LAYOUT\tadapter={}\tfactory={}\tfuture={}\toutput={}\tqueue={}\tactive_poll={}\tterminal={}\tfuture_type={}",
                size_of::<CallbackCompletion<'call, 'run, 'db, O, M, F, T>>(),
                size_of::<M>(),
                size_of::<F>(),
                size_of::<T>(),
                size_of::<super::Queue<'run>>(),
                size_of::<super::progress::ActivePoll>(),
                size_of::<super::progress::TerminalDisposition>(),
                std::any::type_name::<F>(),
            );
        }
    }
    CallbackCompletion {
        returned: None,
        state: CallbackState::Factory { make: Some(make) },
        endpoint,
        owner,
        kind,
        stopped: false,
        unaccepted_error: None,
    }
}

impl<O, M, F, T> Future for CallbackCompletion<'_, '_, '_, O, M, F, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> F,
    F: Future<Output = RunResult<T>>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut this = self.project();
        if *this.stopped {
            return Poll::Pending;
        }
        let Some(receipt) = this.endpoint.callback_poll() else {
            // An invalid private invocation has no authority to alter another task. Keep its
            // owners held; the actual driver's ordinary malformed-Pending check rejects it.
            *this.stopped = true;
            return Poll::Pending;
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| -> RunResult<Poll<T>> {
            if let CallbackStateProj::Factory { make } = this.state.as_mut().project() {
                let access = this.endpoint.queue.begin_access()?;
                if matches!(this.kind, CallbackKind::NativeQuotation) {
                    // The enclosing admitted task includes this future's inline storage.
                    // Its factory must be passive; dynamic quote state is admitted when polled.
                    this.endpoint.admit_work(1)?;
                    access.check()?;
                }
                if matches!(
                    this.kind,
                    CallbackKind::NativeQuotation
                        | CallbackKind::NativeAdmission
                        | CallbackKind::NativeCall
                ) {
                    this.owner.check_resume()?;
                    access.check()?;
                }
                check_completion(&receipt, *this.owner, "callback started beneath its child")?;
                let make = make
                    .take()
                    .ok_or(RunError::Contract("callback factory was consumed"))?;
                this.state.set(CallbackState::Running { future: make() });
            }
            let CallbackStateProj::Running { future } = this.state.as_mut().project() else {
                return Err(RunError::Contract("retired callback was polled"));
            };
            this.endpoint.queue.check_runnable()?;
            let Poll::Ready(result) = future.poll(cx) else {
                explicit_reads::check_acceptance();
                return Ok(Poll::Pending);
            };
            *this.returned = Some(result);
            explicit_reads::check_acceptance();
            if let Some(Err(error)) = this.returned.as_ref() {
                return Err(*error);
            }
            check_completion(
                &receipt,
                *this.owner,
                "callback completed beneath its child",
            )?;
            this.state.set(CallbackState::Retired);
            if matches!(this.kind, CallbackKind::Execution) {
                // Preserve the old successful observation check without moving its output.
                let access = this.endpoint.queue.begin_access()?;
                this.endpoint.context.check_resume()?;
                access.check()?;
            }
            if matches!(
                this.kind,
                CallbackKind::Execution | CallbackKind::ColdInitial
            ) {
                this.endpoint.admit(ExecutionWork::Work { units: 1 })?;
                let access = this.endpoint.queue.begin_access()?;
                this.owner.check_resume()?;
                access.check()?;
            }
            check_completion(
                &receipt,
                *this.owner,
                "callback owner changed during completion",
            )?;
            match this.returned.take() {
                Some(Ok(value)) => Ok(Poll::Ready(value)),
                Some(Err(error)) => Err(error),
                None => Err(RunError::Contract("callback lost its returned value")),
            }
        }));
        finish_poll(&receipt, this.stopped, this.unaccepted_error, outcome)
    }
}

#[derive(Clone, Copy)]
enum ImmediateKind {
    Canonical,
    Local,
}

enum ImmediateState<M, T> {
    Action { make: Option<M> },
    Returned { result: Option<RunResult<T>> },
}

pin_project! {
    pub(super) struct ImmediateCompletion<'call, 'run, 'db: 'run, O: ?Sized, M, T> {
        // The action or its returned value remains owned until the driver drains queued children.
        state: ImmediateState<M, T>,
        endpoint: &'call Endpoint<'run, 'db>,
        owner: &'call O,
        kind: ImmediateKind,
        stopped: bool,
        unaccepted_error: Option<RunError>,
    }
}

/// A canonical action has no child future whose retirement can run further user code.
pub(super) fn complete_immediate<'call, 'run: 'call, 'db: 'run, O, M, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    make: M,
) -> ImmediateCompletion<'call, 'run, 'db, O, M, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> RunResult<T> + 'call,
    T: 'call,
{
    complete_immediate_with_kind(endpoint, owner, ImmediateKind::Canonical, make)
}

pub(super) fn complete_local_immediate<'call, 'run: 'call, 'db: 'run, O, M, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    make: M,
) -> ImmediateCompletion<'call, 'run, 'db, O, M, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> RunResult<T> + 'call,
    T: 'call,
{
    complete_immediate_with_kind(endpoint, owner, ImmediateKind::Local, make)
}

fn complete_immediate_with_kind<'call, 'run: 'call, 'db: 'run, O, M, T>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    kind: ImmediateKind,
    make: M,
) -> ImmediateCompletion<'call, 'run, 'db, O, M, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> RunResult<T> + 'call,
    T: 'call,
{
    ImmediateCompletion {
        state: ImmediateState::Action { make: Some(make) },
        endpoint,
        owner,
        kind,
        stopped: false,
        unaccepted_error: None,
    }
}

impl<O, M, T> Future for ImmediateCompletion<'_, '_, '_, O, M, T>
where
    O: CallbackOwner + ?Sized,
    M: FnOnce() -> RunResult<T>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        let this = self.project();
        if *this.stopped {
            return Poll::Pending;
        }
        let Some(receipt) = this.endpoint.callback_poll() else {
            // Keep the uninvoked action held when no current task can receive the failure.
            *this.stopped = true;
            return Poll::Pending;
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| -> RunResult<Poll<T>> {
            if let ImmediateState::Action { make } = this.state {
                match this.kind {
                    ImmediateKind::Canonical => {
                        this.endpoint.queue.begin_access()?;
                    }
                    ImmediateKind::Local => {
                        // The receipt predates this callback, so revision cancellation retains its
                        // native payload instead of failing the later passive stamp check.
                        check_local_resume(this.endpoint, *this.owner)?;
                    }
                }
                check_completion(&receipt, *this.owner, "callback started beneath its child")?;
                let make = make
                    .take()
                    .ok_or(RunError::Contract("callback factory was consumed"))?;
                // Capture destruction is part of the action. Stage its output before any check
                // that can reject it, retaining that output through child drainage.
                *this.state = ImmediateState::Returned {
                    result: Some(make()),
                };
            }
            this.endpoint.queue.check_runnable()?;
            explicit_reads::check_acceptance();
            let ImmediateState::Returned { result } = this.state else {
                return Err(RunError::Contract("callback lost its returned value"));
            };
            if let Some(Err(error)) = result.as_ref() {
                return Err(*error);
            }
            let message = match this.kind {
                ImmediateKind::Canonical => "callback completed beneath its child",
                ImmediateKind::Local => {
                    check_local_resume(this.endpoint, *this.owner)?;
                    "callback owner changed during completion"
                }
            };
            check_completion(&receipt, *this.owner, message)?;
            match result.take() {
                Some(Ok(value)) => Ok(Poll::Ready(value)),
                Some(Err(error)) => Err(error),
                None => Err(RunError::Contract("callback lost its returned value")),
            }
        }));
        finish_poll(&receipt, this.stopped, this.unaccepted_error, outcome)
    }
}

fn check_local_resume<O: CallbackOwner + ?Sized>(
    endpoint: &Endpoint<'_, '_>,
    owner: &O,
) -> RunResult<()> {
    let access = endpoint.queue.begin_access()?;
    owner.check_resume()?;
    access.check()
}

fn check_completion<O: CallbackOwner + ?Sized>(
    receipt: &CallbackPoll<'_, '_, '_>,
    owner: &O,
    message: &'static str,
) -> RunResult<()> {
    receipt.check_completion()?;
    if !owner.is_current() {
        return Err(RunError::Contract(message));
    }
    Ok(())
}

fn finish_poll<T>(
    receipt: &CallbackPoll<'_, '_, '_>,
    stopped: &mut bool,
    unaccepted_error: &mut Option<RunError>,
    outcome: std::thread::Result<RunResult<Poll<T>>>,
) -> Poll<T> {
    match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            *stopped = true;
            match catch_unwind(AssertUnwindSafe(|| receipt.error(error))) {
                Ok(true) => {}
                Ok(false) => *unaccepted_error = Some(error),
                Err(payload) => {
                    if let Err(payload) = receipt.try_panic(payload) {
                        resume_unwind(payload);
                    }
                }
            }
            Poll::Pending
        }
        Err(payload) => {
            *stopped = true;
            if let Err(payload) = receipt.try_panic(payload) {
                resume_unwind(payload);
            }
            Poll::Pending
        }
    }
}

/// An impossible passive transfer failure still retains the output and its original owner.
pub(super) async fn reject<H>(endpoint: &Endpoint<'_, '_>, error: RunError, held: H) -> Infallible {
    let _held = held;
    poll_fn(|_| {
        if let Some(receipt) = endpoint.callback_poll() {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| receipt.error(error))) {
                if let Err(payload) = receipt.try_panic(payload) { resume_unwind(payload); }
            }
        }
        Poll::Pending
    })
    .await
}
