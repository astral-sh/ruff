//! Passive observations of constructor matching admissions and traversal order.
//!
//! The callback observes existing boundaries on the executing test thread. It does not change
//! admissions or add suspension; runtime controls poll the actual matching future for `Pending`.

use std::cell::Cell;
use std::fmt;

use super::{ConstructorMatchingOperation, ConstructorMatchingPhase};
use crate::Db;
use crate::types::constraints::control::GrowthPlan;
use crate::types::cyclic::{
    TypeIdentity, TypeTransformationControl, TypeTransformationGrowth, TypeTransformationWork,
};
use crate::types::typevar::constructor_nonce::ConstructorNonceOperation;
use crate::types::{ApplyTypeMappingVisitor, GenericContext, Type, TypeMapping};

/// Identifies admission boundaries, callable visits and retained mapping-visitor lifetime observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    BeforeNonce(ConstructorNonceOperation),
    AfterNonce(ConstructorNonceOperation),
    BeforeMatching(ConstructorMatchingOperation),
    AfterMatching(ConstructorMatchingOperation),
    Entry(ConstructorMatchingPhase, usize),
    MappingEntered {
        visitor: usize,
    },
    MappingChild {
        visitor: usize,
        /// Active scope depth, or `None` if the probe could not observe it; see `active_depth`.
        active: Option<usize>,
    },
    MappingRetired {
        visitor: usize,
        /// Active scope depth, or `None` if the probe could not observe it; see `active_depth`.
        active: Option<usize>,
    },
}

thread_local! {
    static OBSERVER: Cell<Option<fn(Event)>> = const { Cell::new(None) };
}

/// Replaces this thread's callback and returns the previous callback for scoped restoration.
pub(in crate::types) fn set_observer(observer: Option<fn(Event)>) -> Option<fn(Event)> {
    OBSERVER.replace(observer)
}

/// Delivers an event without borrowing the callback slot while the callback runs.
fn observe(event: Event) {
    if let Some(observer) = OBSERVER.get() {
        observer(event);
    }
}

/// Records arrival before a nonce mutation requests its normal admission.
pub(in crate::types) fn observe_before_nonce(operation: ConstructorNonceOperation) {
    observe(Event::BeforeNonce(operation));
}

/// Records a nonce mutation after its admission succeeds and its state changes.
pub(in crate::types) fn observe_after_nonce(operation: ConstructorNonceOperation) {
    observe(Event::AfterNonce(operation));
}

/// Records arrival before matching storage requests its normal admission.
pub(in crate::types) fn observe_before_matching(operation: ConstructorMatchingOperation) {
    observe(Event::BeforeMatching(operation));
}

/// Records matching storage after its admission succeeds and its state changes.
pub(in crate::types) fn observe_after_matching(operation: ConstructorMatchingOperation) {
    observe(Event::AfterMatching(operation));
}

/// Records a callable's address at its actual visit in the indicated traversal phase.
pub(in crate::types) fn observe_entry(phase: ConstructorMatchingPhase, identity: usize) {
    observe(Event::Entry(phase, identity));
}

/// Reads the current transformation depth by refusing before any new scope is installed.
#[derive(Debug)]
struct ActiveDepth;

impl TypeTransformationControl for ActiveDepth {
    type Error = Option<usize>;

    fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Self::Error> {
        match work {
            TypeTransformationWork::CacheLookup { .. }
            | TypeTransformationWork::AncestorComparison
            | TypeTransformationWork::InlinePayload { .. } => Ok(()),
            TypeTransformationWork::ActiveStorage { len, .. } => Err(Some(len)),
            TypeTransformationWork::CacheStorage { .. }
            | TypeTransformationWork::RehashKey { .. }
            | TypeTransformationWork::Grow { .. } => Err(None),
        }
    }

    fn identity<'db>(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        match ty {
            Type::Never => Ok(TypeIdentity::Other(ty)),
            _ => Err(None),
        }
    }

    fn prepare_growth(
        &self,
        _request: TypeTransformationGrowth,
    ) -> Result<Option<GrowthPlan>, Self::Error> {
        Err(None)
    }
}

/// Samples active transformation-scope depth without constructing a transformer or changing scopes.
/// Returns `Some(0)` for an uninitialized transformer; `None` means the probe did not reach the
/// depth observation, so it does not establish whether any scope is active.
fn active_depth<'db>(
    db: &'db dyn Db,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    context: GenericContext<'db>,
    delta: u32,
) -> Option<usize> {
    visitor
        .transformer_cell(&TypeMapping::FreshenBoundTypeVars {
            generic_context: context,
            delta,
        })
        .get()
        .map(|transformer| {
            transformer
                .begin_visit_with(db, Type::Never, &ActiveDepth)
                .err()
                .flatten()
        })
        .unwrap_or(Some(0))
}

/// Observes transformation scopes already active when a retained freshening child starts.
pub(in crate::types) fn observe_mapping_child<'db>(
    db: &'db dyn Db,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    context: GenericContext<'db>,
    delta: u32,
) {
    if OBSERVER.get().is_some() {
        observe(Event::MappingChild {
            visitor: std::ptr::from_ref(visitor).addr(),
            active: active_depth(db, visitor, context, delta),
        });
    }
}

/// Observes scope retirement at the end of a freshening entry while its pooled visitor is alive.
/// This borrows the visitor and does not observe the later deallocation of the resource pool.
pub(in crate::types) struct MappingLifetime<'a, 'env, 'db> {
    db: &'db dyn Db,
    visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
    context: GenericContext<'db>,
    delta: u32,
    recording: bool,
}

impl fmt::Debug for MappingLifetime<'_, '_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MappingLifetime")
            .field("visitor", &std::ptr::from_ref(self.visitor).addr())
            .field("delta", &self.delta)
            .field("recording", &self.recording)
            .finish()
    }
}

impl<'a, 'env, 'db> MappingLifetime<'a, 'env, 'db> {
    /// Begins passive lifetime observation without allocating or taking ownership of the visitor.
    pub(in crate::types) fn new(
        db: &'db dyn Db,
        visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Self {
        let recording = OBSERVER.get().is_some();
        if recording {
            observe(Event::MappingEntered {
                visitor: std::ptr::from_ref(visitor).addr(),
            });
        }
        Self {
            db,
            visitor,
            context,
            delta,
            recording,
        }
    }
}

impl Drop for MappingLifetime<'_, '_, '_> {
    fn drop(&mut self) {
        if self.recording {
            observe(Event::MappingRetired {
                visitor: std::ptr::from_ref(self.visitor).addr(),
                active: active_depth(self.db, self.visitor, self.context, self.delta),
            });
        }
    }
}
