use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::LazyLock;
use std::task::{Context, Poll};

use super::super::task::{RuntimeTask, TypedTask};
use super::super::{PendingTask, RunContext, RunResult, Task};
use crate::DatabaseKeyIndex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::function) enum Event {
    Claim {
        key: DatabaseKeyIndex,
        serial: usize,
        operation: usize,
    },
    Execute {
        key: DatabaseKeyIndex,
        serial: usize,
        operation: usize,
        old_memo: Option<usize>,
    },
}

#[derive(Default)]
pub(in crate::function::execute::execution_run) struct Observations {
    pub events: Vec<Event>,
    pub polls: usize,
    pub max_active_polls: usize,
    active_polls: usize,
}

thread_local! {
    static OBSERVATIONS: RefCell<Option<Observations>> = const { RefCell::new(None) };
}

static TASK_LAYOUT_PROBE: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some());

// Preserve the prior generic body exactly so its future layout can be compared for the same
// factory, future and result types. Only its type is measured; the sizing closure never runs.
async fn prior_completing_task<T, F: Future<Output = RunResult<T>>, M: FnOnce() -> F>(
    context: Rc<RunContext<'_>>,
    make: M,
    destination: Rc<RefCell<Option<RunResult<T>>>>,
) -> RunResult<()> {
    let result = context.observe(make().await);
    let outcome = result.as_ref().map(|_| ()).map_err(|error| *error);
    *destination.borrow_mut() = Some(result);
    outcome
}

fn result_size<A, B, C, R>(_factory: impl FnOnce(A, B, C) -> R) -> usize {
    size_of::<R>()
}

pub(in crate::function::execute::execution_run) fn record_task_layout<'db, M, F, T>(
    _context: &Rc<RunContext<'db>>,
    requested_bytes: usize,
) where
    M: FnOnce() -> F,
    F: Future<Output = RunResult<T>>,
{
    if !*TASK_LAYOUT_PROBE {
        return;
    }
    let old_payload = result_size(
        |context: Rc<RunContext<'db>>, make: M, destination: Rc<RefCell<Option<RunResult<T>>>>| {
            prior_completing_task(context, make, destination)
        },
    );
    let new_payload = size_of::<TypedTask<'db, M, F, T>>();
    let reply_slot = size_of::<RefCell<Option<RunResult<T>>>>();
    let wanted = size_of::<Cell<bool>>();
    let pending = size_of::<PendingTask<'_>>();
    let rc_headers = 4 * size_of::<usize>();
    let reply_alignment = align_of::<RefCell<Option<RunResult<T>>>>().max(align_of::<usize>());
    let wanted_alignment = align_of::<usize>();
    let requested = |payload: usize| {
        payload
            .saturating_add(reply_slot)
            .saturating_add(wanted)
            .saturating_add(8 * pending)
            .saturating_add(rc_headers)
            .saturating_add(reply_alignment)
            .saturating_add(wanted_alignment)
    };
    let old_requested = requested(old_payload);
    let new_requested = requested(new_payload);
    assert_eq!(new_requested, requested_bytes);
    eprintln!(
        "TASK_LAYOUT\told_payload={old_payload}\tnew_payload={new_payload}\tfuture={}\tfactory={}\toutput={}\treply_slot={reply_slot}\twanted={wanted}\tpending={pending}\trc_headers={rc_headers}\treply_alignment={reply_alignment}\twanted_alignment={wanted_alignment}\told_requested={old_requested}\tnew_requested={new_requested}\tactual_requested={requested_bytes}\tfuture_type={}\tfactory_type={}\toutput_type={}",
        size_of::<F>(),
        size_of::<M>(),
        size_of::<T>(),
        type_name::<F>(),
        type_name::<M>(),
        type_name::<T>(),
    );
}

pub(in crate::function::execute::execution_run) fn record(event: Event) {
    super::validation_trace::record(super::validation_trace::TraceEvent::Ownership(event));
    OBSERVATIONS.with_borrow_mut(|observations| {
        if let Some(observations) = observations {
            observations.events.push(event);
        }
    });
}

pub(in crate::function::execute::execution_run) fn collect<T>(
    f: impl FnOnce() -> T,
) -> (T, Observations) {
    OBSERVATIONS.with_borrow_mut(|observations| {
        assert!(observations.replace(Observations::default()).is_none());
    });
    let reset = Reset;
    let result = f();
    let observations = OBSERVATIONS.with_borrow_mut(|observations| observations.take().unwrap());
    assert_eq!(observations.active_polls, 0);
    drop(reset);
    (result, observations)
}

struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        OBSERVATIONS.with_borrow_mut(|observations| *observations = None);
    }
}

struct PollGuard;

impl PollGuard {
    fn enter() -> Self {
        OBSERVATIONS.with_borrow_mut(|observations| {
            if let Some(observations) = observations {
                observations.polls += 1;
                observations.active_polls += 1;
                observations.max_active_polls =
                    observations.max_active_polls.max(observations.active_polls);
            }
        });
        Self
    }
}

impl Drop for PollGuard {
    fn drop(&mut self) {
        OBSERVATIONS.with_borrow_mut(|observations| {
            if let Some(observations) = observations {
                observations.active_polls -= 1;
            }
        });
    }
}

struct ObservedTask<'run>(Task<'run>);

impl RuntimeTask for ObservedTask<'_> {
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<RunResult<()>> {
        let _poll = PollGuard::enter();
        self.0.as_mut().poll(context)
    }

    fn retire_future(mut self: Pin<&mut Self>) -> RunResult<()> {
        self.0.as_mut().retire_future()
    }

    fn discard_returned(mut self: Pin<&mut Self>) -> RunResult<()> {
        self.0.as_mut().discard_returned()
    }

    fn complete(mut self: Pin<&mut Self>, decision: RunResult<()>) -> RunResult<()> {
        self.0.as_mut().complete(decision)
    }
}

pub(in crate::function::execute::execution_run) fn observe_task(task: Task<'_>) -> Task<'_> {
    // Wrap the boxed task itself, so directly polling a demanded child would increase depth.
    // This extra allocation exists only in unit tests; production tasks keep their original box.
    Box::pin(ObservedTask(task))
}
