//! Host settings retained during structural preparation for admitted semantic reads.

use std::future::Future;
use std::rc::Rc;

use salsa::execution_probe::{
    Demand, ExecutionWork, RunError, RunResult, TaskEndpoint, task_layout, task_state_layout,
};
use salsa::prepared_source_probe::PreparationError;

use crate::AnalysisSettings;
use crate::lint::RuleSelection;
pub use crate::types::local_transfer::{
    boxed_future_with_fixed_transfers_at, generated_field_quote, local_with_fixed_transfers_at,
};

/// Prepared reads preserve the host's ordinary dependency edges and conditional field access.
///
/// Implementations retain canonical memo certificates and borrowed database state. They create
/// demands through the supplied endpoint, without adding a query frame. The caller must enclose
/// both demand creation and its await in `TaskEndpoint::child_call`. Each demand retains its host
/// handle until it drains. Settings borrow database storage rather than the request future.
pub trait PreparedHostFileReads<'db> {
    /// Rechecks retained certificates while the database is idle, before catalog publication.
    fn check_current(&self) -> Result<(), PreparationError>;

    fn should_check_file<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<bool>>
    where
        'db: 'run;

    fn rule_selection<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<&'db RuleSelection>>
    where
        'db: 'run;

    /// Reads the same verbose setting as ordinary diagnostic reporting, preserving its dependencies.
    fn verbose<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<bool>>
    where
        'db: 'run;

    fn analysis_settings<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<&'db AnalysisSettings>>
    where
        'db: 'run;
}

/// Admits a verbose-read task's endpoint clone and concrete factory/future transfers.
///
/// `make` is a noncapturing factory constructor, used only to identify its returned closure and
/// future types. After admission the caller clones the endpoint, calls `make` with its retained
/// host and child endpoint, then passes the returned closure directly to `TaskEndpoint::demand`.
/// The caller pays for its host/endpoint arguments and their cleanup. This helper prepays the
/// child endpoint's cleanup; demand separately admits task, reply and queue allocations.
pub fn admit_host_task_setup<'run, 'db: 'run, T: ?Sized, G, M, F>(
    endpoint: &TaskEndpoint<'run, 'db>,
    _make: &G,
) -> RunResult<()>
where
    G: FnOnce(Rc<T>, TaskEndpoint<'run, 'db>) -> M,
    M: FnOnce() -> F,
    F: Future<Output = RunResult<bool>>,
{
    // Work counts quotation (57), clone operations (16), carrier construction/transfers (41),
    // three non-final Rc drops (15), and factory invocation (1), independently of byte widths.
    endpoint.admit_work(130)?;
    let requested_bytes = size_of::<TaskEndpoint<'run, 'db>>()
        .checked_mul(6)
        .and_then(|bytes| bytes.checked_add(size_of::<(Rc<T>, TaskEndpoint<'run, 'db>)>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<M>().checked_mul(7)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Option<M>>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(task_state_layout::<M, F>().size().checked_mul(7)?))
        .and_then(|bytes| bytes.checked_add(task_layout::<M, F, bool>().size().checked_mul(5)?))
        .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(3)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Demand<bool>>().checked_mul(4)?))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Demand<bool>>>().checked_mul(4)?))
        .and_then(|bytes| bytes.checked_add(size_of::<usize>().checked_mul(54)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Option<usize>>().checked_mul(26)?))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<usize>>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<std::alloc::Layout>().checked_mul(4)?))
        .and_then(|bytes| bytes.checked_add(size_of::<[(&TaskEndpoint<'run, 'db>, &G); 2]>()))
        .ok_or(RunError::Contract("prepared host task setup bytes overflow"))?;
    endpoint.admit(ExecutionWork::Resource { requested_bytes })
}
