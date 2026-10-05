//! Experimental normal incomplete returns for queries without tracked outputs.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::rc::Rc;

use crate::prepared_source_probe::Stamp;
use crate::sync::Arc;
use crate::sync::atomic::{AtomicU8, Ordering};
use crate::zalsa::Zalsa;
use crate::zalsa_local::ZalsaLocal;
use crate::{Database, Revision};

#[cfg(test)]
pub(crate) mod charge_observation;

#[cfg(all(test, not(feature = "shuttle")))]
pub(crate) mod registration_test_support;

#[cfg(all(test, not(feature = "shuttle")))]
pub(crate) mod paired_test_support;

#[cfg(all(test, not(feature = "shuttle")))]
pub(crate) mod transfer_test_support;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryPolicy {
    Unclassified,
    ReturnOnly,
    CompleteOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationExecution {
    Ordinary,
    AdmittedOutputOnly,
}

/// The declaration restricts dependencies; execution permission belongs to this operation only.
/// Admitted execution may refuse and carry attempt support, but cannot create tracked outputs or
/// direct accumulated values. Ordinary execution retains the declaration's completion guarantees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperationPolicy {
    pub(crate) declared: QueryPolicy,
    execution: OperationExecution,
}

impl OperationPolicy {
    pub(crate) const fn ordinary(declared: QueryPolicy) -> Self {
        Self {
            declared,
            execution: OperationExecution::Ordinary,
        }
    }

    pub(crate) const fn admitted(declared: QueryPolicy) -> Self {
        Self {
            declared,
            execution: OperationExecution::AdmittedOutputOnly,
        }
    }

    pub(crate) fn allows_incomplete(self) -> bool {
        self.declared == QueryPolicy::ReturnOnly
            || self.execution == OperationExecution::AdmittedOutputOnly
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    ActiveQuery,
    ActiveOperation,
    NestedAttempt,
    /// Retained for compatibility. Independent evaluation entry does not return this variant.
    ConcurrentAttempt,
    /// The attempt cannot give another operation scope a unique identity; its body was not run.
    ScopeIdentityExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Incomplete {
    Allowance,
    RequestedAllocation,
    Interrupted,
}

/// Cumulative limits shared by every execution registry in one evaluation.
///
/// Requested bytes count conservative storage quotations, not live memory or allocator backing.
/// Accepted charges are not refunded when their storage is released.
/// Providers may account for initialization and copying of fixed value representations with
/// requested bytes, including inline values charged separately on each construction. Metered work
/// then depends on both counters, not semantic work alone; variable computation and dynamic cleanup
/// still require their own work admissions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    pub semantic_work: usize,
    pub requested_bytes: usize,
}

/// Work and requested bytes admitted during one completed or interrupted evaluation.
///
/// Rejected charges are excluded. Accepted charges remain counted after their storage is released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionUsage {
    pub semantic_work: usize,
    pub requested_bytes: usize,
}

/// The outcome and final admitted consumption of an evaluation that returned normally.
#[derive(Debug, Eq, PartialEq)]
pub struct ExecutionReceipt<T> {
    pub outcome: AttemptOutcome<T>,
    pub usage: ExecutionUsage,
}

/// Authority to use the runtime-owned limits of one evaluation on this worker.
///
/// Obtain this capability through [`try_with_execution_budget`] or
/// [`try_with_metered_execution_budget`]. It cannot be constructed, cloned, returned from the root,
/// or moved to another worker.
///
/// ```compile_fail
/// use salsa::execution_probe::ExecutionBudget;
/// let budget = ExecutionBudget {};
/// ```
///
/// ```compile_fail
/// use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};
/// let db = salsa::DatabaseImpl::default();
/// try_with_execution_budget(&db, ExecutionLimits { semantic_work: 1, requested_bytes: 1024 },
///     |budget| { let _copy = budget.clone(); });
/// ```
///
/// ```compile_fail
/// use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};
/// let db = salsa::DatabaseImpl::default();
/// try_with_execution_budget(&db, ExecutionLimits { semantic_work: 1, requested_bytes: 1024 },
///     |budget| std::thread::scope(|scope| { scope.spawn(move || drop(budget)); }));
/// ```
///
/// ```compile_fail
/// use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};
/// let db = salsa::DatabaseImpl::default();
/// try_with_execution_budget(&db, ExecutionLimits { semantic_work: 1, requested_bytes: 1024 },
///     |budget| std::thread::scope(|scope| {
///         let shared = &budget;
///         scope.spawn(move || { let _ = &shared; });
///     }));
/// ```
pub struct ExecutionBudget<'session> {
    support: AttemptSupport,
    // Invariance prevents widening the root's lifetime; Rc keeps authority on its worker.
    marker: PhantomData<(&'session mut &'session (), Rc<()>)>,
}

impl ExecutionBudget<'_> {
    pub(crate) fn is_current(&self, db: &dyn Database) -> bool {
        self.support.admission_is_budget(db.zalsa()) == Some(true)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum AttemptOutcome<T> {
    Complete(T),
    Incomplete(Incomplete),
}

#[derive(Debug)]
struct AttemptToken {
    database: usize,
    revision: Revision,
    cancellation: u8,
    state: AtomicU8,
}

const RUNNING: u8 = 0;
const INCOMPLETE: u8 = 1;
const FINISHED: u8 = 2;
const ABANDONED: u8 = 3;
const INTERRUPTED: u8 = 4;
const REQUESTED_ALLOCATION: u8 = 5;

impl AttemptToken {
    fn reason(&self) -> Option<Incomplete> {
        match self.state.load(Ordering::Acquire) {
            INCOMPLETE => Some(Incomplete::Allowance),
            REQUESTED_ALLOCATION => Some(Incomplete::RequestedAllocation),
            INTERRUPTED => Some(Incomplete::Interrupted),
            _ => None,
        }
    }

    fn report_incomplete(&self, reason: Incomplete) -> Incomplete {
        if let Some(previous) = self.reason() {
            return previous;
        }
        assert_eq!(self.state.load(Ordering::Acquire), RUNNING);
        self.state.store(
            match reason {
                Incomplete::Allowance => INCOMPLETE,
                Incomplete::RequestedAllocation => REQUESTED_ALLOCATION,
                Incomplete::Interrupted => INTERRUPTED,
            },
            Ordering::Release,
        );
        reason
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AttemptSupport {
    owner: Arc<AttemptToken>,
    // Provisional ownership becomes irrelevant after finalization; this marker never does.
    explicitly_incomplete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MemoReuse {
    Ordinary,
    Incomplete,
    Stale,
}

impl AttemptSupport {
    pub(crate) fn reason(&self) -> Option<Incomplete> {
        self.owner.reason()
    }

    pub(crate) fn incomplete(&self, provisional: bool) -> bool {
        self.explicitly_incomplete
            || provisional
                && matches!(
                    self.owner.state.load(Ordering::Acquire),
                    INCOMPLETE | REQUESTED_ALLOCATION | INTERRUPTED | ABANDONED
                )
    }

    pub(crate) fn make_incomplete(&mut self) {
        self.explicitly_incomplete = true;
        assert!(
            self.owner.reason().is_some(),
            "incomplete support has no refusal reason"
        );
    }

    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
    }

    /// Checks worker, database, stamp and root identity without invoking application code.
    pub(crate) fn admission_is_budget(&self, zalsa: &Zalsa) -> Option<bool> {
        if !self.is_current(zalsa) {
            return None;
        }
        LOCAL.with_borrow(|local| {
            let session = local.session.as_ref()?;
            self.same_owner(&session.support)
                .then_some(matches!(session.admission, AdmissionMode::Budget { .. }))
        })
    }

    pub(crate) fn is_running_current(&self, zalsa: &Zalsa) -> bool {
        self.is_current(zalsa)
            && self.owner.state.load(Ordering::Acquire) == RUNNING
            && !self.explicitly_incomplete
    }

    pub(crate) fn is_current(&self, zalsa: &Zalsa) -> bool {
        self.owner.revision == zalsa.current_revision()
            && self.owner.cancellation == zalsa.runtime().cancellation_count()
            && self.owns_current_session(zalsa)
    }

    /// Cleanup still belongs to this session after revision cancellation changes its stamp.
    pub(crate) fn owns_current_session(&self, zalsa: &Zalsa) -> bool {
        self.owner.database == std::ptr::from_ref(zalsa).addr()
            && LOCAL.with_borrow(|local| {
                local
                    .session
                    .as_ref()
                    .is_some_and(|session| self.same_owner(&session.support))
            })
    }

    /// Scope ownership survives revision cancellation, but never a different attempt or worker.
    pub(crate) fn local_ownership(&self, zalsa: &Zalsa) -> Option<LocalOwnershipReceipt> {
        if self.owner.database != std::ptr::from_ref(zalsa).addr() {
            return None;
        }
        LOCAL.with_borrow(|local| {
            let session = local.session.as_ref()?;
            self.same_owner(&session.support)
                .then_some(LocalOwnershipReceipt {
                    scope: session.current_scope,
                })
        })
    }

    pub(crate) fn reuse(&self, zalsa: &Zalsa, provisional: bool) -> MemoReuse {
        if self.incomplete(provisional) {
            if self.is_current(zalsa) {
                MemoReuse::Incomplete
            } else {
                MemoReuse::Stale
            }
        } else if provisional
            && !self.is_current(zalsa)
            && self.owner.state.load(Ordering::Acquire) != FINISHED
        {
            MemoReuse::Stale
        } else {
            MemoReuse::Ordinary
        }
    }
}

struct Session {
    support: AttemptSupport,
    local: usize,
    remaining: usize,
    admission: AdmissionMode,
    current_scope: Option<NonZeroUsize>,
    next_scope: Option<NonZeroUsize>,
}

enum AdmissionMode {
    LegacyCallback,
    Budget { remaining_requested_bytes: usize },
}

/// A scope identity is meaningful only when checked through its original attempt support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocalOwnershipReceipt {
    scope: Option<NonZeroUsize>,
}

#[derive(Clone, Copy)]
struct ScopeOwnership {
    id: NonZeroUsize,
    parent: Option<NonZeroUsize>,
}

#[derive(Default)]
struct Local {
    session: Option<Session>,
    structural: Option<StructuralReservation>,
    operations: Vec<(usize, OperationPolicy)>,
    cycle_support_scope: bool,
    scopes: Vec<usize>,
    incomplete_observation: Option<bool>,
}

impl Local {
    #[cfg(test)]
    fn has_registration(&self, database: usize) -> bool {
        self.scopes.contains(&database)
            || self
                .operations
                .iter()
                .any(|(active, _)| *active == database)
    }
}

thread_local! {
    static LOCAL: RefCell<Local> = RefCell::new(Local::default());
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct StructuralIdentity {
    database: usize,
    local: usize,
    root: usize,
}

struct StructuralReservation {
    identity: StructuralIdentity,
    violation: Option<&'static str>,
}

impl StructuralReservation {
    fn check(
        &mut self,
        database: usize,
        local: Option<usize>,
        policy: Option<QueryPolicy>,
    ) -> Result<(), &'static str> {
        let error = if database != self.identity.database
            || local.is_some_and(|local| local != self.identity.local)
        {
            Some("structural preparation changed database")
        } else if policy.is_some_and(|policy| policy != QueryPolicy::CompleteOnly) {
            Some("structural preparation requested a non-structural query")
        } else {
            None
        };
        if let Some(error) = self.violation.or(error) {
            self.violation = Some(error);
            Err(error)
        } else {
            Ok(())
        }
    }
}

/// Retains one semantic attempt while its executor owns structural preparation.
/// The executor separately parks its query stack, poll and explicit-read boundary.
pub(crate) struct StructuralPreparation {
    saved: Local,
    identity: StructuralIdentity,
    marker: PhantomData<Rc<()>>,
}

impl StructuralPreparation {
    pub(crate) fn park(db: &dyn Database, expected: &AttemptSupport) -> Result<Self, &'static str> {
        let identity = StructuralIdentity {
            database: std::ptr::from_ref(db.zalsa()).addr(),
            local: std::ptr::from_ref(db.zalsa_local()).addr(),
            root: Arc::as_ptr(&expected.owner).addr(),
        };
        LOCAL.with(|local| {
            let mut local = local
                .try_borrow_mut()
                .map_err(|_| "attempt state is borrowed during structural preparation")?;
            if local.structural.is_some() {
                return Err("nested structural preparation");
            }
            let Some(session) = &local.session else {
                return Err("structural preparation has no semantic attempt");
            };
            if !session.support.same_owner(expected)
                || session.local != identity.local
                || expected.owner.database != identity.database
                || expected.owner.revision != db.zalsa().current_revision()
                || expected.owner.cancellation != db.zalsa().runtime().cancellation_count()
                || expected.owner.state.load(Ordering::Acquire) != RUNNING
            {
                return Err("structural preparation has a foreign or stopped attempt");
            }
            if local.operations.last().is_some_and(|(database, policy)| {
                *database != identity.database || !policy.allows_incomplete()
            }) {
                return Err("structural preparation has a non-semantic operation");
            }
            let replacement = Local {
                structural: Some(StructuralReservation {
                    identity,
                    violation: None,
                }),
                ..Local::default()
            };
            Ok(Self {
                saved: std::mem::replace(&mut *local, replacement),
                identity,
                marker: PhantomData,
            })
        })
    }

    /// Check before publishing structural results; dropping the guard restores semantic ownership.
    pub(crate) fn check_complete(&self) -> Result<(), &'static str> {
        LOCAL.with(|local| {
            let local = local
                .try_borrow()
                .map_err(|_| "attempt state is borrowed after structural preparation")?;
            let Some(reservation) = &local.structural else {
                return Err("structural preparation lost its reservation");
            };
            if reservation.identity != self.identity
                || local.session.is_some()
                || !local.operations.is_empty()
                || !local.scopes.is_empty()
                || local.incomplete_observation.is_some()
            {
                return Err("structural preparation retained an operation or changed owner");
            }
            if let Some(error) = reservation.violation {
                return Err(error);
            }
            Ok(())
        })
    }
}

impl Drop for StructuralPreparation {
    fn drop(&mut self) {
        LOCAL.with_borrow_mut(|local| {
            assert!(
                local
                    .structural
                    .as_ref()
                    .is_some_and(|reservation| reservation.identity == self.identity)
                    && local.session.is_none()
                    && local.operations.is_empty()
                    && !local.cycle_support_scope
                    && local.scopes.is_empty()
                    && local.incomplete_observation.is_none(),
                "structural preparation did not retire its operations before restoration"
            );
            *local = std::mem::take(&mut self.saved);
        });
    }
}

pub(crate) fn check_structural_access(db: &dyn Database) -> Result<(), &'static str> {
    check_structural_storage(db.zalsa(), Some(db.zalsa_local()))
}

pub(crate) fn check_structural_storage(
    zalsa: &Zalsa,
    zalsa_local: Option<&ZalsaLocal>,
) -> Result<(), &'static str> {
    LOCAL.with(|local| {
        let mut local = local
            .try_borrow_mut()
            .map_err(|_| "attempt state is borrowed during structural access")?;
        match &mut local.structural {
            Some(reservation) => reservation.check(
                std::ptr::from_ref(zalsa).addr(),
                zalsa_local.map(|local| std::ptr::from_ref(local).addr()),
                None,
            ),
            None => Ok(()),
        }
    })
}

pub(crate) fn check_structural_mutation() -> Result<(), &'static str> {
    LOCAL.with(|local| {
        let mut local = local
            .try_borrow_mut()
            .map_err(|_| "attempt state is borrowed during structural mutation")?;
        match &mut local.structural {
            Some(reservation) => Err(*reservation
                .violation
                .get_or_insert("structural preparation attempted database mutation")),
            None => Ok(()),
        }
    })
}

/// Rejects semantic query entry before generated key or ingredient discovery during preparation.
#[doc(hidden)]
pub fn assert_structural_query(db: &dyn Database, policy: QueryPolicy) {
    LOCAL.with_borrow_mut(|local| {
        if let Some(reservation) = &mut local.structural
            && let Err(error) = reservation.check(
                std::ptr::from_ref(db.zalsa()).addr(),
                Some(std::ptr::from_ref(db.zalsa_local()).addr()),
                Some(policy),
            )
        {
            panic!("{error}");
        }
    });
}

struct OperationScope<'a> {
    depth: usize,
    zalsa: &'a Zalsa,
    #[cfg(test)]
    counted: bool,
    attempt_scope: Option<ScopeOwnership>,
}

impl Drop for OperationScope<'_> {
    fn drop(&mut self) {
        LOCAL.with_borrow_mut(|local| {
            debug_assert_eq!(local.scopes.len(), self.depth + 1);
            let database = local.scopes.pop();
            debug_assert_eq!(database, Some(std::ptr::from_ref(self.zalsa).addr()));
            if let Some(scope) = self.attempt_scope {
                let Some(session) = &mut local.session else {
                    panic!("operation scope outlived its attempt");
                };
                assert_eq!(session.current_scope, Some(scope.id));
                session.current_scope = scope.parent;
            }
        });
        #[cfg(test)]
        if self.counted {
            self.zalsa.attempt_operations.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Runs ordinary work under one worker registration, without changing query policy.
/// Nested scopes and queries reuse the registration for this database. A scope inside
/// an attempt shares that attempt's allowance and completion status.
pub fn try_with_operation<T>(db: &dyn Database, body: impl FnOnce() -> T) -> Result<T, StartError> {
    let zalsa = db.zalsa();
    let scope = LOCAL.with_borrow_mut(|local| {
        let database = std::ptr::from_ref(zalsa).addr();
        if let Some(reservation) = &mut local.structural {
            reservation
                .check(
                    database,
                    Some(std::ptr::from_ref(db.zalsa_local()).addr()),
                    None,
                )
                .map_err(|_| StartError::ActiveOperation)?;
        }
        let attempt_scope = match &local.session {
            Some(session) => {
                assert_eq!(
                    session.support.owner.database, database,
                    "foreign database in attempt"
                );
                Some(ScopeOwnership {
                    id: session
                        .next_scope
                        .ok_or(StartError::ScopeIdentityExhausted)?,
                    parent: session.current_scope,
                })
            }
            None => None,
        };
        #[cfg(test)]
        let counted = !local.has_registration(database);
        #[cfg(test)]
        if counted {
            zalsa.attempt_operations.fetch_add(1, Ordering::SeqCst);
            #[cfg(all(test, not(feature = "shuttle")))]
            registration_test_support::after_increment(registration_test_support::EntryKind::Scope);
        }
        let depth = local.scopes.len();
        local.scopes.push(database);
        if let (Some(session), Some(scope)) = (&mut local.session, attempt_scope) {
            session.current_scope = Some(scope.id);
            session.next_scope = scope.id.checked_add(1);
        }
        Ok(OperationScope {
            depth,
            zalsa,
            #[cfg(test)]
            counted,
            attempt_scope,
        })
    })?;
    let value = body();
    drop(scope);
    Ok(value)
}

struct Installed {
    support: AttemptSupport,
    finished: bool,
}

impl Drop for Installed {
    fn drop(&mut self) {
        LOCAL.with_borrow_mut(|local| {
            assert!(
                local
                    .session
                    .as_ref()
                    .is_some_and(|session| { session.support.same_owner(&self.support) }),
                "attempt lost its installed session"
            );
            if !self.finished {
                self.support.owner.state.store(ABANDONED, Ordering::Release);
            }
            assert!(local.operations.is_empty());
            assert!(!local.cycle_support_scope);
            local.session = None;
        });
    }
}

/// Runs one evaluation on this worker. Nested semantic queries share its installed allowance.
/// Other workers can independently evaluate queries against the same database storage.
pub fn try_with_attempt<T>(
    db: &dyn Database,
    allowance: usize,
    body: impl FnOnce() -> T,
) -> Result<AttemptOutcome<T>, StartError> {
    with_root(db, allowance, AdmissionMode::LegacyCallback, |_| body())
        .map(|receipt| receipt.outcome)
}

/// Runs one evaluation with runtime-owned work and requested-byte limits.
///
/// Nested operations share these counters. Other workers retain independent roots and limits.
/// The capability is borrowed by registries and cannot outlive this call.
///
/// ```
/// use salsa::attempt_probe::AttemptOutcome;
/// use salsa::execution_probe::{ExecutionLimits, RegistryBuilder, try_with_execution_budget};
/// let db = salsa::DatabaseImpl::default();
/// let outcome = try_with_execution_budget(&db,
///     ExecutionLimits { semantic_work: 10, requested_bytes: 1_000_000 },
///     |budget| RegistryBuilder::with_budget(&db, &budget)?.seal()?.run(|_| async { Ok(42) }));
/// assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(42))));
/// ```
///
/// ```compile_fail
/// use salsa::execution_probe::{ExecutionBudget, ExecutionLimits, try_with_execution_budget};
/// fn escape(db: &dyn salsa::Database) -> ExecutionBudget<'static> {
///     let mut escaped = None;
///     try_with_execution_budget(db, ExecutionLimits { semantic_work: 1, requested_bytes: 1024 },
///         |budget| escaped = Some(budget));
///     escaped.unwrap()
/// }
/// ```
pub fn try_with_execution_budget<T>(
    db: &dyn Database,
    limits: ExecutionLimits,
    body: impl for<'session> FnOnce(ExecutionBudget<'session>) -> T,
) -> Result<AttemptOutcome<T>, StartError> {
    try_with_metered_execution_budget(db, limits, body).map(|receipt| receipt.outcome)
}

/// Runs one evaluation and reports its final admitted work and requested-byte consumption.
///
/// This has the same admission and outcome rules as [`try_with_execution_budget`]. The receipt
/// includes charges accepted before a refusal or interruption, including charges made while the
/// body's local owners are dropped. Counters are read after the body returns and before the attempt
/// is removed. A panic, including native database cancellation, unwinds without returning a receipt.
///
/// A caller coordinating several evaluations can subtract each receipt's usage from its remaining
/// limits before starting the next evaluation. The receipt retains no attempt or budget authority;
/// each evaluation must start after the preceding one has returned and released its owners.
pub fn try_with_metered_execution_budget<T>(
    db: &dyn Database,
    limits: ExecutionLimits,
    body: impl for<'session> FnOnce(ExecutionBudget<'session>) -> T,
) -> Result<ExecutionReceipt<T>, StartError> {
    with_root(
        db,
        limits.semantic_work,
        AdmissionMode::Budget {
            remaining_requested_bytes: limits.requested_bytes,
        },
        |support| {
            body(ExecutionBudget {
                support: support.clone(),
                marker: PhantomData,
            })
        },
    )
}

fn with_root<T>(
    db: &dyn Database,
    allowance: usize,
    admission: AdmissionMode,
    body: impl for<'session> FnOnce(&'session AttemptSupport) -> T,
) -> Result<ExecutionReceipt<T>, StartError> {
    if db.zalsa_local().active_query().is_some() {
        return Err(StartError::ActiveQuery);
    }
    let requested_bytes = match admission {
        AdmissionMode::LegacyCallback => 0,
        AdmissionMode::Budget {
            remaining_requested_bytes,
        } => remaining_requested_bytes,
    };
    let support = LOCAL.with_borrow_mut(|local| {
        if local.session.is_some() || local.structural.is_some() {
            return Err(StartError::NestedAttempt);
        }
        if !local.operations.is_empty() || !local.scopes.is_empty() {
            return Err(StartError::ActiveOperation);
        }
        assert!(!local.cycle_support_scope);
        let support = AttemptSupport {
            owner: Arc::new(AttemptToken {
                database: std::ptr::from_ref(db.zalsa()).addr(),
                revision: db.zalsa().current_revision(),
                cancellation: db.zalsa().runtime().cancellation_count(),
                state: AtomicU8::new(RUNNING),
            }),
            explicitly_incomplete: false,
        };
        local.session = Some(Session {
            support: support.clone(),
            local: std::ptr::from_ref(db.zalsa_local()).addr(),
            remaining: allowance,
            admission,
            current_scope: None,
            next_scope: Some(NonZeroUsize::MIN),
        });
        Ok(support)
    })?;
    let stamp = Stamp::current(db);
    let mut installed = Installed {
        support,
        finished: false,
    };
    let value = body(&installed.support);
    assert!(stamp.belongs_to(db), "database changed during attempt");
    let (incomplete, usage) = LOCAL.with_borrow(|local| {
        let session = local
            .session
            .as_ref()
            .expect("attempt lost its installed session");
        let remaining_requested_bytes = match session.admission {
            AdmissionMode::LegacyCallback => 0,
            AdmissionMode::Budget {
                remaining_requested_bytes,
            } => remaining_requested_bytes,
        };
        (
            session.support.owner.reason(),
            ExecutionUsage {
                semantic_work: allowance - session.remaining,
                requested_bytes: requested_bytes - remaining_requested_bytes,
            },
        )
    });
    if incomplete.is_none() {
        LOCAL.with_borrow(|local| {
            if let Some(session) = &local.session {
                session
                    .support
                    .owner
                    .state
                    .store(FINISHED, Ordering::Release);
            }
        });
    }
    installed.finished = true;
    drop(installed);
    let outcome = match incomplete {
        Some(reason) => AttemptOutcome::Incomplete(reason),
        None => AttemptOutcome::Complete(value),
    };
    Ok(ExecutionReceipt { outcome, usage })
}

pub(crate) fn charge_requested_bytes(db: &dyn Database, bytes: usize) -> Result<(), Incomplete> {
    let result = LOCAL.with_borrow_mut(|local| {
        assert_can_refuse(local, db);
        let session = local.session.as_mut().expect("byte charge outside attempt");
        if let Some(reason) = session.support.reason() {
            return Err(reason);
        }
        let AdmissionMode::Budget {
            remaining_requested_bytes,
        } = &mut session.admission
        else {
            panic!("byte charge outside a budget root");
        };
        // Saturating allocation quotations use MAX as the overflow sentinel.
        if bytes != usize::MAX
            && let Some(remaining) = remaining_requested_bytes.checked_sub(bytes)
        {
            *remaining_requested_bytes = remaining;
            Ok(())
        } else {
            Err(Incomplete::RequestedAllocation)
        }
    });
    result.map_err(|reason| report_incomplete(db, reason))
}

pub fn charge(db: &dyn Database, units: usize) -> Result<(), Incomplete> {
    let result = LOCAL.with_borrow_mut(|local| {
        assert_can_refuse(local, db);
        let session = local
            .session
            .as_mut()
            .expect("constructor charge outside attempt");
        if let Some(reason) = session.support.owner.reason() {
            return Err(reason);
        }
        if let Some(remaining) = session.remaining.checked_sub(units) {
            session.remaining = remaining;
            Ok(())
        } else {
            #[cfg(test)]
            charge_observation::refused(units, session.remaining);
            Err(Incomplete::Allowance)
        }
    });
    if let Err(reason) = result {
        return Err(report_incomplete(db, reason));
    }
    result
}

/// Reads the remaining budget of this thread's current, running attempt for diagnostics.
///
/// Returns `None` outside that attempt, for another database or stamp, or after refusal.
/// This does not admit work, invoke cancellation callbacks, emit events or mark query support. The value
/// is an observation, not permission to perform work without the normal admission controls.
#[doc(hidden)]
pub fn remaining_allowance_for_diagnostics(db: &dyn Database) -> Option<usize> {
    let zalsa = db.zalsa();
    LOCAL.with_borrow(|local| {
        let session = local.session.as_ref()?;
        let owner = &session.support.owner;
        (owner.database == std::ptr::from_ref(zalsa).addr()
            && owner.revision == zalsa.current_revision()
            && owner.cancellation == zalsa.runtime().cancellation_count()
            && owner.state.load(Ordering::Acquire) == RUNNING)
            .then_some(session.remaining)
    })
}

fn assert_can_refuse(local: &Local, db: &dyn Database) {
    // An attempt admits no unclassified operations, and every descendant of a
    // complete-only operation must also be complete-only.
    assert!(
        local
            .operations
            .last()
            .is_none_or(|(_, policy)| policy.allows_incomplete()),
        "constructor work crossed a complete-only query operation"
    );
    let session = local
        .session
        .as_ref()
        .expect("constructor work outside attempt");
    assert_eq!(
        session.support.owner.database,
        std::ptr::from_ref(db.zalsa()).addr()
    );
}

/// Refuses further completion while preserving the first reason reported by this attempt.
pub fn report_incomplete(db: &dyn Database, reason: Incomplete) -> Incomplete {
    let reason = LOCAL.with_borrow_mut(|local| {
        assert_can_refuse(local, db);
        let session = local
            .session
            .as_mut()
            .expect("constructor work outside attempt");
        session.support.owner.report_incomplete(reason)
    });
    db.zalsa_local().mark_attempt_incomplete(db.zalsa());
    reason
}

/// Checks the installed attempt without spending its remaining allowance.
/// A true result marks the active query because it may return recovery data based on this read.
pub fn is_incomplete(db: &dyn Database) -> bool {
    let incomplete = LOCAL.with_borrow(|local| {
        if local.session.is_none() {
            return false;
        }
        assert_can_refuse(local, db);
        local
            .session
            .as_ref()
            .is_some_and(|session| session.support.owner.reason().is_some())
    });
    if incomplete {
        db.zalsa_local().mark_attempt_incomplete(db.zalsa());
    }
    incomplete
}

pub(crate) fn current() -> Option<AttemptSupport> {
    LOCAL.with_borrow(|local| {
        local.session.as_ref().map(|session| {
            let mut support = session.support.clone();
            support.explicitly_incomplete = false;
            support
        })
    })
}

pub(crate) fn current_query() -> Option<AttemptSupport> {
    current_operation_policy().allows_incomplete()
        .then(current)
        .flatten()
}

pub(crate) fn current_cycle_support(zalsa: &Zalsa) -> Option<AttemptSupport> {
    if let Some(support) = current_query() {
        return Some(support);
    }
    let in_scope = LOCAL.with_borrow(|local| local.cycle_support_scope);
    if in_scope {
        current().filter(|support| support.is_running_current(zalsa))
    } else {
        None
    }
}

struct IncompleteObservation {
    previous: Option<bool>,
}

impl Drop for IncompleteObservation {
    fn drop(&mut self) {
        LOCAL.with_borrow_mut(|local| {
            let observed = local.incomplete_observation == Some(true);
            local.incomplete_observation = self.previous.map(|previous| previous || observed);
        });
    }
}

/// Records incomplete use by a callback that has no query frame of its own.
/// An earlier refusal alone does not make an independent callback incomplete.
/// Nested observations propagate to their caller, including when it catches a panic.
pub(crate) fn with_incomplete_observation<T>(body: impl FnOnce() -> T) -> (T, bool) {
    if current_query().is_none() {
        return (body(), false);
    }
    let observation = IncompleteObservation {
        previous: LOCAL.with_borrow_mut(|local| local.incomplete_observation.replace(false)),
    };
    let value = body();
    let observed = LOCAL.with_borrow(|local| local.incomplete_observation == Some(true));
    drop(observation);
    (value, observed)
}

pub(crate) fn observe_incomplete(support: &AttemptSupport) {
    LOCAL.with_borrow_mut(|local| {
        let session = local
            .session
            .as_mut()
            .expect("incomplete query read outside attempt");
        assert!(
            session.support.same_owner(support),
            "foreign incomplete query read"
        );
        session.support.make_incomplete();
        if let Some(observed) = &mut local.incomplete_observation {
            *observed = true;
        }
    });
}

pub(crate) struct Operation<'a> {
    depth: usize,
    previous_cycle_support_scope: bool,
    zalsa: &'a Zalsa,
    #[cfg(test)]
    counted: bool,
}

impl Operation<'_> {
    #[cfg(test)]
    pub(crate) fn is_current(&self, policy: QueryPolicy) -> bool {
        self.is_current_execution(OperationPolicy::ordinary(policy))
    }

    pub(crate) fn is_current_execution(&self, policy: OperationPolicy) -> bool {
        LOCAL.with_borrow(|local| {
            local.operations.len() == self.depth + 1
                && local.operations.last() == Some(&(std::ptr::from_ref(self.zalsa).addr(), policy))
        })
    }
}

pub(crate) fn stack_depths() -> (usize, usize) {
    LOCAL.with_borrow(|local| (local.operations.len(), local.scopes.len()))
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        LOCAL.with_borrow_mut(|local| {
            debug_assert_eq!(local.operations.len(), self.depth + 1);
            local.operations.pop();
            local.cycle_support_scope = self.previous_cycle_support_scope;
            debug_assert!(!local.operations.is_empty() || !local.cycle_support_scope);
        });
        #[cfg(test)]
        if self.counted {
            self.zalsa.attempt_operations.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

pub(crate) fn enter<'a>(
    zalsa: &'a Zalsa,
    policy: QueryPolicy,
    name: &'static str,
) -> Operation<'a> {
    enter_with_policy(zalsa, OperationPolicy::ordinary(policy), name)
}

pub(crate) fn check_admitted_entry(policy: QueryPolicy) -> Result<(), &'static str> {
    check_query_dependency(policy)?;
    LOCAL.with_borrow(|local| {
        if local.structural.is_some()
            || local.operations.last().is_some_and(|(_, parent)| {
                parent.declared == QueryPolicy::CompleteOnly && !parent.allows_incomplete()
            })
        {
            return Err("controlled callable has an ordinary complete-only ancestor");
        }
        Ok(())
    })
}

pub(crate) fn check_query_dependency(policy: QueryPolicy) -> Result<(), &'static str> {
    LOCAL.with_borrow(|local| {
        if policy != QueryPolicy::CompleteOnly
            && local
                .operations
                .last()
                .is_some_and(|(_, parent)| parent.declared == QueryPolicy::CompleteOnly)
        {
            return Err("complete-only callable requested a non-complete query");
        }
        Ok(())
    })
}

pub(crate) fn enter_admitted<'a>(
    zalsa: &'a Zalsa,
    policy: QueryPolicy,
    name: &'static str,
) -> Result<Operation<'a>, &'static str> {
    check_admitted_entry(policy)?;
    Ok(enter_with_policy(
        zalsa,
        OperationPolicy::admitted(policy),
        name,
    ))
}

fn enter_with_policy<'a>(
    zalsa: &'a Zalsa,
    policy: OperationPolicy,
    name: &'static str,
) -> Operation<'a> {
    LOCAL.with_borrow_mut(|local| {
        let database = std::ptr::from_ref(zalsa).addr();
        if let Some(reservation) = &mut local.structural
            && let Err(error) = reservation.check(database, None, Some(policy.declared))
        {
            panic!("{error}: {name}");
        }
        if let Some(session) = &local.session {
            assert_eq!(
                session.support.owner.database, database,
                "foreign database in attempt"
            );
            assert_ne!(
                policy.declared,
                QueryPolicy::Unclassified,
                "unclassified query in attempt: {name}"
            );
        }
        // Admitting only complete-only children keeps every descendant of a
        // complete-only operation complete-only, so checking the parent is enough.
        assert!(
            policy.declared == QueryPolicy::CompleteOnly
                || local
                    .operations
                    .last()
                    .is_none_or(|(_, parent)| parent.declared != QueryPolicy::CompleteOnly),
            "complete-only query requested interruptible semantic work"
        );
        #[cfg(test)]
        let counted = !local.has_registration(database);
        #[cfg(test)]
        if counted {
            zalsa.attempt_operations.fetch_add(1, Ordering::SeqCst);
            #[cfg(all(test, not(feature = "shuttle")))]
            registration_test_support::after_increment(registration_test_support::EntryKind::Query);
        }
        let depth = local.operations.len();
        let previous_cycle_support_scope = local.cycle_support_scope;
        local.cycle_support_scope |= policy.allows_incomplete();
        local.operations.push((database, policy));
        Operation {
            depth,
            previous_cycle_support_scope,
            zalsa,
            #[cfg(test)]
            counted,
        }
    })
}

pub(crate) fn current_policy() -> QueryPolicy {
    current_operation_policy().declared
}

pub(crate) fn current_operation_policy() -> OperationPolicy {
    LOCAL.with_borrow(|local| {
        local
            .operations
            .last()
            .map_or(OperationPolicy::ordinary(QueryPolicy::Unclassified), |(_, policy)| *policy)
    })
}

pub(crate) fn assert_output_allowed(local: &crate::zalsa_local::ZalsaLocal) {
    let owner = local.try_with_query_stack(|stack| stack.last().map(|query| query.attempt_policy));
    assert!(
        owner.flatten().is_none_or(|policy| !policy.allows_incomplete()),
        "return-only query attempted to create a tracked output"
    );
    LOCAL.with_borrow(|local| {
        assert!(
            local
                .operations
                .last()
                .is_none_or(|(_, policy)| !policy.allows_incomplete()),
            "return-only query attempted to create a tracked output"
        );
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::num::NonZeroUsize;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{
        AttemptOutcome, ExecutionLimits, ExecutionReceipt, ExecutionUsage, Incomplete, LOCAL,
        QueryPolicy, StartError, StructuralPreparation, assert_structural_query, charge,
        charge_requested_bytes, check_structural_access, current, enter, report_incomplete,
        stack_depths, try_with_attempt, try_with_metered_execution_budget, try_with_operation,
        with_incomplete_observation,
    };
    use crate::execution_probe::{ExecutionWork, RegistryBuilder};
    use crate::sync::atomic::Ordering;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Database, DatabaseImpl};

    const METERED_LIMITS: ExecutionLimits = ExecutionLimits {
        semantic_work: 7,
        requested_bytes: 11,
    };

    fn assert_attempt_cleaned_up(db: &dyn Database) {
        assert!(current().is_none());
        assert_eq!(stack_depths(), (0, 0));
        assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn metered_registered_driver_reports_exact_shared_consumption() {
        let db = DatabaseImpl::default();
        let run = |limits| {
            try_with_metered_execution_budget(&db, limits, |budget| {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        Ok(endpoint
                            .local_call(|| {
                                endpoint.admit_work(3)?;
                                endpoint.admit(ExecutionWork::Resource { requested_bytes: 5 })?;
                                Ok(42)
                            })
                            .await)
                    })
            })
        };
        let measured = run(ExecutionLimits {
            semantic_work: 10_000,
            requested_bytes: 1_000_000,
        })
        .unwrap();
        assert_eq!(measured.outcome, AttemptOutcome::Complete(Ok(42)));
        assert!(measured.usage.semantic_work >= 3);
        assert!(measured.usage.requested_bytes > 5);
        assert_attempt_cleaned_up(&db);
        let exact_limits = ExecutionLimits {
            semantic_work: measured.usage.semantic_work,
            requested_bytes: measured.usage.requested_bytes,
        };
        assert_eq!(run(exact_limits), Ok(measured));
        assert_attempt_cleaned_up(&db);
        for (limits, reason) in [
            (
                ExecutionLimits {
                    semantic_work: exact_limits.semantic_work - 1,
                    ..exact_limits
                },
                Incomplete::Allowance,
            ),
            (
                ExecutionLimits {
                    requested_bytes: exact_limits.requested_bytes - 1,
                    ..exact_limits
                },
                Incomplete::RequestedAllocation,
            ),
        ] {
            let receipt = run(limits).unwrap();
            assert_eq!(receipt.outcome, AttemptOutcome::Incomplete(reason));
            assert!(receipt.usage.semantic_work <= limits.semantic_work);
            assert!(receipt.usage.requested_bytes <= limits.requested_bytes);
            assert_attempt_cleaned_up(&db);
        }
    }

    #[test]
    fn metered_refusal_preserves_prior_accepted_charges() {
        for reason in [
            Incomplete::Allowance,
            Incomplete::RequestedAllocation,
            Incomplete::Interrupted,
        ] {
            let db = DatabaseImpl::default();
            let receipt = try_with_metered_execution_budget(&db, METERED_LIMITS, |_| {
                charge(&db, 3).unwrap();
                charge_requested_bytes(&db, 5).unwrap();
                match reason {
                    Incomplete::Allowance => assert_eq!(charge(&db, 5), Err(reason)),
                    Incomplete::RequestedAllocation => {
                        assert_eq!(charge_requested_bytes(&db, 7), Err(reason));
                    }
                    Incomplete::Interrupted => {
                        assert_eq!(report_incomplete(&db, reason), reason);
                    }
                }
                assert_eq!(charge(&db, 1), Err(reason));
                assert_eq!(charge_requested_bytes(&db, 1), Err(reason));
                assert_eq!(report_incomplete(&db, Incomplete::Interrupted), reason);
            });
            assert_eq!(
                receipt,
                Ok(ExecutionReceipt {
                    outcome: AttemptOutcome::Incomplete(reason),
                    usage: ExecutionUsage {
                        semantic_work: 3,
                        requested_bytes: 5,
                    },
                })
            );
            assert_attempt_cleaned_up(&db);
        }
    }

    struct ChargedTeardown<'db> {
        db: &'db dyn Database,
        expected: Result<(), Incomplete>,
    }

    impl Drop for ChargedTeardown<'_> {
        fn drop(&mut self) {
            assert_eq!(charge(self.db, 4), self.expected);
            assert_eq!(charge_requested_bytes(self.db, 6), self.expected);
        }
    }

    #[test]
    fn metered_receipt_includes_body_teardown_and_its_refusal() {
        for initial_work in [3, 4] {
            let db = DatabaseImpl::default();
            let expected = if initial_work == 3 {
                Ok(())
            } else {
                Err(Incomplete::Allowance)
            };
            let receipt = try_with_metered_execution_budget(&db, METERED_LIMITS, |_| {
                let _teardown = ChargedTeardown { db: &db, expected };
                charge(&db, initial_work).unwrap();
                charge_requested_bytes(&db, 5).unwrap();
                42
            });
            assert_eq!(
                receipt,
                Ok(match expected {
                    Ok(()) => ExecutionReceipt {
                        outcome: AttemptOutcome::Complete(42),
                        usage: ExecutionUsage {
                            semantic_work: 7,
                            requested_bytes: 11,
                        },
                    },
                    Err(reason) => ExecutionReceipt {
                        outcome: AttemptOutcome::Incomplete(reason),
                        usage: ExecutionUsage {
                            semantic_work: 4,
                            requested_bytes: 5,
                        },
                    },
                })
            );
            assert_attempt_cleaned_up(&db);
        }
    }

    #[test]
    fn metered_retry_can_spend_only_the_supplied_remainder() {
        let db = DatabaseImpl::default();
        let first = try_with_metered_execution_budget(&db, METERED_LIMITS, |_| {
            charge(&db, 3).unwrap();
            charge_requested_bytes(&db, 5).unwrap();
            report_incomplete(&db, Incomplete::Interrupted);
        })
        .unwrap();
        assert_eq!(
            first.outcome,
            AttemptOutcome::Incomplete(Incomplete::Interrupted)
        );
        assert_attempt_cleaned_up(&db);
        let remaining = ExecutionLimits {
            semantic_work: METERED_LIMITS.semantic_work - first.usage.semantic_work,
            requested_bytes: METERED_LIMITS.requested_bytes - first.usage.requested_bytes,
        };
        let second = try_with_metered_execution_budget(&db, remaining, |_| {
            charge(&db, 4).unwrap();
            charge_requested_bytes(&db, 6).unwrap();
            assert_eq!(charge(&db, 1), Err(Incomplete::Allowance));
            assert_eq!(charge_requested_bytes(&db, 1), Err(Incomplete::Allowance));
        })
        .unwrap();
        assert_eq!(
            second.outcome,
            AttemptOutcome::Incomplete(Incomplete::Allowance)
        );
        assert_eq!(first.usage.semantic_work + second.usage.semantic_work, 7);
        assert_eq!(
            first.usage.requested_bytes + second.usage.requested_bytes,
            11
        );
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn metered_nested_start_does_not_run_or_change_the_parent_budget() {
        let db = DatabaseImpl::default();
        let called = Cell::new(false);
        let receipt = try_with_metered_execution_budget(&db, METERED_LIMITS, |_| {
            charge(&db, 3).unwrap();
            charge_requested_bytes(&db, 5).unwrap();
            assert_eq!(
                try_with_metered_execution_budget(&db, METERED_LIMITS, |_| called.set(true)),
                Err(StartError::NestedAttempt)
            );
        });
        assert!(!called.get());
        assert_eq!(
            receipt,
            Ok(ExecutionReceipt {
                outcome: AttemptOutcome::Complete(()),
                usage: ExecutionUsage {
                    semantic_work: 3,
                    requested_bytes: 5,
                },
            })
        );
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn metered_panic_unwinds_without_a_receipt_and_cleans_up() {
        let db = DatabaseImpl::default();
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_metered_execution_budget::<()>(&db, METERED_LIMITS, |_| {
                let _teardown = ChargedTeardown {
                    db: &db,
                    expected: Ok(()),
                };
                charge(&db, 3).unwrap();
                charge_requested_bytes(&db, 5).unwrap();
                panic!("metered body panic");
            })
        }));
        let payload = result.unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"metered body panic"));
        assert_attempt_cleaned_up(&db);
        assert_eq!(
            try_with_attempt(&db, 0, || 7),
            Ok(AttemptOutcome::Complete(7))
        );
    }

    #[test]
    fn structural_preparation_preserves_the_root_and_cumulative_allowance() {
        let db = DatabaseImpl::default();
        let other = DatabaseImpl::default();
        let receipt = try_with_metered_execution_budget(&db, METERED_LIMITS, |_| {
            let support = current().expect("installed attempt");
            charge(&db, 3).unwrap();
            charge_requested_bytes(&db, 5).unwrap();
            try_with_operation(&db, || {
                let operation = enter(db.zalsa(), QueryPolicy::ReturnOnly, "semantic");
                let ownership = support.local_ownership(db.zalsa());
                let depths = stack_depths();
                let (_, observed) = with_incomplete_observation(|| {
                    for _ in 0..2 {
                        let preparation = StructuralPreparation::park(&db, &support).unwrap();
                        assert!(current().is_none());
                        assert_eq!(stack_depths(), (0, 0));
                        assert_eq!(check_structural_access(&db), Ok(()));
                        for db in [&db, &other] {
                            assert_eq!(
                                try_with_attempt(db, 100, || panic!("nested root ran")),
                                Err(StartError::NestedAttempt)
                            );
                        }
                        assert!(StructuralPreparation::park(&db, &support).is_err());
                        try_with_operation(&db, || {
                            let structural =
                                enter(db.zalsa(), QueryPolicy::CompleteOnly, "structural");
                            assert!(preparation.check_complete().is_err());
                            drop(structural);
                        })
                        .unwrap();
                        assert_eq!(preparation.check_complete(), Ok(()));
                        drop(preparation);
                        assert!(current().unwrap().same_owner(&support));
                        assert_eq!(support.local_ownership(db.zalsa()), ownership);
                        assert_eq!(stack_depths(), depths);
                        assert!(operation.is_current(QueryPolicy::ReturnOnly));
                    }
                });
                assert!(!observed);
                charge(&db, 4).unwrap();
                charge_requested_bytes(&db, 6).unwrap();
                assert_eq!(charge(&db, 1), Err(Incomplete::Allowance));
            })
            .unwrap();
        });
        assert_eq!(
            receipt,
            Ok(ExecutionReceipt {
                outcome: AttemptOutcome::Incomplete(Incomplete::Allowance),
                usage: ExecutionUsage {
                    semantic_work: 7,
                    requested_bytes: 11,
                },
            })
        );
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn structural_preparation_rejections_are_sticky() {
        let db = DatabaseImpl::default();
        let other = DatabaseImpl::default();
        let other_local = db.clone();
        let outcome = try_with_attempt(&db, 1, || {
            let support = current().unwrap();
            for policy in [QueryPolicy::ReturnOnly, QueryPolicy::Unclassified] {
                for generated_entry in [false, true] {
                    let preparation = StructuralPreparation::park(&db, &support).unwrap();
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        if generated_entry {
                            assert_structural_query(&db, policy);
                        } else {
                            let _operation = enter(db.zalsa(), policy, "rejected");
                        }
                    }));
                    assert!(result.is_err());
                    assert!(preparation.check_complete().is_err());
                    assert_eq!(stack_depths(), (0, 0));
                }
            }
            for foreign in [&other, &other_local] {
                let preparation = StructuralPreparation::park(&db, &support).unwrap();
                assert!(check_structural_access(foreign).is_err());
                assert!(preparation.check_complete().is_err());
                drop(preparation);
                let preparation = StructuralPreparation::park(&db, &support).unwrap();
                assert_eq!(
                    try_with_operation(foreign, || panic!("foreign operation ran")),
                    Err(StartError::ActiveOperation)
                );
                assert!(preparation.check_complete().is_err());
            }
            assert!(current().unwrap().same_owner(&support));
            charge(&db, 1).unwrap();
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(())));
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn structural_preparation_rejected_entry_preserves_semantic_state() {
        let db = DatabaseImpl::default();
        let other_local = db.clone();
        let outcome = try_with_attempt(&db, 0, || {
            let support = current().unwrap();
            assert!(StructuralPreparation::park(&other_local, &support).is_err());
            LOCAL.with(|local| {
                let _borrowed = local.borrow();
                assert!(StructuralPreparation::park(&db, &support).is_err());
            });
            let preparation = StructuralPreparation::park(&db, &support).unwrap();
            LOCAL.with(|local| {
                let _borrowed = local.borrow_mut();
                assert!(preparation.check_complete().is_err());
            });
            assert_eq!(preparation.check_complete(), Ok(()));
            drop(preparation);
            assert!(current().unwrap().same_owner(&support));
            report_incomplete(&db, Incomplete::Interrupted);
            assert!(StructuralPreparation::park(&db, &support).is_err());
            assert_eq!(support.reason(), Some(Incomplete::Interrupted));
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn structural_preparation_restores_semantic_ownership_before_panic_delivery() {
        let db = DatabaseImpl::default();
        let outcome = try_with_attempt(&db, 1, || {
            try_with_operation(&db, || {
                let operation = enter(db.zalsa(), QueryPolicy::ReturnOnly, "semantic");
                let support = current().unwrap();
                let ownership = support.local_ownership(db.zalsa());
                let depths = stack_depths();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let _preparation = StructuralPreparation::park(&db, &support).unwrap();
                    let _structural = enter(db.zalsa(), QueryPolicy::CompleteOnly, "structural");
                    panic!("structural producer panic");
                }));
                assert_eq!(
                    result.unwrap_err().downcast_ref::<&str>(),
                    Some(&"structural producer panic")
                );
                assert!(current().unwrap().same_owner(&support));
                assert_eq!(support.local_ownership(db.zalsa()), ownership);
                assert_eq!(stack_depths(), depths);
                assert!(operation.is_current(QueryPolicy::ReturnOnly));
                charge(&db, 1).unwrap();
            })
            .unwrap();
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(())));
        assert_attempt_cleaned_up(&db);
    }

    #[test]
    fn scope_identity_exhaustion_preserves_enclosing_ownership() {
        let db = DatabaseImpl::default();
        let db: &dyn Database = &db;
        let result = try_with_attempt(db, 0, || {
            let support = current().expect("installed attempt");
            let root = support.local_ownership(db.zalsa());
            assert_eq!(
                try_with_operation(db, || {
                    let parent = support.local_ownership(db.zalsa());
                    LOCAL.with_borrow_mut(|local| {
                        local
                            .session
                            .as_mut()
                            .expect("installed attempt")
                            .next_scope = Some(NonZeroUsize::MAX);
                    });
                    assert_eq!(
                        try_with_operation(db, || {
                            let last = support.local_ownership(db.zalsa());
                            assert_ne!(last, parent);
                            let depths = stack_depths();
                            let registrations =
                                db.zalsa().attempt_operations.load(Ordering::SeqCst);
                            assert_eq!(
                                try_with_operation(db, || panic!("exhausted scope body ran")),
                                Err(StartError::ScopeIdentityExhausted)
                            );
                            assert_eq!(support.local_ownership(db.zalsa()), last);
                            assert_eq!(stack_depths(), depths);
                            assert_eq!(
                                db.zalsa().attempt_operations.load(Ordering::SeqCst),
                                registrations
                            );
                        }),
                        Ok(())
                    );
                    assert_eq!(support.local_ownership(db.zalsa()), parent);
                }),
                Ok(())
            );
            assert_eq!(support.local_ownership(db.zalsa()), root);
            assert_eq!(
                try_with_operation(db, || panic!("scope identity was reused")),
                Err(StartError::ScopeIdentityExhausted)
            );
            assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
            assert_eq!(stack_depths(), (0, 0));
        });
        assert_eq!(result, Ok(AttemptOutcome::Complete(())));
    }

    #[test]
    fn operation_scope_holds_one_registration_across_query_operations() {
        let db = DatabaseImpl::default();
        let db: &dyn Database = &db;
        let other = DatabaseImpl::default();
        let other: &dyn Database = &other;
        let count = || db.zalsa().attempt_operations.load(Ordering::SeqCst);
        let other_count = || other.zalsa().attempt_operations.load(Ordering::SeqCst);
        assert_eq!(count(), 0);
        assert_eq!(
            try_with_operation(db, || {
                assert_eq!(count(), 1);
                for _ in 0..2 {
                    let operation = enter(db.zalsa(), QueryPolicy::Unclassified, "ordinary");
                    assert_eq!(count(), 1);
                    assert_eq!(
                        try_with_operation(db, || {
                            assert_eq!(count(), 1);
                            assert_eq!(
                                try_with_operation(other, || {
                                    assert_eq!((count(), other_count()), (1, 1));
                                    let _operation =
                                        enter(other.zalsa(), QueryPolicy::Unclassified, "other");
                                    assert_eq!((count(), other_count()), (1, 1));
                                }),
                                Ok(())
                            );
                            assert_eq!(other_count(), 0);
                        }),
                        Ok(())
                    );
                    drop(operation);
                    assert_eq!(count(), 1);
                }
            }),
            Ok(())
        );
        assert_eq!((count(), other_count()), (0, 0));
        let operation = enter(db.zalsa(), QueryPolicy::Unclassified, "unscoped");
        assert_eq!(count(), 1);
        assert_eq!(try_with_operation(db, || assert_eq!(count(), 1)), Ok(()));
        drop(operation);
        assert_eq!(count(), 0);
    }
}
