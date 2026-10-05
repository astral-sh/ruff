//! Observes public-promotion admission and mapping-scope cleanup without changing production work.
//! A mapping entry can retire while its visitor remains in the source resource pool. The retirement
//! observation checks active scopes at that point; it does not observe backing-storage deallocation.

use std::cell::RefCell;
use std::fmt;

use salsa::attempt_probe::remaining_allowance_for_diagnostics;

use crate::Db;
use crate::types::constraints::control::GrowthPlan;
use crate::types::cyclic::{
    TypeIdentity, TypeTransformationControl, TypeTransformationGrowth, TypeTransformationWork,
};
use crate::types::{
    ApplyTypeMappingVisitor, KnownClass, PromotionKind, PromotionMode, Type, TypeMapping,
};

/// Identifies canonical-child requests and the public provider's final result transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Stage {
    ScalarRequest(KnownClass),
    SingletonUnion,
    FinalTransfer,
    Transferred,
}

/// Selects observation alone or cancellation when the watched canonical child enters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ChildAction {
    Observe,
    Cancel,
}

/// Captures entries, real suspension, and mapping retirement as separate events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    Stage(Stage),
    MappingEntered {
        visitor: usize,
        mode: PromotionMode,
    },
    Pending {
        live_mappings: usize,
    },
    ChildEntered {
        live_mappings: usize,
    },
    MappingRetired {
        visitor: usize,
        mode: PromotionMode,
        active: Option<usize>,
    },
}

#[derive(Clone, Debug, Default)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) events: Vec<Event>,
    pub(in crate::types) live_mappings: usize,
}

impl Snapshot {
    /// Counts requests or completions at one real provider boundary.
    pub(in crate::types) fn count(&self, stage: Stage) -> usize {
        self.events
            .iter()
            .filter(|event| **event == Event::Stage(stage))
            .count()
    }
}

struct Journal {
    database: usize,
    child: Option<salsa::DatabaseKeyIndex>,
    cancellation: Option<salsa::CancellationToken>,
    snapshot: Snapshot,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Limits observations and an optional cancellation request to one database and canonical child key.
#[derive(Debug)]
pub(in crate::types) struct Recording;

impl Recording {
    pub(in crate::types) fn start(
        db: &dyn Db,
        child: Option<salsa::DatabaseKeyIndex>,
        action: ChildAction,
    ) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal {
                database: std::ptr::from_ref(db.zalsa()).addr(),
                child,
                cancellation: match action {
                    ChildAction::Observe => None,
                    ChildAction::Cancel => Some(db.cancellation_token()),
                },
                snapshot: Snapshot::default(),
            });
        });
        Self
    }

    pub(in crate::types) fn snapshot(&self) -> Snapshot {
        JOURNAL.with_borrow(|journal| match journal {
            Some(journal) => journal.snapshot.clone(),
            None => Snapshot::default(),
        })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
}

/// Records a provider boundary only within the selected database's controlled attempt.
pub(in crate::types) fn stage(db: &dyn Db, stage: Stage) {
    if remaining_allowance_for_diagnostics(db).is_none() {
        return;
    }
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.database == std::ptr::from_ref(db.zalsa()).addr()
        {
            journal.snapshot.events.push(Event::Stage(stage));
        }
    });
}

/// Observes canonical execution entry and requests cancellation there when armed by the test.
/// Entry is not completion or cleanup; the runtime delivers cancellation at its next boundary.
pub(in crate::types) fn query_event(event: &salsa::EventKind) {
    let salsa::EventKind::WillExecute { database_key } = event else {
        return;
    };
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.child == Some(*database_key)
        {
            journal.snapshot.events.push(Event::ChildEntered {
                live_mappings: journal.snapshot.live_mappings,
            });
            if let Some(cancellation) = journal.cancellation.take() {
                cancellation.cancel();
            }
        }
    });
}

/// Records a `Pending` returned by the actual promotion future without introducing suspension.
pub(in crate::types) fn pending() {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.snapshot.events.push(Event::Pending {
                live_mappings: journal.snapshot.live_mappings,
            });
        }
    });
}

/// Refuses before a transformation scope is created while reporting the current active depth.
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
            _ => Err(None),
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

/// Checks transformation depth after the mapping entry's child retires, while the pooled visitor
/// remains available. This witness borrows that visitor and does not observe its eventual destruction.
pub(in crate::types) struct MappingLifetime<'a, 'env, 'db> {
    db: &'db dyn Db,
    visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
    mode: PromotionMode,
    recording: bool,
}

impl fmt::Debug for MappingLifetime<'_, '_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MappingLifetime")
            .field("visitor", &std::ptr::from_ref(self.visitor).addr())
            .field("mode", &self.mode)
            .field("recording", &self.recording)
            .finish()
    }
}

impl<'a, 'env, 'db> MappingLifetime<'a, 'env, 'db> {
    pub(in crate::types) fn new(
        db: &'db dyn Db,
        visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
        mode: PromotionMode,
    ) -> Self {
        let recording = JOURNAL.with_borrow_mut(|journal| {
            if let Some(journal) = journal
                && journal.database == std::ptr::from_ref(db.zalsa()).addr()
            {
                journal.snapshot.live_mappings += 1;
                journal.snapshot.events.push(Event::MappingEntered {
                    visitor: std::ptr::from_ref(visitor).addr(),
                    mode,
                });
                true
            } else {
                false
            }
        });
        Self {
            db,
            visitor,
            mode,
            recording,
        }
    }
}

impl Drop for MappingLifetime<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.recording {
            return;
        }
        let mapping = TypeMapping::Promote(self.mode, PromotionKind::Regular);
        let active = self
            .visitor
            .transformer_cell(&mapping)
            .get()
            .map(|transformer| {
                transformer
                    .begin_visit_with(self.db, Type::Never, &ActiveDepth)
                    .err()
                    .flatten()
            })
            .unwrap_or(Some(0));
        JOURNAL.with_borrow_mut(|journal| {
            if let Some(journal) = journal {
                journal.snapshot.live_mappings -= 1;
                journal.snapshot.events.push(Event::MappingRetired {
                    visitor: std::ptr::from_ref(self.visitor).addr(),
                    mode: self.mode,
                    active,
                });
            }
        });
    }
}
