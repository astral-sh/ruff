//! Passive boundaries for function-context ownership, provider suspension, and final reconstruction.

use std::cell::Cell;
use std::future::{Future, poll_fn};

use salsa::plumbing::AsId;

use crate::Db;
use crate::types::function::FunctionType;

/// Identifies an observed boundary without changing admission or scheduling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    Retained,
    ChildEntered,
    ChildPending,
    ChildRetired,
    /// The signature and implementation list await local aggregate admission.
    BeforeUpdated,
    /// The admitted local callback has built the updated-signature aggregate.
    AfterUpdated,
    /// The completed aggregate is retained before the function interner provider starts.
    BeforeIntern,
    /// The function interner provider has returned the canonical function handle.
    AfterIntern,
    Retiring,
}

thread_local! {
    static OBSERVER: Cell<Option<fn(salsa::Id, Stage)>> = const { Cell::new(None) };
    static FUNCTION: Cell<Option<salsa::Id>> = const { Cell::new(None) };
}

/// Installs a passive callback and returns the callback to restore afterward.
pub(in crate::types) fn set_observer(
    observer: Option<fn(salsa::Id, Stage)>,
) -> Option<fn(salsa::Id, Stage)> {
    OBSERVER.replace(observer)
}

/// Reports a boundary for the current function-context operation, when one is observed.
pub(in crate::types) fn record(_db: &dyn Db, stage: Stage) {
    record_for(FUNCTION.get(), stage);
}

/// Associates a provider boundary with the function retained when that provider was entered.
fn record_for(function: Option<salsa::Id>, stage: Stage) {
    if let Some(function) = function
        && let Some(observer) = OBSERVER.get()
    {
        observer(function, stage);
    }
}

/// Records the enclosing function future's lifetime and restores a nested observation's parent.
#[derive(Debug)]
pub(in crate::types) struct OperationLifetime {
    function: salsa::Id,
    previous: Option<salsa::Id>,
}

impl OperationLifetime {
    /// Begins observing the original function retained by the enclosing constructor-context adapter future.
    pub(in crate::types) fn new(_db: &dyn Db, function: FunctionType<'_>) -> Self {
        let function = function.as_id();
        let previous = FUNCTION.replace(Some(function));
        record_for(Some(function), Stage::Retained);
        Self { function, previous }
    }
}

impl Drop for OperationLifetime {
    fn drop(&mut self) {
        record_for(Some(self.function), Stage::Retiring);
        FUNCTION.set(self.previous);
    }
}

/// Records retirement after the real provider future and its retained values have been dropped.
#[derive(Debug)]
struct ChildLifetime(Option<salsa::Id>);

impl Drop for ChildLifetime {
    fn drop(&mut self) {
        record_for(self.0, Stage::ChildRetired);
    }
}

/// Observes actual polls of a signature, metadata, or implementation provider without adding yields.
pub(in crate::types) async fn observe_child<F: Future>(_db: &dyn Db, future: F) -> F::Output {
    let function = FUNCTION.get();
    let _lifetime = ChildLifetime(function);
    record_for(function, Stage::ChildEntered);
    let mut future = std::pin::pin!(future);
    poll_fn(|context| {
        let result = future.as_mut().poll(context);
        if result.is_pending() {
            record_for(function, Stage::ChildPending);
        }
        result
    })
    .await
}
