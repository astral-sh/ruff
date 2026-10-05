use std::cell::Cell;
use std::rc::Rc;

use salsa::plumbing::AsId;

use crate::Db;
use crate::types::ApplyTypeMappingVisitor;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::relation::{
    EquivalenceChecker, TypeRelation, TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::typevar::TypeVarSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct Direction {
    pub(in crate::types) builder: usize,
    pub(in crate::types) visitor: usize,
    pub(in crate::types) guard: Option<usize>,
    pub(in crate::types) remaining_work: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct Snapshot {
    pub(in crate::types) count: usize,
    pub(in crate::types) directions: [Option<Direction>; 2],
}

thread_local! {
    static DIRECTIONS: Cell<Snapshot> = const { Cell::new(Snapshot {
        count: 0,
        directions: [None; 2],
    }) };
    static CANCEL_AT: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(in crate::types) fn reset(cancel_at: Option<usize>) {
    DIRECTIONS.set(Snapshot {
        count: 0,
        directions: [None; 2],
    });
    CANCEL_AT.set(cancel_at);
}

pub(in crate::types) fn snapshot() -> Snapshot {
    DIRECTIONS.get()
}

pub(super) fn observe(
    db: &dyn Db,
    checker: &EquivalenceChecker<'_, '_, '_>,
    visitor: &ApplyTypeMappingVisitor<'_, '_>,
) {
    let mut snapshot = DIRECTIONS.get();
    let direction = Direction {
        builder: std::ptr::from_ref(checker.constraints).addr(),
        visitor: std::ptr::from_ref(visitor).addr(),
        guard: visitor
            .materialization_equivalence
            .get()
            .map(|guard| Rc::as_ptr(guard).addr()),
        remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
    };
    if let Some(slot) = snapshot.directions.get_mut(snapshot.count) {
        *slot = Some(direction);
    }
    snapshot.count += 1;
    DIRECTIONS.set(snapshot);
    if CANCEL_AT.get() == Some(snapshot.count) {
        CANCEL_AT.set(None);
        db.cancellation_token().cancel();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InvocationStage {
    Allocated,
    Preparing,
    ArgumentCheck,
    BinderCheck,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct InvocationEvent {
    pub(in crate::types) stage: InvocationStage,
    pub(in crate::types) builder: usize,
    pub(in crate::types) remaining_work: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct InvocationSnapshot {
    pub(in crate::types) count: usize,
    pub(in crate::types) events: [Option<InvocationEvent>; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct CheckerIdentity {
    pub(in crate::types) builder: usize,
    pub(in crate::types) environment: usize,
    pub(in crate::types) visitors: [usize; 4],
    pub(in crate::types) relation: TypeRelation,
    pub(in crate::types) typevars: TypeVarEvaluation,
    pub(in crate::types) inferable: Option<salsa::Id>,
    pub(in crate::types) given_is_original_never: bool,
    pub(in crate::types) context: Option<usize>,
    pub(in crate::types) expensive: bool,
}

impl CheckerIdentity {
    fn capture(checker: &TypeRelationChecker<'_, '_, '_>) -> Self {
        Self {
            builder: std::ptr::from_ref(checker.constraints).addr(),
            environment: std::ptr::from_ref(checker.env).addr(),
            visitors: [
                std::ptr::from_ref(checker.relation_visitor).addr(),
                std::ptr::from_ref(checker.disjointness_visitor).addr(),
                std::ptr::from_ref(checker.signature_relation_visitor).addr(),
                std::ptr::from_ref(checker.materialization_visitor).addr(),
            ],
            relation: checker.relation,
            typevars: checker.typevar_evaluation,
            inferable: match checker.inferable {
                TypeVarSet::None => None,
                TypeVarSet::Some(inferable) => Some(inferable.as_id()),
            },
            given_is_original_never: checker
                .given
                .has_same_identity(ConstraintSet::from_bool(checker.constraints, false)),
            context: checker
                .context_tree
                .as_ref()
                .map(|context| std::ptr::from_ref(context).addr()),
            expensive: checker.perform_expensive_checks,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct AssignabilityRoot {
    pub(in crate::types) identity: CheckerIdentity,
    pub(in crate::types) always: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct AssignabilityPair {
    pub(in crate::types) checker: usize,
    pub(in crate::types) identity: CheckerIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct AssignabilityResult {
    pub(in crate::types) always: bool,
    pub(in crate::types) terminal: Option<bool>,
    pub(in crate::types) original_terminal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct AssignabilitySnapshot {
    pub(in crate::types) root_count: usize,
    pub(in crate::types) roots: [Option<AssignabilityRoot>; 8],
    pub(in crate::types) pair_count: usize,
    pub(in crate::types) pairs: [Option<AssignabilityPair>; 64],
    pub(in crate::types) result_count: usize,
    pub(in crate::types) results: [Option<AssignabilityResult>; 8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ClassConditionPair {
    pub(in crate::types) checker: usize,
    pub(in crate::types) identity: CheckerIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ClassConditionResult {
    pub(in crate::types) builder: usize,
    pub(in crate::types) terminal: Option<bool>,
    pub(in crate::types) original_terminal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ClassConditionSnapshot {
    pub(in crate::types) root_count: usize,
    pub(in crate::types) roots: [Option<CheckerIdentity>; 8],
    pub(in crate::types) pair_count: usize,
    pub(in crate::types) pairs: [Option<ClassConditionPair>; 64],
    pub(in crate::types) result_count: usize,
    pub(in crate::types) results: [Option<ClassConditionResult>; 8],
}

thread_local! {
    static INVOCATIONS: Cell<InvocationSnapshot> = const { Cell::new(InvocationSnapshot {
        count: 0,
        events: [None; 32],
    }) };
    static INVOCATION_CANCEL_AT: Cell<Option<InvocationStage>> = const { Cell::new(None) };
    static ASSIGNABILITY: Cell<AssignabilitySnapshot> = const { Cell::new(AssignabilitySnapshot {
        root_count: 0,
        roots: [None; 8],
        pair_count: 0,
        pairs: [None; 64],
        result_count: 0,
        results: [None; 8],
    }) };
    static CLASS_CONDITIONS: Cell<ClassConditionSnapshot> = const { Cell::new(ClassConditionSnapshot {
        root_count: 0,
        roots: [None; 8],
        pair_count: 0,
        pairs: [None; 64],
        result_count: 0,
        results: [None; 8],
    }) };
}

pub(in crate::types) fn reset_invocations() {
    INVOCATIONS.set(InvocationSnapshot {
        count: 0,
        events: [None; 32],
    });
    INVOCATION_CANCEL_AT.set(None);
}

pub(in crate::types) fn set_invocation_cancel_at(stage: Option<InvocationStage>) {
    INVOCATION_CANCEL_AT.set(stage);
}

pub(in crate::types) fn invocation_snapshot() -> InvocationSnapshot {
    INVOCATIONS.get()
}

fn invocation_event(event: InvocationEvent) {
    let mut snapshot = INVOCATIONS.get();
    if let Some(slot) = snapshot.events.get_mut(snapshot.count) {
        *slot = Some(event);
    }
    snapshot.count += 1;
    INVOCATIONS.set(snapshot);
}

pub(in crate::types) fn observe_invocation(
    db: &dyn Db,
    builder: &ConstraintSetBuilder<'_>,
    stage: InvocationStage,
) {
    invocation_event(InvocationEvent {
        stage,
        builder: std::ptr::from_ref(builder).addr(),
        remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
    });
    if INVOCATION_CANCEL_AT.get() == Some(stage) {
        INVOCATION_CANCEL_AT.set(None);
        db.cancellation_token().cancel();
    }
}

pub(in crate::types) fn observe_invocation_allocation(builder: &ConstraintSetBuilder<'_>) {
    invocation_event(InvocationEvent {
        stage: InvocationStage::Allocated,
        builder: std::ptr::from_ref(builder).addr(),
        remaining_work: None,
    });
}

pub(in crate::types) fn reset_assignability() {
    ASSIGNABILITY.set(AssignabilitySnapshot {
        root_count: 0,
        roots: [None; 8],
        pair_count: 0,
        pairs: [None; 64],
        result_count: 0,
        results: [None; 8],
    });
}

pub(in crate::types) fn assignability_snapshot() -> AssignabilitySnapshot {
    ASSIGNABILITY.get()
}

pub(in crate::types) fn observe_assignability_root(
    checker: &TypeRelationChecker<'_, '_, '_>,
    always: bool,
) {
    let mut snapshot = ASSIGNABILITY.get();
    if let Some(slot) = snapshot.roots.get_mut(snapshot.root_count) {
        *slot = Some(AssignabilityRoot {
            identity: CheckerIdentity::capture(checker),
            always,
        });
    }
    snapshot.root_count += 1;
    ASSIGNABILITY.set(snapshot);
}

pub(in crate::types) fn observe_assignability_pair(checker: &TypeRelationChecker<'_, '_, '_>) {
    let mut snapshot = ASSIGNABILITY.get();
    if let Some(slot) = snapshot.pairs.get_mut(snapshot.pair_count) {
        *slot = Some(AssignabilityPair {
            checker: std::ptr::from_ref(checker).addr(),
            identity: CheckerIdentity::capture(checker),
        });
    }
    snapshot.pair_count += 1;
    ASSIGNABILITY.set(snapshot);
}

pub(in crate::types) fn observe_assignability_result<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    result: ConstraintSet<'db, 'c>,
    always: bool,
) {
    let terminal = if result.is_trivially_always_satisfied() {
        Some(true)
    } else if result.is_trivially_never_satisfied() {
        Some(false)
    } else {
        None
    };
    let mut snapshot = ASSIGNABILITY.get();
    if let Some(slot) = snapshot.results.get_mut(snapshot.result_count) {
        *slot = Some(AssignabilityResult {
            always,
            terminal,
            original_terminal: terminal.is_some_and(|terminal| {
                result.has_same_identity(ConstraintSet::from_bool(builder, terminal))
            }),
        });
    }
    snapshot.result_count += 1;
    ASSIGNABILITY.set(snapshot);
}

pub(in crate::types) fn reset_class_conditions() {
    CLASS_CONDITIONS.set(ClassConditionSnapshot {
        root_count: 0,
        roots: [None; 8],
        pair_count: 0,
        pairs: [None; 64],
        result_count: 0,
        results: [None; 8],
    });
}

pub(in crate::types) fn class_condition_snapshot() -> ClassConditionSnapshot {
    CLASS_CONDITIONS.get()
}

pub(super) fn observe_class_condition_root(checker: &TypeRelationChecker<'_, '_, '_>) {
    let mut snapshot = CLASS_CONDITIONS.get();
    if let Some(slot) = snapshot.roots.get_mut(snapshot.root_count) {
        *slot = Some(CheckerIdentity::capture(checker));
    }
    snapshot.root_count += 1;
    CLASS_CONDITIONS.set(snapshot);
}

pub(in crate::types::relation::source) fn observe_class_condition_pair(
    checker: &TypeRelationChecker<'_, '_, '_>,
) {
    let mut snapshot = CLASS_CONDITIONS.get();
    if let Some(slot) = snapshot.pairs.get_mut(snapshot.pair_count) {
        *slot = Some(ClassConditionPair {
            checker: std::ptr::from_ref(checker).addr(),
            identity: CheckerIdentity::capture(checker),
        });
    }
    snapshot.pair_count += 1;
    CLASS_CONDITIONS.set(snapshot);
}

pub(super) fn observe_class_condition_result<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    result: ConstraintSet<'db, 'c>,
) {
    let terminal = if result.is_trivially_always_satisfied() {
        Some(true)
    } else if result.is_trivially_never_satisfied() {
        Some(false)
    } else {
        None
    };
    let mut snapshot = CLASS_CONDITIONS.get();
    if let Some(slot) = snapshot.results.get_mut(snapshot.result_count) {
        *slot = Some(ClassConditionResult {
            builder: std::ptr::from_ref(builder).addr(),
            terminal,
            original_terminal: terminal.is_some_and(|terminal| {
                result.has_same_identity(ConstraintSet::from_bool(builder, terminal))
            }),
        });
    }
    snapshot.result_count += 1;
    CLASS_CONDITIONS.set(snapshot);
}
