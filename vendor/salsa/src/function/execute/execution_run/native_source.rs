//! Scoped native queries keep their canonical frames while a generated child uses its own queue.

use std::cell::Cell;
#[cfg(test)]
use std::cell::RefCell;
use std::future::pending;
use std::num::NonZeroUsize;

use super::callback::{self, CallbackOwner};
use super::frame_free::{Caller, EntryScope};
use super::registration::{NativeSourceRoute, TaskEndpoint};
use super::{Endpoint, Queue, RunContext, RunError, RunResult};
use crate::attempt_probe::{self, AttemptSupport, Incomplete, LocalOwnershipReceipt, QueryPolicy, StartError};
use crate::function::{Configuration, VerifyResult};
use crate::zalsa::ZalsaDatabase;
use crate::{Database, Id, Revision};
#[cfg(test)]
use crate::DatabaseKeyIndex;

/// Bounds simultaneous registered drivers, not native source stack consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCallbackLimits {
    max_active_runs: NonZeroUsize,
}

impl NativeCallbackLimits {
    pub const fn new(max_active_runs: NonZeroUsize) -> Self {
        Self { max_active_runs }
    }
}

/// A lexical native caller authorizes only registries borrowed within this entry.
pub struct NativeCallbackEntry<'entry> {
    pub(super) receipt: &'entry NativeEntryReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DriverIdentity {
    id: NonZeroUsize,
    depth: usize,
    limits: Option<NativeCallbackLimits>,
}

#[cfg(test)]
impl DriverIdentity {
    pub(super) fn depth(self) -> usize { self.depth }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourcePermit {
    queue: usize,
    epoch: usize,
    driver: DriverIdentity,
    caller: Caller,
    database: usize,
    local: usize,
}

thread_local! {
    static CURRENT_DRIVER: Cell<Option<DriverIdentity>> = const { Cell::new(None) };
    static NEXT_DRIVER: Cell<Option<NonZeroUsize>> = const { Cell::new(Some(NonZeroUsize::MIN)) };
    static SOURCE_PERMIT: Cell<Option<SourcePermit>> = const { Cell::new(None) };
}

pub(super) fn current_driver() -> Option<DriverIdentity> {
    CURRENT_DRIVER.with(Cell::get)
}

#[derive(Clone)]
pub(super) struct NativeEntryReceipt {
    pub(super) support: AttemptSupport,
    caller: Caller,
    depths: (usize, usize),
    ownership: LocalOwnershipReceipt,
    driver: Option<DriverIdentity>,
    permit: Option<SourcePermit>,
    limits: NativeCallbackLimits,
    local: usize,
}

impl NativeEntryReceipt {
    fn capture(db: &dyn Database, limits: NativeCallbackLimits) -> RunResult<Self> {
        let receipt = Self::capture_owner(db, limits)?;
        receipt.check(db)?;
        receipt.check_depth(db)?;
        Ok(receipt)
    }

    fn capture_owner(db: &dyn Database, limits: NativeCallbackLimits) -> RunResult<Self> {
        if attempt_probe::current_policy() != QueryPolicy::ReturnOnly {
            return Err(RunError::Contract("native callback requires a return-only query"));
        }
        let support = attempt_probe::current()
            .filter(|support| support.is_current(db.zalsa()))
            .ok_or(RunError::Contract("native callback requires the current attempt"))?;
        let caller = Caller::capture_db(db)?;
        if caller.key.is_none() {
            return Err(RunError::Contract("native callback requires an active native query"));
        }
        let ownership = support.local_ownership(db.zalsa())
            .ok_or(RunError::Contract("native callback lost its lexical scope"))?;
        let receipt = Self {
            support,
            caller,
            depths: attempt_probe::stack_depths(),
            ownership,
            driver: current_driver(),
            permit: SOURCE_PERMIT.with(Cell::get),
            limits,
            local: std::ptr::from_ref(db.zalsa_local()).addr(),
        };
        receipt.check_owner_with_driver(db, receipt.driver)?;
        Ok(receipt)
    }

    pub(super) fn check(&self, db: &dyn Database) -> RunResult<()> {
        self.check_with_driver(db, self.driver)
    }

    fn check_with_driver(&self, db: &dyn Database, driver: Option<DriverIdentity>) -> RunResult<()> {
        self.check_owner_with_driver(db, driver)?;
        if let Some(reason) = self.support.reason() {
            return Err(RunError::Refused(reason));
        }
        Ok(())
    }

    fn check_owner_with_driver(&self, db: &dyn Database, driver: Option<DriverIdentity>) -> RunResult<()> {
        if attempt_probe::current_policy() != QueryPolicy::ReturnOnly
            || !self.support.is_current(db.zalsa())
            || self.support.local_ownership(db.zalsa()) != Some(self.ownership)
            || attempt_probe::stack_depths() != self.depths
            || !self.caller.is_current_db(db)
            || self.local != std::ptr::from_ref(db.zalsa_local()).addr()
            || current_driver() != driver
            || SOURCE_PERMIT.with(Cell::get) != self.permit
        {
            return Err(RunError::Contract("native callback changed its enclosing owner"));
        }
        if let Some(driver) = self.driver {
            let Some(permit) = self.permit else {
                return Err(RunError::Contract("nested driver requires a native source permit"));
            };
            if permit.driver != driver
                || permit.database != std::ptr::from_ref(db.zalsa()).addr()
                || permit.local != self.local
                || self.caller.depth <= permit.caller.depth
                || db.zalsa_local().try_with_query_stack(|stack| {
                    permit.caller.depth == 0
                        || stack.get(permit.caller.depth - 1)
                            .map(|frame| frame.database_key_index) == permit.caller.key
                }) != Some(true)
                || driver.limits.is_some_and(|limits| limits != self.limits)
            {
                return Err(RunError::Contract("native callback has a foreign source permit or limit"));
            }
        }
        Ok(())
    }

    pub(super) fn deliver<T>(&self, db: &dyn Database, result: RunResult<T>) -> RunResult<T> {
        let error = match result {
            Ok(value) => match self.check(db) {
                Ok(()) => return Ok(value),
                Err(error) => {
                    // A rejected value can call back when dropped. Validate the recipient again
                    // after disposal, before attaching the failed operation to its native caller.
                    drop(value);
                    error
                }
            },
            Err(error) => error,
        };
        if self.check_owner_with_driver(db, self.driver).is_ok()
            && let Some(reason) = match error {
                RunError::Refused(reason) => Some(reason),
                RunError::Contract(_) | RunError::RequiresFetch | RunError::Preparation(_) => {
                    self.support.reason()
                }
            }
        {
            attempt_probe::report_incomplete(db, reason);
        }
        Err(error)
    }

    fn check_depth(&self, db: &dyn Database) -> RunResult<()> {
        let depth = self.driver.map_or(Some(1), |driver| driver.depth.checked_add(1))
            .ok_or(RunError::Contract("native callback depth overflow"))?;
        if depth > self.limits.max_active_runs.get() {
            #[cfg(test)]
            record(NativeTrace::Denied {
                depth,
                limit: self.limits.max_active_runs.get(),
                remaining: attempt_probe::remaining_allowance_for_diagnostics(db),
                caller: self.caller,
            });
            return Err(RunError::Refused(attempt_probe::report_incomplete(db, Incomplete::Allowance)));
        }
        Ok(())
    }
}

/// Reuses the installed attempt and gives this native callback a distinct lexical owner.
/// The closure cannot return an entry-authorized registry, but may return a database-lived memo.
pub fn with_native_callback<T>(
    db: &dyn Database,
    limits: NativeCallbackLimits,
    body: impl for<'entry> FnOnce(NativeCallbackEntry<'entry>) -> RunResult<T>,
) -> RunResult<T> {
    let outer = NativeEntryReceipt::capture_owner(db, limits)?;
    let result = (|| {
        outer.check(db)?;
        outer.check_depth(db)?;
        attempt_probe::try_with_operation(db, || {
            let receipt = NativeEntryReceipt::capture(db, limits)?;
            let value = body(NativeCallbackEntry { receipt: &receipt });
            // An original failure wins; a successful value remains held through restoration checks.
            match value {
                Ok(value) => { receipt.check(db)?; Ok(value) }
                Err(error) => Err(error),
            }
        }).map_err(|error| match error {
            StartError::ScopeIdentityExhausted => RunError::Contract("native callback scope identity exhausted"),
            StartError::ActiveQuery | StartError::ActiveOperation | StartError::NestedAttempt
                | StartError::ConcurrentAttempt => RunError::Contract("native callback operation scope rejected"),
        })?
    })();
    // Failed registered frames have already retired. Their native caller needs the same
    // incomplete support before it can publish a fallback derived from the refused result.
    outer.deliver(db, result)
}

pub(super) struct DriverLease {
    identity: DriverIdentity,
    previous: Option<DriverIdentity>,
}

impl DriverLease {
    pub(super) fn enter(context: &RunContext<'_>) -> RunResult<Self> {
        let previous = current_driver();
        let limits = match &context.native_entry {
            Some(entry) => {
                entry.check(context.db)?;
                entry.check_depth(context.db)?;
                attempt_probe::charge(context.db, 1).map_err(RunError::Refused)?;
                Some(entry.limits)
            }
            None if previous.is_none() => None,
            None => return Err(RunError::Contract("nested execution driver")),
        };
        let id = NEXT_DRIVER.with(|next| {
            let id = next.get().ok_or(RunError::Contract("execution driver identity exhausted"))?;
            next.set(id.get().checked_add(1).and_then(NonZeroUsize::new));
            Ok(id)
        })?;
        let depth = previous.map_or(Some(1), |driver| driver.depth.checked_add(1))
            .ok_or(RunError::Contract("execution driver depth overflow"))?;
        let identity = DriverIdentity { id, depth, limits };
        let lease = Self { identity, previous };
        CURRENT_DRIVER.with(|current| current.set(Some(identity)));
        #[cfg(test)]
        {
            super::RUN_ACTIVE.with(|active| active.set(true));
            record(NativeTrace::Enter { identity, parent: previous, caller: Caller::capture_db(context.db)? });
        }
        Ok(lease)
    }

    pub(super) fn identity(&self) -> DriverIdentity { self.identity }

    pub(super) fn check_context(&self, context: &RunContext<'_>) -> RunResult<()> {
        if current_driver() != Some(self.identity) {
            return Err(RunError::Contract("execution driver lease changed during admission"));
        }
        context.check_owner_baseline()?;
        if let Some(entry) = &context.native_entry {
            entry.check_with_driver(context.db, Some(self.identity))?;
        }
        Ok(())
    }
}

impl Drop for DriverLease {
    fn drop(&mut self) {
        CURRENT_DRIVER.with(|current| current.set(self.previous));
        #[cfg(test)]
        {
            super::RUN_ACTIVE.with(|active| active.set(self.previous.is_some()));
            record(NativeTrace::Exit { identity: self.identity });
        }
    }
}

struct SourcePause<'queue, 'run> {
    queue: &'queue Queue<'run>,
    permit: SourcePermit,
    previous: Option<SourcePermit>,
    poll: super::progress::PollIdentity,
}

impl<'queue, 'run> SourcePause<'queue, 'run> {
    fn enter(endpoint: &'queue Endpoint<'run, '_>) -> RunResult<Self> {
        let access = endpoint.queue.begin_access()?;
        let receipt = endpoint.callback_poll()
            .ok_or(RunError::Contract("native source requires an active task poll"))?;
        receipt.check_completion()?;
        let caller = Caller::capture_db(endpoint.context.db)?;
        let queue = endpoint.queue.as_ref();
        let epoch = queue.pause_epoch.get().checked_add(1)
            .ok_or(RunError::Contract("native source pause identity exhausted"))?;
        let permit = SourcePermit {
            queue: std::ptr::from_ref(queue).addr(),
            epoch,
            driver: queue.driver,
            caller,
            database: std::ptr::from_ref(endpoint.context.db.zalsa()).addr(),
            local: std::ptr::from_ref(endpoint.context.db.zalsa_local()).addr(),
        };
        access.check()?;
        let previous = SOURCE_PERMIT.with(|current| current.replace(Some(permit)));
        queue.pause_epoch.set(epoch);
        queue.paused.set(true);
        #[cfg(test)]
        record(NativeTrace::SourceEnter { caller, driver: permit.driver, epoch, queue: permit.queue });
        Ok(Self { queue, permit, previous, poll: receipt.identity() })
    }

    fn finish(self) -> RunResult<()> {
        if current_driver() != Some(self.permit.driver)
            || SOURCE_PERMIT.with(Cell::get) != Some(self.permit)
            || !self.queue.paused.get()
            || self.queue.pause_epoch.get() != self.permit.epoch
            || !self.poll.is_current(self.queue)
        {
            return Err(RunError::Contract("native source did not restore its parent owner"));
        }
        Ok(())
    }
}

impl Drop for SourcePause<'_, '_> {
    fn drop(&mut self) {
        SOURCE_PERMIT.with(|current| current.set(self.previous));
        self.queue.paused.set(false);
        #[cfg(test)]
        record(NativeTrace::SourceExit { driver: self.permit.driver, epoch: self.permit.epoch });
    }
}

pub(super) async fn read<'call, 'run: 'call, 'db: 'run, C: Configuration>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    route: &'call NativeSourceRoute<'db, C>,
    id: Id,
) -> &'db C::Output<'db> {
    if !endpoint.inner.local_call_is_eligible() { return pending().await; }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, route).await {},
    };
    callback::complete_local_immediate(&endpoint.inner, &scope, || {
        endpoint.admit_work(1)?;
        endpoint.check_native_source(route)?;
        let pause = SourcePause::enter(&endpoint.inner)?;
        #[cfg(test)]
        record(NativeTrace::Read {
            key: route.ingredient.database_key_index(id),
        });
        let value = route
            .ingredient
            .fetch(route.db, route.db.zalsa(), route.db.zalsa_local(), id);
        if !scope.is_current() {
            return Err(RunError::Contract("native source changed its caller"));
        }
        pause.finish()?;
        Ok(value)
    })
    .await
}

pub(super) async fn validate<'run, 'db: 'run, C: Configuration>(
    endpoint: TaskEndpoint<'run, 'db>,
    route: NativeSourceRoute<'db, C>,
    id: Id,
    revision: Revision,
) -> RunResult<VerifyResult> {
    if !endpoint.inner.local_call_is_eligible() { return pending().await; }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, route).await {},
    };
    let value = callback::complete_local_immediate(&endpoint.inner, &scope, || {
        endpoint.admit_work(1)?;
        endpoint.check_native_source(&route)?;
        let pause = SourcePause::enter(&endpoint.inner)?;
        #[cfg(test)]
        record(NativeTrace::Validation {
            key: route.ingredient.database_key_index(id),
            revision,
        });
        let value = route.ingredient.maybe_changed_after(route.db, id, revision);
        if !scope.is_current() {
            return Err(RunError::Contract(
                "native source validation changed its caller",
            ));
        }
        pause.finish()?;
        Ok(value)
    })
    .await;
    Ok(value)
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum NativeTrace {
    Enter { identity: DriverIdentity, parent: Option<DriverIdentity>, caller: Caller },
    Exit { identity: DriverIdentity },
    Denied { depth: usize, limit: usize, remaining: Option<usize>, caller: Caller },
    SourceEnter { caller: Caller, driver: DriverIdentity, epoch: usize, queue: usize },
    SourceExit { driver: DriverIdentity, epoch: usize },
    Read { key: DatabaseKeyIndex },
    Validation { key: DatabaseKeyIndex, revision: Revision },
}

#[cfg(test)]
thread_local! { static TRACE: RefCell<Option<Vec<NativeTrace>>> = const { RefCell::new(None) }; }

#[cfg(test)]
fn record(event: NativeTrace) {
    TRACE.with_borrow_mut(|trace| { if let Some(trace) = trace { trace.push(event); } });
}

#[cfg(test)]
pub(super) struct TraceGuard;

#[cfg(test)]
impl Drop for TraceGuard {
    fn drop(&mut self) { TRACE.with_borrow_mut(|trace| *trace = None); }
}

#[cfg(test)]
pub(super) fn trace() -> TraceGuard {
    TRACE.with_borrow_mut(|trace| assert!(trace.replace(Vec::new()).is_none()));
    TraceGuard
}

#[cfg(test)]
pub(super) fn take_trace() -> Vec<NativeTrace> {
    TRACE.with_borrow_mut(|trace| trace.as_mut().map(std::mem::take).unwrap_or_default())
}

#[cfg(test)]
pub(super) fn source_permit_is_clear() -> bool { SOURCE_PERMIT.with(Cell::get).is_none() }

#[cfg(test)]
pub(super) fn layout() -> [(usize, usize); 4] {
    [
        (size_of::<DriverLease>(), align_of::<DriverLease>()),
        (size_of::<NativeEntryReceipt>(), align_of::<NativeEntryReceipt>()),
        (size_of::<SourcePause<'_, '_>>(), align_of::<SourcePause<'_, '_>>()),
        (size_of::<SourcePermit>(), align_of::<SourcePermit>()),
    ]
}
