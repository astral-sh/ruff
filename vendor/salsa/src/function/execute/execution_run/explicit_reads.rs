//! A lexical execution boundary rejects ordinary reads before they enter Salsa.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::panic::{panic_any, resume_unwind};

use super::{RunError, RunResult};
use crate::Database;
use crate::attempt_probe::{self, ExecutionBudget};
use crate::sync::thread;
use crate::zalsa::Zalsa;
use crate::zalsa_local::ZalsaLocal;

/// Native panic payload for an ordinary Salsa operation inside an explicit-read boundary.
///
/// This indicates an implementation contract violation, not resource refusal. Native unwind
/// rules apply: callback-local values can be destroyed before queued children are drained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrdinaryReadViolation {
    pub operation: &'static str,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct BoundaryIdentity {
    budget: usize,
    database: usize,
    local: usize,
}

#[derive(Clone, Copy)]
struct BoundaryState {
    identity: BoundaryIdentity,
    scope: ScopeId,
    violation: Option<OrdinaryReadViolation>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct ScopeId(NonZeroUsize);

thread_local! {
    static BOUNDARY: Cell<Option<BoundaryState>> = const { Cell::new(None) };
    static NEXT_SCOPE: Cell<Option<NonZeroUsize>> = const { Cell::new(Some(NonZeroUsize::MIN)) };
    static DRAINING: Cell<bool> = const { Cell::new(false) };
}

/// Requires explicit fallible Salsa operations while `body` runs.
///
/// The budget must belong to the current root and worker. Enclose registry construction,
/// execution and destruction in this call. Returned values do not extend the boundary.
/// A registry created inside it cannot execute outside the same lexical scope; arbitrary
/// returned values and their destructors remain the caller's responsibility.
/// Ordinary getters, queries and interning panic with
/// [`OrdinaryReadViolation`]; catching that panic cannot make a later completion succeed.
/// Expected unsupported operations must instead refuse through their fallible effects.
/// This boundary enforces trusted implementations' read discipline; it does not sandbox Rust.
pub fn with_explicit_reads<T>(
    db: &dyn Database,
    budget: &ExecutionBudget<'_>,
    body: impl FnOnce() -> RunResult<T>,
) -> RunResult<T> {
    if !budget.is_current(db) {
        return Err(RunError::Contract(
            "explicit reads require the current execution budget",
        ));
    }
    let identity = BoundaryIdentity {
        budget: std::ptr::from_ref(budget).addr(),
        database: std::ptr::from_ref(db.zalsa()).addr(),
        local: std::ptr::from_ref(db.zalsa_local()).addr(),
    };
    let owns_boundary = BOUNDARY.with(|boundary| match boundary.get() {
        Some(state) if state.identity == identity => Ok(false),
        Some(_) => Err(RunError::Contract(
            "explicit reads changed their enclosing root",
        )),
        None => {
            let scope = NEXT_SCOPE.with(|next| -> RunResult<ScopeId> {
                let scope = next
                    .get()
                    .ok_or(RunError::Contract("explicit read scope identity exhausted"))?;
                next.set(scope.get().checked_add(1).and_then(NonZeroUsize::new));
                Ok(ScopeId(scope))
            })?;
            boundary.set(Some(BoundaryState {
                identity,
                scope,
                violation: None,
            }));
            Ok(true)
        }
    })?;
    let _scope = BoundaryScope { owns_boundary };
    check_acceptance();
    let result = body();
    check_acceptance();
    result
}

struct BoundaryScope {
    owns_boundary: bool,
}

impl Drop for BoundaryScope {
    fn drop(&mut self) {
        if self.owns_boundary {
            BOUNDARY.with(|boundary| boundary.set(None));
        }
    }
}

/// Identifies the storage accessed by an ordinary operation before it discovers an ingredient.
#[doc(hidden)]
pub enum OrdinaryAccess<'db> {
    Database(&'db Zalsa, &'db ZalsaLocal),
    Storage(&'db Zalsa),
    Mutation,
}

impl<'db> From<(&'db Zalsa, &'db ZalsaLocal)> for OrdinaryAccess<'db> {
    fn from((zalsa, local): (&'db Zalsa, &'db ZalsaLocal)) -> Self {
        Self::Database(zalsa, local)
    }
}

/// Checks generated ordinary entry points before key, ingredient or field discovery.
#[doc(hidden)]
pub fn assert_ordinary_execution_allowed(access: OrdinaryAccess<'_>, operation: &'static str) {
    let violation = BOUNDARY.with(|boundary| {
        let mut state = boundary.get()?;
        let violation = *state
            .violation
            .get_or_insert(OrdinaryReadViolation { operation });
        boundary.set(Some(state));
        Some(violation)
    });
    if let Some(violation) = violation {
        panic_any(violation);
    }
    let permitted = match access {
        OrdinaryAccess::Database(zalsa, local) => {
            attempt_probe::check_structural_storage(zalsa, Some(local))
        }
        OrdinaryAccess::Storage(zalsa) => attempt_probe::check_structural_storage(zalsa, None),
        OrdinaryAccess::Mutation => attempt_probe::check_structural_mutation(),
    };
    if let Err(error) = permitted {
        panic!("{error}");
    }
}

pub(crate) fn strict_is_active() -> bool {
    BOUNDARY.with(|boundary| boundary.get().is_some())
}

pub(super) fn current_scope() -> Option<ScopeId> {
    BOUNDARY.with(|boundary| boundary.get().map(|state| state.scope))
}

pub(super) struct ParkedBoundary {
    state: Option<BoundaryState>,
}

impl ParkedBoundary {
    pub(super) fn enter() -> RunResult<Self> {
        check_acceptance();
        if DRAINING.with(Cell::get) {
            return Err(RunError::Contract(
                "structural preparation cannot interrupt owner cleanup",
            ));
        }
        Ok(Self {
            state: BOUNDARY.with(|boundary| boundary.take()),
        })
    }

    pub(super) fn check_finished(&self) -> RunResult<()> {
        if BOUNDARY.with(|boundary| boundary.get().is_some()) || DRAINING.with(Cell::get) {
            return Err(RunError::Contract(
                "structural preparation retained an explicit-read boundary",
            ));
        }
        Ok(())
    }
}

impl Drop for ParkedBoundary {
    fn drop(&mut self) {
        BOUNDARY.with(|boundary| boundary.set(self.state));
    }
}

pub(super) fn check_scope(scope: Option<ScopeId>) -> RunResult<()> {
    if scope.is_some() && scope != current_scope() {
        return Err(RunError::Contract(
            "execution run escaped its explicit read scope",
        ));
    }
    Ok(())
}

pub(crate) fn check_acceptance() {
    let violation = BOUNDARY.with(|boundary| boundary.get().and_then(|state| state.violation));
    let Some(violation) = violation else {
        return;
    };
    if thread::panicking() || DRAINING.with(Cell::get) {
        return;
    }
    resume_unwind(Box::new(violation));
}

pub(super) fn drain_guard() -> Option<DrainGuard> {
    if !strict_is_active() {
        return None;
    }
    // Retiring valid owners must not rethrow the violation that caused their cleanup.
    // Ordinary entry points still reject new calls during this drain.
    Some(DrainGuard {
        previous: DRAINING.with(|draining| draining.replace(true)),
    })
}

pub(super) struct DrainGuard {
    previous: bool,
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        DRAINING.with(|draining| draining.set(self.previous));
    }
}
