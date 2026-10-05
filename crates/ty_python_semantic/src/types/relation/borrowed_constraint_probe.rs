//! Flat task storage with real constraint sets borrowing a stable external builder.
//!
//! The dependency chain is synthetic. Each task owns a real checker and private visitor scopes;
//! its parent consumes the child's exact constraint handle as an assumption in further checking.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker, TypeVarEvaluation,
};
use crate::db::tests::setup_db;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::cyclic::CycleDetectorVisit;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, Type, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

struct RootResources<'db> {
    constraints: ConstraintSetBuilder<'db>,
    env: ProgramEnvironment<'db>,
}

struct RelationInputs<'a, 'db, 'c> {
    env: &'a ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    int: Type<'db>,
    typevar: BoundTypeVarInstance<'db>,
    caller_set: Option<ConstraintSet<'db, 'c>>,
}

#[derive(Default)]
struct TaskStats {
    starts: Cell<usize>,
    polls: Cell<usize>,
    resumed: Cell<usize>,
    completed: Cell<bool>,
    dropped: Cell<bool>,
    expected_cached: Cell<usize>,
}

struct RelationSlots<'db, 'c> {
    values: Vec<Cell<Option<ConstraintSet<'db, 'c>>>>,
    requests: RefCell<VecDeque<(usize, usize)>>,
    stats: Vec<TaskStats>,
    poll_depth: Cell<usize>,
    max_poll_depth: Cell<usize>,
}

impl RelationSlots<'_, '_> {
    fn new(length: usize) -> Self {
        Self {
            values: (0..length).map(|_| Cell::new(None)).collect(),
            requests: RefCell::default(),
            stats: (0..length).map(|_| TaskStats::default()).collect(),
            poll_depth: Cell::new(0),
            max_poll_depth: Cell::new(0),
        }
    }
}

struct RelationDemand<'a, 'db, 'c> {
    slots: &'a RelationSlots<'db, 'c>,
    parent: usize,
    child: usize,
    registered: bool,
}

impl<'db, 'c> Future for RelationDemand<'_, 'db, 'c> {
    type Output = ConstraintSet<'db, 'c>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        assert_eq!(self.slots.poll_depth.get(), 1);
        if !self.registered {
            self.slots
                .requests
                .borrow_mut()
                .push_back((self.parent, self.child));
            self.registered = true;
            return Poll::Pending;
        }
        self.slots.values[self.child]
            .get()
            .map_or(Poll::Pending, Poll::Ready)
    }
}

struct VisitAudit<'a, 'db, 'c> {
    visitor: &'a HasRelationToVisitor<'db, 'c>,
    stats: &'a TaskStats,
}

impl Drop for VisitAudit<'_, '_, '_> {
    fn drop(&mut self) {
        assert_eq!(
            self.visitor.ownership_probe_counts(),
            (0, self.stats.expected_cached.get())
        );
        self.stats.dropped.set(true);
    }
}

async fn relation_task<'db, 'c>(
    db: &'db dyn Db,
    inputs: &RelationInputs<'_, 'db, 'c>,
    slots: &RelationSlots<'db, 'c>,
    index: usize,
) -> ConstraintSet<'db, 'c> {
    let RelationInputs {
        env,
        constraints,
        int,
        typevar,
        caller_set,
    } = *inputs;
    let stats = &slots.stats[index];
    stats.starts.set(stats.starts.get() + 1);
    assert_eq!(slots.poll_depth.get(), 1);
    let relation_visitor = HasRelationToVisitor::default(constraints);
    let disjointness_visitor = IsDisjointVisitor::default(constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mapping_visitor = ApplyTypeMappingVisitor::new(env);
    let _audit = VisitAudit {
        visitor: &relation_visitor,
        stats,
    };
    let mut checker = TypeRelationChecker::new(
        env,
        TypeRelation::Subtyping,
        constraints,
        TypeVarSet::from_typevars(db, [typevar]),
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    );
    checker.typevar_evaluation = TypeVarEvaluation::Lazy;
    let key = (
        int,
        Type::TypeVar(typevar),
        checker.relation,
        checker.typevar_evaluation,
    );
    let CycleDetectorVisit::Pending(visit) = relation_visitor.try_begin_visit(db, key, |_| true)
    else {
        panic!("each task begins with its own empty visitor");
    };

    let result = if index + 1 < slots.values.len() {
        let child = RelationDemand {
            slots,
            parent: index,
            child: index + 1,
            registered: false,
        }
        .await;
        assert_eq!(relation_visitor.ownership_probe_counts(), (1, 0));
        assert!(child.ownership_probe_same_set(
            slots.values[index + 1].get().expect("completed child slot")
        ));
        assert!(!child.is_trivially_always_satisfied());
        assert!(!child.is_trivially_never_satisfied());
        checker.given = child;
        checker.relation = TypeRelation::SubtypingAssuming;
        let justified = checker.check_type_pair(db, int, Type::TypeVar(typevar));
        assert!(justified.is_always_satisfied(db, env));
        assert!(checker.given.ownership_probe_same_set(child));
        stats.resumed.set(stats.resumed.get() + 1);
        child
    } else if let Some(caller_set) = caller_set {
        checker.given = caller_set;
        checker.relation = TypeRelation::SubtypingAssuming;
        assert!(
            checker
                .check_type_pair(db, int, Type::TypeVar(typevar))
                .is_always_satisfied(db, env)
        );
        caller_set
    } else {
        checker.check_type_pair(db, int, Type::TypeVar(typevar))
    };

    assert!(!result.is_trivially_always_satisfied());
    assert!(!result.is_trivially_never_satisfied());
    let result = visit.finish(result);
    stats.expected_cached.set(1);
    stats.completed.set(true);
    result
}

type RelationTask<'task, 'db, 'c> = Pin<Box<dyn Future<Output = ConstraintSet<'db, 'c>> + 'task>>;

struct RunResult<'db, 'c> {
    result: Option<ConstraintSet<'db, 'c>>,
    polls: usize,
    allocations: usize,
    max_poll_depth: usize,
}

#[derive(Clone, Copy, Debug)]
enum DropOrder {
    ParentsFirst,
    ChildrenFirst,
}

fn run_chain<'db, 'c>(
    db: &'db dyn Db,
    inputs: &RelationInputs<'_, 'db, 'c>,
    length: usize,
    cancel_after_polls: Option<usize>,
    drop_order: DropOrder,
) -> RunResult<'db, 'c> {
    let slots = RelationSlots::new(length);
    let make_task = |index| relation_task(db, inputs, &slots, index);
    // Resource owners and typed result slots exist outside the task-storage vector.
    let mut tasks: Vec<Option<RelationTask<'_, 'db, 'c>>> = (0..length).map(|_| None).collect();
    tasks[0] = Some(Box::pin(make_task(0)));
    let mut allocations = 1;
    let mut ready = VecDeque::from([0]);
    let mut waiting_parent = vec![None; length];
    let mut polls = 0;
    let mut cx = Context::from_waker(Waker::noop());

    while cancel_after_polls != Some(polls) {
        let Some(index) = ready.pop_front() else {
            break;
        };
        let stats = &slots.stats[index];
        let Some(task) = tasks[index].as_mut() else {
            panic!("ready task is stored independently");
        };
        stats.polls.set(stats.polls.get() + 1);
        polls += 1;
        let depth = slots.poll_depth.get() + 1;
        slots.poll_depth.set(depth);
        slots
            .max_poll_depth
            .set(slots.max_poll_depth.get().max(depth));
        let result = task.as_mut().poll(&mut cx);
        slots.poll_depth.set(depth - 1);
        if let Poll::Ready(result) = result {
            slots.values[index].set(Some(result));
            tasks[index] = None;
            if let Some(parent) = waiting_parent[index] {
                ready.push_back(parent);
            }
        }
        // Drain request metadata only after polling ends; no metadata borrow crosses a poll.
        loop {
            let request = slots.requests.borrow_mut().pop_front();
            let Some((parent, child)) = request else {
                break;
            };
            assert!(tasks[child].is_none());
            assert!(waiting_parent[child].replace(parent).is_none());
            tasks[child] = Some(Box::pin(make_task(child)));
            allocations += 1;
            ready.push_back(child);
        }
    }
    let result = slots.values[0].get();
    if let Some(result) = result {
        assert_eq!(polls, 2 * length - 1);
        for (index, stats) in slots.stats.iter().enumerate() {
            assert_eq!(stats.starts.get(), 1);
            assert_eq!(stats.polls.get(), if index + 1 == length { 1 } else { 2 });
            assert_eq!(stats.resumed.get(), usize::from(index + 1 < length));
            assert!(stats.completed.get());
            assert!(stats.dropped.get());
            assert!(
                slots.values[index]
                    .get()
                    .expect("completed result")
                    .ownership_probe_same_set(result)
            );
        }
    }

    match drop_order {
        DropOrder::ParentsFirst => {
            for task in &mut tasks {
                *task = None;
            }
        }
        DropOrder::ChildrenFirst => {
            for task in tasks.iter_mut().rev() {
                *task = None;
            }
        }
    }
    for stats in &slots.stats {
        assert_eq!(stats.dropped.get(), stats.starts.get() > 0);
        if !stats.completed.get() {
            assert_eq!(stats.expected_cached.get(), 0);
        }
    }
    // The returned handle borrows the external builder, not slots or task storage.
    RunResult {
        result,
        polls,
        allocations,
        max_poll_depth: slots.max_poll_depth.get(),
    }
}

#[test]
fn borrowed_constraint_future_flat_tasks() {
    let db = setup_db();
    let resources = Box::new(RootResources {
        constraints: ConstraintSetBuilder::new(),
        env: db.program_environment(),
    });
    let int = KnownClass::Int.to_instance(&db, &resources.env);
    let typevar = BoundTypeVarInstance::synthetic(
        &db,
        &resources.env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let inputs = RelationInputs {
        env: &resources.env,
        constraints: &resources.constraints,
        int,
        typevar,
        caller_set: None,
    };
    for length in [1, 2, 64, 1024] {
        let run = run_chain(&db, &inputs, length, None, DropOrder::ParentsFirst);
        assert!(run.result.is_some());
        assert_eq!(run.allocations, length);
        assert_eq!(run.max_poll_depth, 1);
        let result = run.result.expect("completed root result");
        assert!(!result.is_always_satisfied(&db, &resources.env));
        assert!(!result.is_never_satisfied(&db, &resources.env));
    }

    for length in [2, 8] {
        for cancel_at in 0..(2 * length - 1) {
            for order in [DropOrder::ParentsFirst, DropOrder::ChildrenFirst] {
                let run = run_chain(&db, &inputs, length, Some(cancel_at), order);
                assert!(run.result.is_none());
                assert_eq!(run.polls, cancel_at);
            }
        }
    }
    for order in [DropOrder::ParentsFirst, DropOrder::ChildrenFirst] {
        let run = run_chain(&db, &inputs, 1024, Some(1023), order);
        assert_eq!(run.polls, 1023);
        assert_eq!(run.allocations, 1024);
        assert_eq!(run.max_poll_depth, 1);
        assert!(run.result.is_none());
    }
}

#[test]
fn borrowed_constraint_future_keeps_caller_set() {
    let db = setup_db();
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let int = KnownClass::Int.to_instance(&db, &env);
    let typevar = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let original =
        ConstraintSet::constrain_typevar_lower_bound(&db, &env, &constraints, typevar, int);
    let inputs = RelationInputs {
        env: &env,
        constraints: &constraints,
        int,
        typevar,
        caller_set: Some(original),
    };
    let run = run_chain(&db, &inputs, 32, None, DropOrder::ChildrenFirst);
    assert!(
        run.result
            .expect("completed root")
            .ownership_probe_same_set(original)
    );
    assert_eq!(run.polls, 63);
    assert_eq!(run.allocations, 32);
    assert_eq!(run.max_poll_depth, 1);
    assert!(!original.is_always_satisfied(&db, &env));
    assert!(!original.is_never_satisfied(&db, &env));
}
