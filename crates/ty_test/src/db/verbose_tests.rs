//! Compare prepared verbose reads with ordinary values and query invalidation.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{SystemPath, WritableSystem};
use salsa::attempt_probe::AttemptOutcome;
use salsa::execution_probe::{
    CallableRouteProvider, ExecutionLimits, NativeValueOperation, NativeValueQuote, RegistryBuilder,
    RunError, RunResult, TaskEndpoint, try_with_execution_budget,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;
use salsa::prepared_source_probe::try_with_preparation;
use salsa::{Cycle, Id};
use ty_python_semantic::Db as SemanticDb;
use ty_python_semantic::prepared_host::{PreparedHostFileReads, local_with_fixed_transfers_at};

use super::Db;

thread_local! {
    static ORDINARY_CALLS: Cell<usize> = const { Cell::new(0) };
    static CONTROLLED_ORDINARY_CALLS: Cell<usize> = const { Cell::new(0) };
}

/// Reads the ordinary verbose setting in its own memo so its invalidation can be compared.
#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn ordinary_verbose_probe(db: &dyn SemanticDb, _file: File) -> bool {
    ORDINARY_CALLS.set(ORDINARY_CALLS.get() + 1);
    db.verbose()
}

/// Supplies the canonical query identity whose controlled body is provided by `VerboseProvider`.
/// Its ordinary body is counted separately so an unexpected ordinary execution fails the control.
#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn controlled_verbose_probe(db: &dyn SemanticDb, _file: File) -> bool {
    CONTROLLED_ORDINARY_CALLS.set(CONTROLLED_ORDINARY_CALLS.get() + 1);
    db.verbose()
}

/// Retains the real prepared host and counts controlled executions of the probe query.
struct VerboseProvider<'db, 'run> {
    host: Rc<dyn PreparedHostFileReads<'db> + 'db>,
    calls: &'run Cell<usize>,
}

impl fmt::Debug for VerboseProvider<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerboseProvider")
            .field("host", &Rc::as_ptr(&self.host))
            .field("calls", &self.calls)
            .finish()
    }
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for VerboseProvider<'db, 'run>
where
    C: for<'a> Configuration<DbView = dyn SemanticDb, Input<'a> = File, Output<'a> = bool>,
{
    async fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn SemanticDb,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let requested_bytes = match operation {
            NativeValueOperation::InputConversion(_) => size_of::<File>(),
            NativeValueOperation::Comparison { .. } => 0,
        };
        Ok(NativeValueQuote {
            work: 1,
            requested_bytes,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn SemanticDb,
        _input: File,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 3, 0, || {
            self.calls.set(self.calls.get() + 1);
        })
        .await?;
        Ok(endpoint
            .child_call(|| async {
                let bytes = size_of::<(
                    [TaskEndpoint<'run, 'db>; 4],
                    [(Rc<dyn PreparedHostFileReads<'db> + 'db>, TaskEndpoint<'run, 'db>); 3],
                )>();
                let (host, child) = local_with_fixed_transfers_at(&endpoint, 45, bytes, || {
                    (Rc::clone(&self.host), endpoint.clone())
                })
                .await?;
                host.verbose(child)?.await
            })
            .await)
    }

    async fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn SemanticDb,
        _id: Id,
        _input: File,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::Contract("verbose probe unexpectedly entered a cycle"))
    }

    async fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn SemanticDb,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: File,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::Contract("verbose probe unexpectedly recovered a cycle"))
    }
}

/// Requests the canonical probe through its prepared-host provider with the unchanged limits.
/// Only structural settings preparation precedes the controlled request; the ordinary probe is
/// invoked separately afterward by `check_verbose_step`.
fn run_verbose_probe(
    db: &dyn SemanticDb,
    file: File,
    calls: &Cell<usize>,
) -> Result<AttemptOutcome<RunResult<bool>>, String> {
    try_with_preparation(db, || db.prepare_analysis_file_settings(file))
        .map_err(|error| format!("verbose fixture preparation failed: {error:?}"))?;
    let host = db
        .prepare_analysis_host_reads(file)
        .map_err(|error| format!("verbose fixture sealing failed: {error:?}"))?;
    try_with_execution_budget(
        db,
        ExecutionLimits {
            semantic_work: 1_000_000,
            requested_bytes: 16 * 1024 * 1024,
        },
        |budget| {
            let mut registry = RegistryBuilder::with_budget(db, &budget)?;
            let route = registry.reserve_callable(
                db,
                controlled_verbose_probe::fn_ingredient_(db, db.zalsa()),
            )?;
            registry.bind_callable(&route, VerboseProvider { host, calls })?;
            registry.seal()?.run(move |endpoint| async move {
                let value = endpoint
                    .child_call(|| async { endpoint.fetch_ref(&route, file.as_id())?.await })
                    .await;
                local_with_fixed_transfers_at(&endpoint, 1, 0, || *value).await
            })
        },
    )
    .map_err(|error| format!("verbose fixture execution could not start: {error:?}"))
}

/// The expected result and cumulative body executions of each independent probe query.
#[derive(Clone, Copy, Debug)]
struct Expected {
    value: bool,
    executions: usize,
}

/// Checks controlled completion first, then ordinary value and matching memo reexecution counts.
fn check_verbose_step(db: &dyn SemanticDb, file: File, calls: &Cell<usize>, expected: Expected) {
    assert_eq!(
        run_verbose_probe(db, file, calls),
        Ok(AttemptOutcome::Complete(Ok(expected.value))),
    );
    assert_eq!(calls.get(), expected.executions);
    assert_eq!(ordinary_verbose_probe(db, file), expected.value);
    assert_eq!(ORDINARY_CALLS.get(), expected.executions);
    assert_eq!(CONTROLLED_ORDINARY_CALLS.get(), 0);
}

/// The mdtest host records the settings input dependency: toggling verbosity changes both
/// probe results and reexecutes each once, while a same-revision request reuses its memo.
#[test]
fn mdtest_verbose_preserves_value_and_input_invalidation() -> anyhow::Result<()> {
    ORDINARY_CALLS.set(0);
    CONTROLLED_ORDINARY_CALLS.set(0);
    let mut db = Db::setup();
    let path = SystemPath::new("/verbose.py");
    db.system.write_file_bytes(path, b"")?;
    let file = system_path_to_file(&db, path)?;
    let calls = Cell::new(0);

    check_verbose_step(&db, file, &calls, Expected { value: false, executions: 1 });
    db.set_verbosity(true);
    check_verbose_step(&db, file, &calls, Expected { value: true, executions: 2 });
    check_verbose_step(&db, file, &calls, Expected { value: true, executions: 2 });
    db.set_verbosity(false);
    check_verbose_step(&db, file, &calls, Expected { value: false, executions: 3 });
    Ok(())
}
