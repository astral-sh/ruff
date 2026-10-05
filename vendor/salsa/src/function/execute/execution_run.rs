//! Private execution ownership for an exclusive, depth-first attempt.
//!
//! Providers can demand child tasks, but the driver retains every query request. A provider
//! cannot remove an ancestor's active frame while its child is still running. Registered fetch
//! and validation use the shared runtime states; generated query entries remain separate.

use std::cell::{Cell, RefCell};
use std::future::{Future, ready};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use super::{
    BodyContinuation, CompletionDisposition, CompletionPreparation, CycleComparison, CycleInitial,
    ExecutionStep, ExpandHeads, FinalizedCycle, InitialContinuation, InitialReplacement,
    IteratingQuery, PreparedQueryCommit, QueryBody, QueryExecution, RecoveryContinuation,
};
use crate::attempt_probe::{
    self, AttemptSupport, Incomplete, LocalOwnershipReceipt, Operation, OperationPolicy,
    QueryPolicy,
};
#[cfg(test)]
use crate::function::memo::SelectedMemo;
use crate::function::{ClaimGuard, Configuration, IngredientImpl, Memo};
#[cfg(test)]
use crate::function::{ClaimResult, Reentrancy};
use crate::sync::thread;
#[cfg(test)]
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::ActiveQueryGuard;
use crate::{Cancelled, Cycle, Database, Id};

mod admission;
mod callback;
pub(crate) mod explicit_reads;
mod fetch_run;
mod field_run;
mod frame_free;
mod key_run;
mod native_source;
mod native_values;
mod passive_memos;
mod prepared_source_run;
mod progress;
mod read_run;
pub mod registration;
mod source_run;
mod structural_dependencies;
mod structural_preparation;
mod task;
mod validation_run;

use frame_free::{CallbackScope, Caller, FrameFreeRequest, OwnedFrameFree};
use native_values::{NativeValueOperation, NativeValueQuote};
use progress::{ActivePoll, PollGeneration, PollGuard, TerminalDisposition};
use task::{Task, TypedTask};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunError {
    Refused(Incomplete),
    Contract(&'static str),
    RequiresFetch,
    Preparation(crate::prepared_source_probe::PreparationError),
}

pub type RunResult<T> = Result<T, RunError>;

/// Work is admitted before requesting its allocation or polling its future.
/// Byte charges describe requested payload and bookkeeping, not allocator backing or RSS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionWork {
    Task { requested_bytes: usize },
    Resource { requested_bytes: usize },
    Work { units: usize },
    Poll,
}

pub trait ExecutionAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()>;
}

#[cfg(test)]
struct Unrestricted;

#[cfg(test)]
impl ExecutionAdmission for Unrestricted {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

struct RunContext<'db> {
    db: &'db dyn Database,
    support: AttemptSupport,
    initial_depths: (usize, usize),
    initial_query_depth: usize,
    initial_caller: Caller,
    initial_ownership: LocalOwnershipReceipt,
    explicit_read_scope: Option<explicit_reads::ScopeId>,
    native_entry: Option<native_source::NativeEntryReceipt>,
    operations: RefCell<Vec<usize>>,
    next_operation: Cell<usize>,
    reason: Cell<Option<Incomplete>>,
}

impl<'db> RunContext<'db> {
    fn new(db: &'db dyn Database) -> RunResult<Self> {
        let support = attempt_probe::current().ok_or(RunError::Contract(
            "execution run requires an installed attempt",
        ))?;
        if !support.is_current(db.zalsa()) || db.zalsa_local().active_query().is_some() {
            return Err(RunError::Contract(
                "execution run has a foreign or active query",
            ));
        }
        if attempt_probe::current_policy() == QueryPolicy::CompleteOnly {
            return Err(RunError::Contract(
                "execution run has a complete-only ancestor",
            ));
        }
        if attempt_probe::stack_depths().0 != 0 {
            return Err(RunError::Contract(
                "execution run cannot nest inside a query operation",
            ));
        }
        Self::capture(db, support, None)
    }

    fn for_native_callback(
        db: &'db dyn Database,
        entry: &native_source::NativeEntryReceipt,
    ) -> RunResult<Self> {
        entry.check(db)?;
        Self::capture(db, entry.support.clone(), Some(entry.clone()))
    }

    fn capture(
        db: &'db dyn Database,
        support: AttemptSupport,
        native_entry: Option<native_source::NativeEntryReceipt>,
    ) -> RunResult<Self> {
        let initial_query_depth = db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.len())
            .ok_or(RunError::Contract("query stack is borrowed"))?;
        let initial_ownership = support
            .local_ownership(db.zalsa())
            .ok_or(RunError::Contract("execution run has a foreign attempt"))?;
        Ok(Self {
            db,
            support,
            initial_depths: attempt_probe::stack_depths(),
            initial_query_depth,
            initial_caller: Caller::capture_db(db)?,
            initial_ownership,
            explicit_read_scope: explicit_reads::current_scope(),
            native_entry,
            operations: RefCell::new(Vec::new()),
            next_operation: Cell::new(0),
            reason: Cell::new(None),
        })
    }

    fn refuse(&self, reason: Incomplete) -> Incomplete {
        assert!(
            self.support.owns_current_session(self.db.zalsa()),
            "execution run lost its attempt"
        );
        let reason = attempt_probe::report_incomplete(self.db, self.reason.get().unwrap_or(reason));
        self.reason.set(Some(reason));
        reason
    }

    fn observe<T>(&self, result: RunResult<T>) -> RunResult<T> {
        explicit_reads::check_acceptance();
        match result {
            Ok(value) => {
                self.check_resume()?;
                Ok(value)
            }
            Err(error @ RunError::Refused(reason)) => {
                self.refuse(reason);
                Err(error)
            }
            Err(other) => {
                self.refuse(Incomplete::Interrupted);
                Err(other)
            }
        }
    }

    fn check_resume(&self) -> RunResult<()> {
        explicit_reads::check_acceptance();
        explicit_reads::check_scope(self.explicit_read_scope)?;
        if !self.support.owns_current_session(self.db.zalsa()) {
            return Err(RunError::Contract("execution run has a foreign attempt"));
        }
        if let Some(reason) = self.reason.get().or_else(|| self.support.reason()) {
            return Err(RunError::Refused(self.refuse(reason)));
        }
        // Revision cancellation remains Salsa cancellation, with its original panic payload.
        self.db
            .zalsa()
            .unwind_if_revision_cancelled(self.db.zalsa_local());
        explicit_reads::check_acceptance();
        if !self.support.is_current(self.db.zalsa()) {
            return Err(RunError::Contract(
                "execution run changed attempt or revision",
            ));
        }
        // The cancellation event callback can record refusal without changing the revision.
        if let Some(reason) = self.reason.get().or_else(|| self.support.reason()) {
            return Err(RunError::Refused(self.refuse(reason)));
        }
        Ok(())
    }

    fn assert_finished(&self) {
        assert!(
            self.operations.borrow().is_empty(),
            "execution operations survived their driver"
        );
        assert_eq!(attempt_probe::stack_depths(), self.initial_depths);
        assert_eq!(
            self.db
                .zalsa_local()
                .try_with_query_stack(|stack| stack.len()),
            Some(self.initial_query_depth)
        );
        assert!(self.initial_caller.is_current_db(self.db));
        assert_eq!(
            self.support.local_ownership(self.db.zalsa()),
            Some(self.initial_ownership)
        );
    }

    fn check_start(&self) -> RunResult<()> {
        if let Some(entry) = &self.native_entry {
            entry.check(self.db)?;
        }
        self.check_resume()?;
        self.check_baseline()
    }

    fn check_baseline(&self) -> RunResult<()> {
        self.check_owner_baseline()?;
        if let Some(entry) = &self.native_entry {
            entry.check(self.db)?;
        }
        Ok(())
    }

    fn check_owner_baseline(&self) -> RunResult<()> {
        if attempt_probe::stack_depths() != self.initial_depths
            || self
                .db
                .zalsa_local()
                .try_with_query_stack(|stack| stack.len())
                != Some(self.initial_query_depth)
            || self.support.local_ownership(self.db.zalsa()) != Some(self.initial_ownership)
            || !self.initial_caller.is_current_db(self.db)
        {
            return Err(RunError::Contract(
                "execution registration changed its enclosing scope",
            ));
        }
        Ok(())
    }
}

struct RunOperation<'db> {
    operation: Option<Operation<'db>>,
    policy: OperationPolicy,
    context: Rc<RunContext<'db>>,
    ordinal: usize,
}

struct PreparedOperationOrdinal {
    ordinal: usize,
    next: usize,
    policy: OperationPolicy,
}

impl<'db> RunOperation<'db> {
    #[cfg(test)]
    fn enter<C: Configuration>(context: Rc<RunContext<'db>>) -> RunResult<Self> {
        let prepared = Self::prepare::<C>(&context, OperationPolicy::ordinary(C::ATTEMPT_POLICY))?;
        Self::enter_admitted::<C>(context, prepared)
    }

    fn prepare<C: Configuration>(
        context: &RunContext<'db>,
        policy: OperationPolicy,
    ) -> RunResult<PreparedOperationOrdinal> {
        context.check_resume()?;
        if policy.declared != C::ATTEMPT_POLICY
            || !matches!(
                policy.declared,
                QueryPolicy::ReturnOnly | QueryPolicy::CompleteOnly
            )
            || !policy.allows_incomplete()
        {
            return Err(RunError::Contract("only return-only execution can suspend"));
        }
        attempt_probe::check_admitted_entry(policy.declared).map_err(RunError::Contract)?;
        let ordinal = context.next_operation.get();
        let next = ordinal
            .checked_add(1)
            .ok_or(RunError::Contract("execution operation identity exhausted"))?;
        Ok(PreparedOperationOrdinal {
            ordinal,
            next,
            policy,
        })
    }

    fn enter_admitted<C: Configuration>(
        context: Rc<RunContext<'db>>,
        prepared: PreparedOperationOrdinal,
    ) -> RunResult<Self> {
        let PreparedOperationOrdinal {
            ordinal,
            next,
            policy,
        } = prepared;
        if context.next_operation.get() != ordinal {
            return Err(RunError::Contract("execution operation identity changed"));
        }
        let operation = if policy == OperationPolicy::ordinary(C::ATTEMPT_POLICY) {
            attempt_probe::check_admitted_entry(policy.declared).map_err(RunError::Contract)?;
            attempt_probe::enter(context.db.zalsa(), policy.declared, C::DEBUG_NAME)
        } else {
            attempt_probe::enter_admitted(context.db.zalsa(), policy.declared, C::DEBUG_NAME)
                .map_err(RunError::Contract)?
        };
        context.next_operation.set(next);
        context.operations.borrow_mut().push(ordinal);
        Ok(Self {
            operation: Some(operation),
            policy,
            context,
            ordinal,
        })
    }

    fn is_current(&self) -> bool {
        self.context.operations.borrow().last() == Some(&self.ordinal)
            && self
                .operation
                .as_ref()
                .is_some_and(|operation| operation.is_current_execution(self.policy))
    }
}

impl Drop for RunOperation<'_> {
    fn drop(&mut self) {
        assert!(
            self.is_current(),
            "execution operations dropped out of order"
        );
        drop(self.operation.take());
        let ordinal = self.context.operations.borrow_mut().pop();
        assert_eq!(ordinal, Some(self.ordinal));
    }
}

trait AbortRequest {
    fn active_query(&self) -> &ActiveQueryGuard<'_>;
    fn abort(self, depth: usize);
}

impl<C: Configuration> FrameFreeRequest for super::participant::Participant<'_, C> {
    fn abort(self) {
        super::participant::Participant::abort(self);
    }

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        frame_free::trace_owner("participant", self.key(), operation, caller);
    }
}

async fn retire_participant_owned<'run, 'db: 'run, C: Configuration>(
    endpoint: &Endpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    mut request: super::participant::Participant<'db, C>,
) -> super::participant::ParticipantProgress<'db, C> {
    loop {
        let caller = match Caller::capture(operation) {
            Ok(caller) => caller,
            Err(error) => match callback::reject(endpoint, error, request).await {},
        };
        let owned = OwnedFrameFree::new(request, operation, caller);
        let work = match owned.admitted() {
            Ok(request) => match request.work() {
                Some(work) => work,
                None => match callback::reject(
                    endpoint,
                    RunError::Contract("participant work size overflow"),
                    owned,
                )
                .await {},
            },
            Err(error) => match callback::reject(endpoint, error, owned).await {},
        };
        callback::complete(endpoint, &owned, callback::CallbackKind::Canonical, || {
            ready(
                endpoint
                    .admit(ExecutionWork::Resource {
                        requested_bytes: work.bytes(),
                    })
                    .and_then(|()| {
                        attempt_probe::charge(endpoint.context.db, work.units())
                            .map_err(RunError::Refused)
                    })
                    .and_then(|()| {
                        endpoint.admit(ExecutionWork::Work {
                            units: work.units(),
                        })
                    }),
            )
        })
        .await;
        request = match owned.take_admitted() {
            Ok(request) => request,
            Err((error, owned)) => match callback::reject(endpoint, error, owned).await {},
        };
        match request.advance(work) {
            super::participant::ParticipantProgress::Pending(next) => request = next,
            result => return result,
        }
    }
}

impl<C: Configuration> FrameFreeRequest for QueryExecution<'_, C> {
    fn abort(self) {
        self.claim_guard.abort();
    }

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        frame_free::trace_owner(
            "execute.start",
            Some(self.claim_guard.database_key_index()),
            operation,
            caller,
        );
    }
}

impl<C: Configuration> FrameFreeRequest for CompletionPreparation<'_, C> {
    fn abort(self) {
        drop(self.value);
        drop(self.completed);
        abort_completion(self.disposition);
    }

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        frame_free::trace_owner(
            "execute.prepare",
            Some(
                self.disposition
                    .execution()
                    .claim_guard
                    .database_key_index(),
            ),
            operation,
            caller,
        );
    }
}

fn abort_completion<C: Configuration>(disposition: CompletionDisposition<'_, C>) {
    match disposition {
        CompletionDisposition::Return {
            cancellation_guard,
            execution,
        } => {
            execution.claim_guard.abort();
            drop(cancellation_guard);
        }
        CompletionDisposition::Repeat { query, .. } => abort_iteration(query),
        CompletionDisposition::Finalize {
            poison_guard,
            cancellation_guard,
            execution,
            ..
        } => {
            drop(poison_guard);
            execution.claim_guard.abort();
            drop(cancellation_guard);
        }
    }
}

impl<C: Configuration> FrameFreeRequest for PreparedQueryCommit<'_, C> {
    fn abort(self) {
        drop(self.memo);
        drop(self.targets);
        drop(self.stale_tracked_structs);
        abort_completion(self.disposition);
    }

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        frame_free::trace_owner(
            "execute.commit",
            Some(
                self.disposition
                    .execution()
                    .claim_guard
                    .database_key_index(),
            ),
            operation,
            caller,
        );
    }
}

impl<C: Configuration> FrameFreeRequest for FinalizedCycle<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        frame_free::trace_owner("execute.finalized", Some(self.key), operation, caller);
    }
}

impl<C: Configuration> AbortRequest for ExpandHeads<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.active_query
    }
    fn abort(self, depth: usize) {
        drop(self.new_value);
        self.active_query.abort_return_only(depth);
        abort_iteration(self.query);
    }
}

impl<C: Configuration> AbortRequest for QueryBody<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.active_query
    }

    fn abort(self, depth: usize) {
        self.active_query.abort_return_only(depth);
        match self.continuation {
            BodyContinuation::Ordinary(execution) => execution.claim_guard.abort(),
            BodyContinuation::Iterating(query) => abort_iteration(query),
        }
    }
}

impl<C: Configuration> AbortRequest for CycleInitial<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.active_query
    }

    fn abort(self, depth: usize) {
        // A destructor runs with the same active query and policy as the value it replaces.
        drop(self.computed_value);
        self.active_query.abort_return_only(depth);
        match self.continuation {
            InitialContinuation::Participant(participant) => abort_iteration(participant.query),
            InitialContinuation::Head(head) => abort_iteration(head.query),
        }
    }
}

impl<C: Configuration> AbortRequest for RecoveryContinuation<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.active_query
    }

    fn abort(self, depth: usize) {
        self.active_query.abort_return_only(depth);
        abort_iteration(self.head.query);
    }
}

impl<C: Configuration> AbortRequest for InitialReplacement<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.active_query
    }

    fn abort(self, depth: usize) {
        drop(self.old_value);
        drop(self.replacement);
        self.active_query.abort_return_only(depth);
        match self.continuation {
            InitialContinuation::Participant(participant) => abort_iteration(participant.query),
            InitialContinuation::Head(head) => abort_iteration(head.query),
        }
    }
}

impl<C: Configuration> AbortRequest for CycleComparison<'_, C> {
    fn active_query(&self) -> &ActiveQueryGuard<'_> {
        &self.continuation.active_query
    }

    fn abort(self, depth: usize) {
        drop(self.new_value);
        self.continuation.abort(depth);
    }
}

fn abort_iteration<C: Configuration>(query: IteratingQuery<'_, C>) {
    drop(query.poison_guard);
    drop(query.last_stale_tracked_ids);
    query.execution.claim_guard.abort();
    drop(query.cancellation_guard);
}

struct OwnedRequest<'a, 'db, Q: AbortRequest> {
    request: Option<Q>,
    operation: &'a RunOperation<'db>,
    depth: usize,
}

impl<'a, 'db, Q: AbortRequest> OwnedRequest<'a, 'db, Q> {
    fn new(request: Q, operation: &'a RunOperation<'db>) -> Self {
        let depth = operation
            .context
            .db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.len())
            .expect("query stack is borrowed while suspending execution");
        assert!(operation.is_current() && request.active_query().is_current_at_depth(depth));
        Self {
            request: Some(request),
            operation,
            depth,
        }
    }

    fn get(&self) -> RunResult<&Q> {
        self.request
            .as_ref()
            .ok_or(RunError::Contract("execution request already consumed"))
    }

    fn split(&mut self) -> RunResult<(&mut Q, CallbackScope<'a, 'db>)> {
        let request = self
            .request
            .as_mut()
            .ok_or(RunError::Contract("execution request already consumed"))?;
        let caller = Caller::for_active_query(request.active_query(), self.depth);
        Ok((request, CallbackScope::new(self.operation, caller)))
    }

    fn take_admitted(mut self) -> Result<Q, (RunError, Self)> {
        if !callback::CallbackOwner::is_current(&self) {
            return Err((
                RunError::Contract("execution resumed beneath its child"),
                self,
            ));
        }
        match self.request.take() {
            Some(request) => Ok(request),
            None => Err((
                RunError::Contract("execution request already consumed"),
                self,
            )),
        }
    }
}

impl<Q: AbortRequest> callback::CallbackOwner for OwnedRequest<'_, '_, Q> {
    fn check_resume(&self) -> RunResult<()> {
        self.operation.context.check_resume()?;
        if !self.is_current() {
            return Err(RunError::Contract("execution resumed beneath its child"));
        }
        Ok(())
    }

    fn is_current(&self) -> bool {
        self.operation.is_current()
            && self
                .request
                .as_ref()
                .is_some_and(|request| request.active_query().is_current_at_depth(self.depth))
    }
}

impl<Q: AbortRequest> Drop for OwnedRequest<'_, '_, Q> {
    fn drop(&mut self) {
        let Some(request) = self.request.take() else {
            return;
        };
        if thread::panicking() {
            // The existing request field order handles query pop, poison, cancellation and claim.
            drop(request);
        } else {
            assert!(
                self.operation.is_current()
                    && request.active_query().is_current_at_depth(self.depth)
            );
            // A refused child never ran a fetch epilogue. Attach support to every ancestor here.
            self.operation.context.refuse(Incomplete::Interrupted);
            #[cfg(test)]
            tests::fetch::cleanup::observe_abort(self.operation.context.db, self.depth);
            request.abort(self.depth);
        }
    }
}

#[cfg(test)]
trait Provider<'run, 'db: 'run, C: Configuration> {
    type Output: 'run;

    async fn native_value(
        &self,
        _db: &'db C::DbView,
        _operation: NativeValueOperation<'_, 'db, C>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<NativeValueQuote> {
        Err(RunError::Contract("native value operation has no profile"))
    }

    async fn body(
        &self,
        db: &'db C::DbView,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<C::Output<'db>>;
    async fn initial(
        &self,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<C::Output<'db>>;
    async fn recover(
        &self,
        db: &'db C::DbView,
        cycle: &Cycle<'_>,
        last: &C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<C::Output<'db>>;

    /// This receives the exact selected memo, or the existing transfer/refetch decision.
    /// The query operation remains installed until this future completes or is dropped.
    async fn complete(
        &self,
        db: &'db C::DbView,
        memo: Option<SelectedMemo<'db, C>>,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<Self::Output>;
}

/// Execution callbacks cannot select memos or complete the caller's validation/fetch operation.
trait ExecutionProvider<'run, 'db: 'run, C: Configuration> {
    fn operation_policy(&self) -> OperationPolicy {
        OperationPolicy::ordinary(QueryPolicy::ReturnOnly)
    }

    fn native_value<'call>(
        &'call self,
        _db: &'db C::DbView,
        _operation: NativeValueOperation<'call, 'db, C>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract(
            "native value operation has no profile",
        )))
    }

    fn body<'call>(
        &'call self,
        db: &'db C::DbView,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;
    fn initial<'call>(
        &'call self,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;
    fn recover<'call>(
        &'call self,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;
}

#[cfg(test)]
struct ScriptCallbacks<'a, P>(&'a P);

#[cfg(test)]
impl<'run, 'db: 'run, C: Configuration, P: Provider<'run, 'db, C>> ExecutionProvider<'run, 'db, C>
    for ScriptCallbacks<'_, P>
{
    fn native_value<'call>(
        &'call self,
        db: &'db C::DbView,
        operation: NativeValueOperation<'call, 'db, C>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        self.0.native_value(db, operation, endpoint)
    }

    fn body<'call>(
        &'call self,
        db: &'db C::DbView,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.0.body(db, input, endpoint)
    }
    fn initial<'call>(
        &'call self,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.0.initial(db, id, input, endpoint)
    }
    fn recover<'call>(
        &'call self,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
        endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.0.recover(db, cycle, last, value, input, endpoint)
    }
}

#[cfg(test)]
async fn execute<'run, 'db: 'run, C, P>(
    endpoint: Endpoint<'run, 'db>,
    ingredient: &'db IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
    old_memo: Option<&'db Memo<C>>,
    provider: P,
) -> RunResult<P::Output>
where
    C: Configuration,
    P: Provider<'run, 'db, C>,
{
    if !std::ptr::eq(db.zalsa(), endpoint.context.db.zalsa())
        || !std::ptr::eq(db.zalsa_local(), endpoint.context.db.zalsa_local())
    {
        return Err(RunError::Contract(
            "execution uses a foreign database or worker",
        ));
    }
    let operation = RunOperation::enter::<C>(endpoint.context.clone())?;
    let consumer = super::participant::Consumer::capture(db.zalsa_local());
    let claim =
        match ingredient
            .sync_table
            .try_claim(db.zalsa(), db.zalsa_local(), id, Reentrancy::Allow)
        {
            ClaimResult::Claimed(claim) => claim,
            // This entry begins after the caller has selected reexecution. It is not a fetch solver.
            ClaimResult::Cycle { .. } | ClaimResult::Running(_) => {
                return Err(RunError::RequiresFetch);
            }
        };
    let memo = execute_owned(
        endpoint.clone(),
        &operation,
        ingredient,
        db,
        claim,
        old_memo,
        consumer,
        &ScriptCallbacks(&provider),
    )
    .await?;
    let selected = memo
        .map(|memo| {
            SelectedMemo::new(memo).ok_or(RunError::Contract("completed execution has no value"))
        })
        .transpose()?;
    endpoint
        .context
        .observe(provider.complete(db, selected, endpoint.clone()).await)
}

async fn execute_owned<'run, 'db: 'run, C, P>(
    endpoint: Endpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    ingredient: &'db IngredientImpl<C>,
    db: &'db C::DbView,
    claim: ClaimGuard<'db>,
    old_memo: Option<&'db Memo<C>>,
    consumer: super::participant::Consumer,
    provider: &P,
) -> RunResult<Option<&'db Memo<C>>>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
{
    execute_request_owned(
        endpoint,
        operation,
        QueryExecution::start(ingredient, db, claim, old_memo, consumer),
        provider,
    )
    .await
}

async fn execute_request_owned<'run, 'db: 'run, C, P>(
    endpoint: Endpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    request: QueryExecution<'db, C>,
    provider: &P,
) -> RunResult<Option<&'db Memo<C>>>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
{
    let db = request.db;

    let claim = &request.claim_guard;
    #[cfg(test)]
    let old_memo = request.previous.as_ref().map(super::PreviousMemo::retained);
    let execution_caller = match Caller::capture(operation) {
        Ok(caller) => caller,
        Err(error) => match callback::reject(&endpoint, error, request).await {},
    };
    let id = claim.database_key_index().key_index();
    #[cfg(test)]
    tests::observation::record(tests::observation::Event::Execute {
        key: claim.database_key_index(),
        serial: claim.test_serial(),
        operation: operation.ordinal,
        old_memo: old_memo.map(|memo| std::ptr::from_ref(memo).addr()),
    });
    let mut step = ExecutionStep::Start(request);
    loop {
        step = match step {
            ExecutionStep::Start(request) => {
                let caller = match Caller::capture(operation) {
                    Ok(caller) => caller,
                    Err(error) => match callback::reject(&endpoint, error, request).await {},
                };
                let mut owned = OwnedFrameFree::new(request, operation, caller);
                let (request, scope) = match owned.split() {
                    Ok(parts) => parts,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                callback::complete(&endpoint, &scope, callback::CallbackKind::Canonical, || {
                    if C::ATTEMPT_POLICY == QueryPolicy::CompleteOnly
                        && let Some(previous) = &request.previous
                    {
                        let header = previous.output_header();
                        let units = header.output_check_work(super::MemoOutputCheck::Controlled);
                        if units != 0
                            && let Err(error) = endpoint.admit_work(units)
                        {
                            return ready(Err(error));
                        }
                        if !header.controlled_outputs_are_empty() {
                            return ready(Err(RunError::Contract(
                                "controlled callable requires a memo without outputs or direct accumulators",
                            )));
                        }
                    }
                    request.prepare_start();
                    ready(Ok(()))
                })
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume_start(),
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::Body(request) => {
                let owned = OwnedRequest::new(request, operation);
                let input = native_values::input(&endpoint, &owned, provider, db, id).await;
                let value = callback::complete(
                    &endpoint,
                    &owned,
                    callback::CallbackKind::Execution,
                    || provider.body(db, input, endpoint.clone()),
                )
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume(value),
                    Err((error, owned)) => {
                        match callback::reject(&endpoint, error, (value, owned)).await {}
                    }
                }
            }
            ExecutionStep::Initial(request) => {
                let owned = OwnedRequest::new(request, operation);
                let input = native_values::input(&endpoint, &owned, provider, db, id).await;
                let value = callback::complete(
                    &endpoint,
                    &owned,
                    callback::CallbackKind::Execution,
                    || provider.initial(db, id, input, endpoint.clone()),
                )
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume(value),
                    Err((error, owned)) => {
                        match callback::reject(&endpoint, error, (value, owned)).await {}
                    }
                }
            }
            ExecutionStep::RetireInitial(request) => {
                let mut owned = OwnedRequest::new(request, operation);
                let (request, scope) = match owned.split() {
                    Ok(parts) => parts,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                callback::complete(&endpoint, &scope, callback::CallbackKind::Canonical, || {
                    request.retire();
                    ready(Ok(()))
                })
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume(),
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::Recovery(request) => {
                let super::CycleRecovery {
                    continuation,
                    new_value,
                } = request;
                let owned = OwnedRequest::new(continuation, operation);
                // Drop an unconsumed recovery value before its owning query frame on refusal.
                let new_value = new_value;
                let input = native_values::input(&endpoint, &owned, provider, db, id).await;
                let head = match owned.get() {
                    Ok(request) => &request.head,
                    Err(error) => {
                        match callback::reject(&endpoint, error, (input, new_value, owned)).await {}
                    }
                };
                let cycle = Cycle {
                    head_ids: head.cycle_heads.ids(),
                    id,
                    iteration: head.cycle_iteration.iteration_as_u32(),
                };
                let value = callback::complete(
                    &endpoint,
                    &owned,
                    callback::CallbackKind::Execution,
                    || {
                        provider.recover(
                            db,
                            &cycle,
                            head.last_provisional_value,
                            new_value,
                            input,
                            endpoint.clone(),
                        )
                    },
                )
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume(value),
                    Err((error, owned)) => {
                        match callback::reject(&endpoint, error, (value, owned)).await {}
                    }
                }
            }
            ExecutionStep::ExpandHeads(request) => {
                let owned = OwnedRequest::new(request, operation);
                let request = match owned.get() {
                    Ok(request) => request,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                let work = request.work();
                let quote = if request.traversal.complete() {
                    request
                        .traversal
                        .finish_work()
                        .map(|units| (units, request.traversal.finish_bytes()))
                } else {
                    work.as_ref().map(|work| (work.units, Some(work.bytes)))
                };
                let Some((units, Some(bytes))) = quote else {
                    match callback::reject(
                        &endpoint,
                        RunError::Contract("cycle head traversal work overflow"),
                        owned,
                    )
                    .await {}
                };
                callback::complete(&endpoint, &owned, callback::CallbackKind::Canonical, || {
                    ready(
                        endpoint
                            .admit(ExecutionWork::Resource {
                                requested_bytes: bytes,
                            })
                            .and_then(|()| {
                                attempt_probe::charge(endpoint.context.db, units)
                                    .map_err(RunError::Refused)
                            })
                            .and_then(|()| endpoint.admit(ExecutionWork::Work { units })),
                    )
                })
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.advance(work),
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::CompareCycle(request) => {
                let owned = OwnedRequest::new(request, operation);
                let request = match owned.get() {
                    Ok(request) => request,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                if let Some((left, right)) = request.comparison_values() {
                    native_values::admit(
                        &endpoint,
                        &owned,
                        provider,
                        db,
                        NativeValueOperation::Comparison { left, right },
                    )
                    .await;
                }
                let value_converged = callback::complete(
                    &endpoint,
                    &owned,
                    callback::CallbackKind::NativeCall,
                    || ready(Ok(request.compare())),
                )
                .await;
                match owned.take_admitted() {
                    Ok(request) => request.resume(value_converged),
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::Prepare(request) => {
                let caller = match Caller::capture(operation) {
                    Ok(caller) => caller,
                    Err(error) => match callback::reject(&endpoint, error, request).await {},
                };
                let mut owned = OwnedFrameFree::new(request, operation, caller);
                let (request, scope) = match owned.split() {
                    Ok(parts) => parts,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                let stage = callback::complete(
                    &endpoint,
                    &scope,
                    callback::CallbackKind::Canonical,
                    || {
                        let Some(units) = request.output_check_work() else {
                            return ready(Err(RunError::Contract("completion metadata work overflow")));
                        };
                        if units != 0
                            && let Err(error) = endpoint.admit_work(units)
                        {
                            return ready(Err(error));
                        }
                        if !operation.policy.allows_incomplete() || !request.controlled_outputs_are_empty() {
                            return ready(Err(RunError::Contract(
                                "return-only completion retained tracked outputs",
                            )));
                        }
                        ready(Ok(request.prepare_before_comparison()))
                    },
                )
                .await;
                if let Some((left, right)) = request.comparison_values(&stage) {
                    native_values::admit(
                        &endpoint,
                        &scope,
                        provider,
                        db,
                        NativeValueOperation::Comparison { left, right },
                    )
                    .await;
                }
                callback::complete(
                    &endpoint,
                    &scope,
                    callback::CallbackKind::NativeCall,
                    || {
                        request.prepare_after_comparison(stage);
                        let result = request
                            .storage_bytes()
                            .ok_or(RunError::Contract("memo preparation size overflow"))
                            .and_then(|requested_bytes| {
                                endpoint.admit(ExecutionWork::Resource { requested_bytes })
                            });
                        ready(result)
                    },
                )
                .await;
                match owned.take_admitted() {
                    Ok(request) => match request.into_commit() {
                        Ok(commit) => ExecutionStep::Commit(commit),
                        Err((error, request)) => {
                            let owned = OwnedFrameFree::new(request, operation, caller);
                            match callback::reject(&endpoint, RunError::Contract(error), owned)
                                .await {}
                        }
                    },
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::Commit(request) => {
                let caller = match Caller::capture(operation) {
                    Ok(caller) => caller,
                    Err(error) => match callback::reject(&endpoint, error, request).await {},
                };
                let owned = OwnedFrameFree::new(request, operation, caller);
                let request = match owned.admitted() {
                    Ok(request) => request,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                callback::complete(&endpoint, &owned, callback::CallbackKind::Canonical, || {
                    ready(
                        request
                            .publication_work()
                            .ok_or(RunError::Contract("memo publication work overflow"))
                            .and_then(|units| endpoint.admit_work(units)),
                    )
                })
                .await;
                if !request.is_current() {
                    match callback::reject(
                        &endpoint,
                        RunError::Contract("prepared publication changed its selected memos"),
                        owned,
                    )
                    .await {}
                }
                explicit_reads::check_acceptance();
                match owned.take_admitted() {
                    Ok(request) => {
                        let retirement = request.retirement();
                        request.publish(Some(retirement))
                    }
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::DidFinalize(request) => {
                let caller = match Caller::capture(operation) {
                    Ok(caller) => caller,
                    Err(error) => match callback::reject(&endpoint, error, request).await {},
                };
                let owned = OwnedFrameFree::new(request, operation, caller);
                let request = match owned.admitted() {
                    Ok(request) => request,
                    Err(error) => match callback::reject(&endpoint, error, owned).await {},
                };
                callback::complete(&endpoint, &owned, callback::CallbackKind::Canonical, || {
                    request.event();
                    ready(Ok(()))
                })
                .await;
                match owned.take_admitted() {
                    Ok(request) => ExecutionStep::Complete(request.memo),
                    Err((error, owned)) => match callback::reject(&endpoint, error, owned).await {},
                }
            }
            ExecutionStep::Participant(request) => {
                match retire_participant_owned(&endpoint, operation, request).await {
                    super::participant::ParticipantProgress::Pending(request) => {
                        ExecutionStep::Participant(request)
                    }
                    super::participant::ParticipantProgress::Complete(memo) => {
                        ExecutionStep::Complete(memo)
                    }
                    super::participant::ParticipantProgress::Execute(request) => {
                        ExecutionStep::Start(request)
                    }
                }
            }
            ExecutionStep::Complete(memo) => {
                let scope = CallbackScope::new(operation, execution_caller);
                let memo = callback::complete(
                    &endpoint,
                    &scope,
                    callback::CallbackKind::Canonical,
                    || ready(endpoint.context.observe(Ok(memo))),
                )
                .await;
                return Ok(memo);
            }
        };
    }
}

struct PendingTask<'run> {
    task: Task<'run>,
    wanted: Rc<Cell<bool>>,
    resume_checkpoint: Option<PollGeneration>,
}

struct Queue<'run> {
    pending: RefCell<Vec<PendingTask<'run>>>,
    cancelled: Cell<bool>,
    active_poll: RefCell<Option<ActivePoll>>,
    last_poll_generation: Cell<usize>,
    driver: native_source::DriverIdentity,
    paused: Cell<bool>,
    pause_epoch: Cell<usize>,
}

impl<'run> Queue<'run> {
    fn check_runnable(&self) -> RunResult<()> {
        if self.cancelled.get() {
            return Err(RunError::Contract("execution driver has ended"));
        }
        if self.paused.get() {
            return Err(RunError::Contract(
                "execution queue is paused in a native source call",
            ));
        }
        if native_source::current_driver() != Some(self.driver) {
            return Err(RunError::Contract(
                "execution queue belongs to another driver",
            ));
        }
        Ok(())
    }

    fn begin_access(&self) -> RunResult<QueueAccess<'_, 'run>> {
        self.check_runnable()?;
        Ok(QueueAccess {
            queue: self,
            epoch: self.pause_epoch.get(),
        })
    }
}

/// Admissions may call native code. A completed pause still invalidates that admission's
/// permission to allocate or enqueue work, even though the parent is runnable again.
struct QueueAccess<'queue, 'run> {
    queue: &'queue Queue<'run>,
    epoch: usize,
}

impl QueueAccess<'_, '_> {
    fn check(&self) -> RunResult<()> {
        self.queue.check_runnable()?;
        if self.queue.pause_epoch.get() != self.epoch {
            return Err(RunError::Contract(
                "execution queue paused during admission",
            ));
        }
        Ok(())
    }
}

struct Endpoint<'run, 'db: 'run> {
    queue: Rc<Queue<'run>>,
    context: Rc<RunContext<'db>>,
    admission: admission::Admission<'run>,
}

impl Clone for Endpoint<'_, '_> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            context: self.context.clone(),
            admission: self.admission,
        }
    }
}

struct Reply<T> {
    value: Rc<RefCell<Option<RunResult<T>>>>,
    wanted: Rc<Cell<bool>>,
}

impl<T> Future for Reply<T> {
    type Output = RunResult<T>;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.value.borrow_mut().take() {
            Some(value) => Poll::Ready(value),
            None => Poll::Pending,
        }
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        self.wanted.set(false);
    }
}

impl<'run, 'db: 'run> Endpoint<'run, 'db> {
    fn admit_work(&self, units: usize) -> RunResult<()> {
        if !self.local_call_is_eligible() {
            return Err(RunError::Contract(
                "semantic work requires the current interruptible execution run",
            ));
        }
        let access = self.queue.begin_access()?;
        attempt_probe::charge(self.context.db, units).map_err(RunError::Refused)?;
        self.admit(ExecutionWork::Work { units })?;
        access.check()
    }

    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let access = self.queue.begin_access()?;
        self.context.check_resume()?;
        access.check()?;
        let result = self.admission.admit(&self.context, work);
        if result.is_ok() {
            access.check()?;
        }
        self.context.observe(result)?;
        access.check()
    }

    fn demand<T: 'run, F: Future<Output = RunResult<T>> + 'run, M: FnOnce() -> F + 'run>(
        &self,
        make: M,
    ) -> RunResult<Reply<T>> {
        let access = self.queue.begin_access()?;
        self.context.check_resume()?;
        access.check()?;
        // Include both vectors' spare capacity and the reference-counted reply allocations.
        // The concrete task owns its factory, future and unpublished result in one allocation.
        let requested_bytes = size_of::<TypedTask<'db, M, F, T>>()
            .saturating_add(size_of::<RefCell<Option<RunResult<T>>>>())
            .saturating_add(size_of::<Cell<bool>>())
            .saturating_add(8 * size_of::<PendingTask<'run>>())
            .saturating_add(4 * size_of::<usize>())
            .saturating_add(align_of::<RefCell<Option<RunResult<T>>>>().max(align_of::<usize>()))
            .saturating_add(align_of::<usize>());
        #[cfg(test)]
        tests::observation::record_task_layout::<M, F, T>(&self.context, requested_bytes);
        self.admit(ExecutionWork::Task { requested_bytes })?;
        access.check()?;
        let value = Rc::new(RefCell::new(None));
        let wanted = Rc::new(Cell::new(true));
        let destination = value.clone();
        let context = self.context.clone();
        let task = Box::pin(TypedTask::<'db, M, F, T>::new(make, destination, context));
        #[cfg(test)]
        let task = tests::observation::observe_task(task);
        access.check()?;
        self.queue.pending.borrow_mut().push(PendingTask {
            task,
            wanted: wanted.clone(),
            resume_checkpoint: None,
        });
        Ok(Reply { value, wanted })
    }

    #[cfg(test)]
    fn execute<C, P>(
        &self,
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        id: Id,
        old_memo: Option<&'db Memo<C>>,
        provider: P,
    ) -> RunResult<Reply<P::Output>>
    where
        C: Configuration,
        P: Provider<'run, 'db, C> + 'run,
    {
        let endpoint = self.clone();
        self.demand(move || execute(endpoint, ingredient, db, id, old_memo, provider))
    }
}

// Compatibility observation for existing tests; this is never nesting authority.
#[cfg(test)]
thread_local! { static RUN_ACTIVE: Cell<bool> = const { Cell::new(false) }; }

struct Driver<'run, 'db: 'run> {
    endpoint: Endpoint<'run, 'db>,
    stack: Vec<PendingTask<'run>>,
    lease: Option<native_source::DriverLease>,
}

enum TaskProgress {
    Complete,
    Discard,
    Child,
    Checkpoint(PollGeneration),
    Terminal(RunError),
}

enum DriveFailure {
    Ordinary(RunError),
    Terminal(RunError),
}

impl From<RunError> for DriveFailure {
    fn from(error: RunError) -> Self {
        Self::Ordinary(error)
    }
}

fn check_completed_poll(progress: &ActivePoll, queue: &Queue<'_>) -> RunResult<()> {
    queue.check_runnable()?;
    if progress.has_unconsumed_resume() {
        return Err(RunError::Contract("task ignored its checkpoint receipt"));
    }
    if progress.requested_checkpoint().is_some() {
        return Err(RunError::Contract("completed task retained a checkpoint"));
    }
    if !queue.pending.borrow().is_empty() {
        return Err(RunError::Contract("completed task retained a child"));
    }
    Ok(())
}

fn check_active_completion(queue: &Queue<'_>) -> RunResult<()> {
    let active = queue.active_poll.borrow();
    let progress = active
        .as_ref()
        .ok_or(RunError::Contract("execution poll lost its owner"))?;
    check_completed_poll(progress, queue)
}

impl<'run, 'db: 'run> Driver<'run, 'db> {
    #[cfg(test)]
    fn run<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        db: &'db dyn Database,
        make: impl FnOnce(Endpoint<'run, 'db>) -> F + 'run,
    ) -> RunResult<T> {
        Self::run_with_admission(db, &Unrestricted, make)
    }

    #[cfg(test)]
    fn run_with_admission<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        db: &'db dyn Database,
        admission: &'run dyn ExecutionAdmission,
        make: impl FnOnce(Endpoint<'run, 'db>) -> F + 'run,
    ) -> RunResult<T> {
        let admission = admission::Admission::legacy(db, admission)?;
        let context = RunContext::new(db)?;
        context.observe(admission.admit(
            &context,
            ExecutionWork::Resource {
                requested_bytes: size_of::<RunContext<'db>>().saturating_add(2 * size_of::<usize>()),
            },
        ))?;
        Self::run_with_context(Rc::new(context), admission, make)
    }

    fn run_with_context<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        context: Rc<RunContext<'db>>,
        admission: admission::Admission<'run>,
        make: impl FnOnce(Endpoint<'run, 'db>) -> F + 'run,
    ) -> RunResult<T> {
        context.check_start()?;
        let lease = native_source::DriverLease::enter(&context)?;
        let admitted = admission.admit(
            &context,
            ExecutionWork::Resource {
                requested_bytes: size_of::<Queue<'run>>().saturating_add(2 * size_of::<usize>()),
            },
        );
        if admitted.is_ok() {
            lease.check_context(&context)?;
        }
        context.observe(admitted)?;
        lease.check_context(&context)?;
        let endpoint = Endpoint {
            queue: Rc::new(Queue {
                pending: RefCell::new(Vec::new()),
                cancelled: Cell::new(false),
                active_poll: RefCell::new(None),
                last_poll_generation: Cell::new(0),
                driver: lease.identity(),
                paused: Cell::new(false),
                pause_epoch: Cell::new(0),
            }),
            context,
            admission,
        };
        let mut driver = Self {
            endpoint,
            stack: Vec::new(),
            lease: Some(lease),
        };
        // Retain query owners while cancellation unwinds the active callout. Aborting them
        // before rethrowing avoids panic-poisoned provisional memos on same-revision retry.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            match driver.drive(make) {
            Ok(value) => driver.endpoint.context.observe(Ok(value)),
            Err(DriveFailure::Ordinary(error)) => driver.endpoint.context.observe(Err(error)),
            // The terminal poll already marked the first incomplete reason and retained
            // the exact callout error; it needs no second observation after cleanup.
            Err(DriveFailure::Terminal(error)) => Err(error),
        }
        }));
        match outcome {
            Ok(result) => result,
            Err(payload) => {
                if matches!(
                    payload.downcast_ref::<Cancelled>(),
                    Some(Cancelled::Local | Cancelled::PendingWrite)
                ) {
                    driver.endpoint.context.refuse(Incomplete::Interrupted);
                    drop(driver);
                }
                resume_unwind(payload)
            }
        }
    }

    fn drive<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        &mut self,
        make: impl FnOnce(Endpoint<'run, 'db>) -> F + 'run,
    ) -> Result<T, DriveFailure> {
        let endpoint = self.endpoint.clone();
        let mut root = self.endpoint.demand(move || make(endpoint))?;
        self.push_child()?;
        let mut cx = Context::from_waker(Waker::noop());
        while !self.stack.is_empty() {
            self.endpoint.admit(ExecutionWork::Poll)?;
            let progress = match self.poll_task(&mut cx) {
                Ok(progress) => progress,
                Err(error) => return Err(self.reject_current(error).into()),
            };
            match progress {
                TaskProgress::Complete => {
                    self.stack.pop();
                }
                TaskProgress::Discard => {
                    // An abandoned future can still own values whose destructors request work.
                    // Discard it before the final callback and queue checks, not afterward.
                    drop(self.stack.pop());
                    self.endpoint.context.check_resume()?;
                    if !self.endpoint.queue.pending.borrow().is_empty() {
                        return Err(self
                            .reject_current(RunError::Contract("discarded task retained a child"))
                            .into());
                    }
                }
                TaskProgress::Checkpoint(checkpoint) => {
                    self.endpoint.queue.check_runnable()?;
                    if let Some(task) = self.stack.last_mut() {
                        task.resume_checkpoint = Some(checkpoint);
                    }
                }
                TaskProgress::Child => {
                    if let Err(error) = self.push_child() {
                        return Err(self.reject_current(error).into());
                    }
                }
                TaskProgress::Terminal(error) => {
                    return Err(DriveFailure::Terminal(self.reject_reported(error)));
                }
            }
        }
        match Pin::new(&mut root).poll(&mut cx) {
            Poll::Ready(value) => value.map_err(DriveFailure::from),
            Poll::Pending => Err(RunError::Contract("root result missing").into()),
        }
    }

    fn poll_task(&mut self, cx: &mut Context<'_>) -> RunResult<TaskProgress> {
        let Some(task) = self.stack.last_mut() else {
            return Err(RunError::Contract("execution task is missing"));
        };
        if !task.wanted.get() {
            if task.resume_checkpoint.is_some() {
                return Err(RunError::Contract("checkpoint task was abandoned"));
            }
            if !self.endpoint.queue.pending.borrow().is_empty() {
                return Err(RunError::Contract("discarded task retained a child"));
            }
            return Ok(TaskProgress::Discard);
        }

        let poll = PollGuard::enter(&self.endpoint, task)?;
        let polled = task.task.as_mut().poll(cx);
        if let Some(terminal) = poll.take_terminal()? {
            return Self::terminal_progress(terminal);
        }
        explicit_reads::check_acceptance();
        match polled {
            // Preserve an existing error before classifying a malformed successful poll.
            Poll::Ready(Err(error)) => Err(error),
            Poll::Ready(Ok(())) => {
                let prepared: RunResult<()> = (|| {
                    self.endpoint.context.check_resume()?;
                    // A malformed Ready may already have a child. Its future must remain on
                    // the stack until that child has been destroyed by ordered cleanup.
                    check_active_completion(&self.endpoint.queue)?;
                    task.task.as_mut().retire_future()?;
                    self.endpoint.context.check_resume()?;
                    if !task.wanted.get() {
                        // Retirement or its cancellation callback may both abandon the result and
                        // queue a child. That child must be dropped before the unwanted output.
                        check_active_completion(&self.endpoint.queue)?;
                        task.task.as_mut().discard_returned()?;
                        self.endpoint.context.check_resume()?;
                    }
                    Ok(())
                })();
                // Future/output destruction and cancellation callbacks can request work. All
                // callback-capable operations precede this final inspection and publication.
                let mut progress = poll.finish()?;
                if let Some(terminal) = progress.take_terminal() {
                    return Self::terminal_progress(terminal);
                }
                prepared?;
                check_completed_poll(&progress, &self.endpoint.queue)?;
                if task.wanted.get() {
                    task.task.as_mut().complete(Ok(()))?;
                }
                Ok(TaskProgress::Complete)
            }
            Poll::Pending => {
                let progress = poll.finish()?;
                if progress.has_unconsumed_resume() {
                    return Err(RunError::Contract("task ignored its checkpoint receipt"));
                }
                if let Some(checkpoint) = progress.requested_checkpoint() {
                    if !self.endpoint.queue.pending.borrow().is_empty() {
                        return Err(RunError::Contract(
                            "task requested a child and a checkpoint",
                        ));
                    }
                    Ok(TaskProgress::Checkpoint(checkpoint))
                } else {
                    Ok(TaskProgress::Child)
                }
            }
        }
    }

    fn terminal_progress(terminal: TerminalDisposition) -> RunResult<TaskProgress> {
        match terminal {
            TerminalDisposition::Error(error) => {
                explicit_reads::check_acceptance();
                Ok(TaskProgress::Terminal(error))
            }
            TerminalDisposition::Panic(payload) => resume_unwind(payload),
        }
    }

    fn reject_current(&mut self, error: RunError) -> RunError {
        let error = self
            .endpoint
            .context
            .observe::<()>(Err(error))
            .err()
            .unwrap_or(error);
        self.reject_reported(error)
    }

    fn reject_reported(&mut self, error: RunError) -> RunError {
        if let Some(task) = self.stack.last_mut() {
            // Failure delivery copies only the error. Unpublished values and futures remain
            // owned by the task until Driver::drop has destroyed its queued descendants.
            let _ = task.task.as_mut().complete(Err(error));
        }
        error
    }

    fn push_child(&mut self) -> RunResult<()> {
        self.endpoint.queue.check_runnable()?;
        let mut pending = self.endpoint.queue.pending.borrow_mut();
        if pending.len() != 1 {
            return Err(RunError::Contract(
                "a pending task must demand exactly one child",
            ));
        }
        if let Some(child) = pending.pop() {
            self.stack.push(child);
        }
        Ok(())
    }
}

impl Drop for Driver<'_, '_> {
    fn drop(&mut self) {
        let _drain = explicit_reads::drain_guard();
        self.endpoint.queue.cancelled.set(true);
        self.endpoint.queue.active_poll.borrow_mut().take();
        let finished = FinishRun(&self.endpoint.context, self.lease.take());
        let active = DrainTasks(&mut self.stack);
        let mut pending = std::mem::take(&mut *self.endpoint.queue.pending.borrow_mut());
        drain_tasks(&mut pending);
        drop(active);
        drop(finished);
    }
}

struct FinishRun<'a, 'db>(&'a RunContext<'db>, Option<native_source::DriverLease>);

impl Drop for FinishRun<'_, '_> {
    fn drop(&mut self) {
        self.0.assert_finished();
        drop(self.1.take());
    }
}

struct DrainTasks<'a, 'run>(&'a mut Vec<PendingTask<'run>>);

impl Drop for DrainTasks<'_, '_> {
    fn drop(&mut self) {
        drain_tasks(self.0);
    }
}

fn drain_tasks(tasks: &mut Vec<PendingTask<'_>>) {
    while let Some(task) = tasks.pop() {
        // If one destructor panics, its remaining ancestors still have to drop deepest-first.
        let remaining = DrainTasks(tasks);
        drop(task);
        std::mem::forget(remaining);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(in crate::function) use tests::validation_trace::{
    TraceEvent, record as record_validation_trace,
};
