use std::cell::RefCell;
#[cfg(not(feature = "shuttle"))]
use std::sync::mpsc;
#[cfg(not(feature = "shuttle"))]
use std::time::Duration;

use super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::observation::{self, Event};
#[cfg(not(feature = "shuttle"))]
use crate::attempt_probe::paired_test_support::run_pair;
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::sync::ClaimResult;
use crate::function::{Configuration, IngredientImpl, Memo, Reentrancy};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

pub(in crate::function::execute::execution_run) mod cleanup;
#[cfg(not(feature = "shuttle"))]
mod long_chain;

#[crate::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
    #[returns(copy)]
    value: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Count(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Seed(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Scalar,
    Wrapped,
    Seed,
}

trait Output {
    const KIND: Kind;
    fn new(value: u32) -> Self;
    fn value(&self) -> u32;
}

impl Output for u32 {
    const KIND: Kind = Kind::Scalar;
    fn new(value: u32) -> Self {
        value
    }
    fn value(&self) -> u32 {
        *self
    }
}

impl Output for Count {
    const KIND: Kind = Kind::Wrapped;
    fn new(value: u32) -> Self {
        Self(value)
    }
    fn value(&self) -> u32 {
        self.0
    }
}

impl Output for Seed {
    const KIND: Kind = Kind::Seed;
    fn new(value: u32) -> Self {
        Self(value)
    }
    fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Chain,
    SelfCycle,
    MutualCycle,
    Fallback,
}

struct AfterChild {
    limit: Option<u32>,
}

impl AfterChild {
    fn resume(self, value: u32) -> u32 {
        let value = value + 1;
        self.limit.map_or(value, |limit| value.min(limit))
    }
}

enum BodyStep {
    Value(u32),
    Child(Node, AfterChild),
}

// Both consumers use these reads and this continuation, so awaiting a child cannot duplicate
// an input read or change the order of dependencies recorded by the authored fixture body.
fn body_step(db: &dyn Database, node: Node, mode: Mode, kind: Kind) -> BodyStep {
    if kind == Kind::Seed {
        return BodyStep::Value(node.value(db));
    }
    let after = AfterChild {
        limit: match mode {
            Mode::Chain | Mode::Fallback => None,
            Mode::SelfCycle => Some(3),
            Mode::MutualCycle => Some(if kind == Kind::Scalar { 3 } else { 4 }),
        },
    };
    match node.next(db) {
        Some(next) => BodyStep::Child(next, after),
        None if matches!(mode, Mode::Chain) => BodyStep::Value(node.value(db) % 2),
        None => BodyStep::Value(after.resume(0)),
    }
}

fn ordinary_body(
    db: &dyn Database,
    node: Node,
    mode: Mode,
    kind: Kind,
    child: impl FnOnce(Node) -> u32,
) -> u32 {
    match body_step(db, node, mode, kind) {
        BodyStep::Value(value) => value,
        BodyStep::Child(next, after) => after.resume(child(next)),
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn scalar(db: &dyn Database, node: Node) -> u32 {
    ordinary_body(db, node, Mode::Chain, Kind::Scalar, |next| {
        wrapped(db, next).0
    })
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn wrapped(db: &dyn Database, node: Node) -> Count {
    Count(ordinary_body(
        db,
        node,
        Mode::Chain,
        Kind::Wrapped,
        |next| scalar(db, next),
    ))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn seed(db: &dyn Database, node: Node) -> Seed {
    Seed(ordinary_body(db, node, Mode::Chain, Kind::Seed, |_| {
        panic!("a seed has no query child")
    }))
}

fn initial(db: &dyn Database, _id: Id, node: Node) -> u32 {
    seed(db, node).0
}

fn wrapped_initial(db: &dyn Database, id: Id, node: Node) -> Count {
    Count(initial(db, id, node))
}

fn recover(db: &dyn Database, _cycle: &Cycle<'_>, _old: &u32, value: u32, node: Node) -> u32 {
    let _ = seed(db, node);
    value
}

fn wrapped_recover(
    db: &dyn Database,
    _cycle: &Cycle<'_>,
    _old: &Count,
    value: Count,
    node: Node,
) -> Count {
    let _ = seed(db, node);
    value
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn self_cycle(db: &dyn Database, node: Node) -> u32 {
    ordinary_body(db, node, Mode::SelfCycle, Kind::Scalar, |next| {
        self_cycle(db, next)
    })
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn mutual_scalar(db: &dyn Database, node: Node) -> u32 {
    ordinary_body(db, node, Mode::MutualCycle, Kind::Scalar, |next| {
        mutual_wrapped(db, next).0
    })
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = wrapped_initial, cycle_fn = wrapped_recover)]
fn mutual_wrapped(db: &dyn Database, node: Node) -> Count {
    Count(ordinary_body(
        db,
        node,
        Mode::MutualCycle,
        Kind::Wrapped,
        |next| mutual_scalar(db, next),
    ))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_result = initial)]
fn fallback(db: &dyn Database, node: Node) -> u32 {
    ordinary_body(db, node, Mode::Fallback, Kind::Scalar, |next| {
        fallback(db, next)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Body,
    Initial,
    Recovery,
}

#[derive(Clone, Copy, Debug)]
struct Visit {
    phase: Phase,
    kind: Kind,
    node: Id,
    caller: DatabaseKeyIndex,
    depth: usize,
}

#[derive(Default)]
struct Script {
    visits: RefCell<Vec<Visit>>,
    refuse_initial: bool,
}

impl Script {
    fn visit(&self, db: &dyn Database, phase: Phase, kind: Kind, node: Node) -> Visit {
        let visit = Visit {
            phase,
            kind,
            node: node.as_id(),
            caller: db
                .zalsa_local()
                .active_query()
                .expect("callback has a caller")
                .0,
            depth: attempt_probe::stack_depths().0,
        };
        self.visits.borrow_mut().push(visit);
        visit
    }

    fn bodies(&self) -> Vec<(Kind, Id)> {
        self.visits
            .borrow()
            .iter()
            .filter_map(|visit| (visit.phase == Phase::Body).then_some((visit.kind, visit.node)))
            .collect()
    }
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

struct Providers<'a, 'db, A: Configuration, B: Configuration, S: Configuration> {
    scalar: Route<'db, A>,
    wrapped: Route<'db, B>,
    seed: Route<'db, S>,
    mode: Mode,
    script: &'a Script,
}

impl<'run, 'db: 'run, A, B, S, C> ExecutableRouteProvider<'run, 'db, C>
    for Providers<'run, 'db, A, B, S>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node>,
    S: Configuration<DbView = dyn Database, Input<'db> = Node>,
    C: Configuration<DbView = dyn Database, Input<'db> = Node>,
    A::Output<'db>: Output,
    B::Output<'db>: Output,
    S::Output<'db>: Output,
    C::Output<'db>: Output,
{
    // Node conversion is handle-only; u32, Count and Seed equality compare one u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        let kind = <C::Output<'db> as Output>::KIND;
        let visit = self.script.visit(db, Phase::Body, kind, node);
        let expected = match kind {
            Kind::Scalar => self.scalar.database_key(node.as_id()),
            Kind::Wrapped => self.wrapped.database_key(node.as_id()),
            Kind::Seed => self.seed.database_key(node.as_id()),
        };
        assert_eq!(visit.caller, expected);
        let value = match body_step(db, node, self.mode, kind) {
            BodyStep::Value(value) => value,
            BodyStep::Child(next, after) => {
                // A semantic child and a tracked fetch suspend under the same query frame.
                context.endpoint().demand(|| async { Ok(()) })?.await?;
                let value = if kind == Kind::Scalar
                    && matches!(self.mode, Mode::Chain | Mode::MutualCycle)
                {
                    context
                        .fetch_ref(&self.wrapped, next.as_id())?
                        .await?
                        .value()
                } else {
                    context
                        .fetch_ref(&self.scalar, next.as_id())?
                        .await?
                        .value()
                };
                after.resume(value)
            }
        };
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(visit.caller)
        );
        assert_eq!(attempt_probe::stack_depths().0, visit.depth);
        Ok(<C::Output<'db> as Output>::new(value))
    }

    async fn initial(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        _id: Id,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        assert!(!matches!(self.mode, Mode::Chain));
        let visit = self
            .script
            .visit(db, Phase::Initial, <C::Output<'db> as Output>::KIND, node);
        if self.script.refuse_initial {
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        let value = context.fetch_ref(&self.seed, node.as_id())?.await?.value();
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(visit.caller)
        );
        assert_eq!(attempt_probe::stack_depths().0, visit.depth);
        Ok(<C::Output<'db> as Output>::new(value))
    }

    async fn recover<'call>(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call C::Output<'db>,
        value: C::Output<'db>,
        node: Node,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        let visit = self
            .script
            .visit(db, Phase::Recovery, <C::Output<'db> as Output>::KIND, node);
        let _ = context.fetch_ref(&self.seed, node.as_id())?.await?;
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(visit.caller)
        );
        assert_eq!(attempt_probe::stack_depths().0, visit.depth);
        Ok(value)
    }
}

struct Fetched<'db> {
    scalar: &'db u32,
    wrapped: Option<&'db Count>,
}

type FetchAttempt<'db> = Result<AttemptOutcome<RunResult<Fetched<'db>>>, attempt_probe::StartError>;

fn try_fetch<'db, A, B>(
    db: &'db DatabaseImpl,
    ingredients: (&'db IngredientImpl<A>, &'db IngredientImpl<B>),
    scalar_node: Node,
    wrapped_node: Option<Node>,
    wrapped_first: bool,
    mode: Mode,
    script: &Script,
) -> FetchAttempt<'db>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = u32>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = Count>,
{
    try_with_attempt(db, 100_000, || {
        fetch_in_attempt(
            db,
            ingredients,
            scalar_node,
            wrapped_node,
            wrapped_first,
            mode,
            script,
        )
    })
}

fn fetch_in_attempt<'db, A, B>(
    db: &'db DatabaseImpl,
    ingredients: (&'db IngredientImpl<A>, &'db IngredientImpl<B>),
    scalar_node: Node,
    wrapped_node: Option<Node>,
    wrapped_first: bool,
    mode: Mode,
    script: &Script,
) -> RunResult<Fetched<'db>>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = u32>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = Count>,
{
    let db: &dyn Database = db;
    let admission = Admission;
    let mut registry = RegistryBuilder::new(db, &admission)?;
    let scalar = registry.reserve(db, ingredients.0)?;
    let wrapped = registry.reserve(db, ingredients.1)?;
    let seed = registry.reserve(db, seed::fn_ingredient_(db, db.zalsa()))?;
    let providers = Providers {
        scalar: scalar.clone(),
        wrapped: wrapped.clone(),
        seed: seed.clone(),
        mode,
        script,
    };
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    let wrong_binding = registry.provider(&providers)?;
    registry.bind_executable(&scalar, &binding)?;
    registry.bind_executable(&wrapped, &binding)?;
    registry.bind_executable(&seed, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let wrong = endpoint.provider(wrong_binding)?;
        assert!(matches!(
            wrong.fetch_ref(&scalar, scalar_node.as_id()),
            Err(RunError::Contract(
                "query route has a different provider binding"
            ))
        ));
        let context = endpoint.provider(binding)?;
        let mut other = None;
        if wrapped_first {
            let node = wrapped_node.expect("wrapped entry has a node");
            let first = context.fetch_ref(&wrapped, node.as_id())?.await?;
            let visits = script.visits.borrow().len();
            let second = context.fetch_ref(&wrapped, node.as_id())?.await?;
            assert!(std::ptr::eq(first, second));
            assert_eq!(script.visits.borrow().len(), visits);
            other = Some(first);
        }
        let first = context.fetch_ref(&scalar, scalar_node.as_id())?.await?;
        let visits = script.visits.borrow().len();
        let second = context.fetch_ref(&scalar, scalar_node.as_id())?.await?;
        assert!(std::ptr::eq(first, second));
        assert_eq!(script.visits.borrow().len(), visits);
        if !wrapped_first && let Some(node) = wrapped_node {
            other = Some(context.fetch_ref(&wrapped, node.as_id())?.await?);
        }
        Ok(Fetched {
            scalar: first,
            wrapped: other,
        })
    })
}

fn fetch<'db, A, B>(
    db: &'db DatabaseImpl,
    ingredients: (&'db IngredientImpl<A>, &'db IngredientImpl<B>),
    scalar_node: Node,
    wrapped_node: Option<Node>,
    wrapped_first: bool,
    mode: Mode,
    script: &Script,
) -> Fetched<'db>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = u32>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = Count>,
{
    let result = try_fetch(
        db,
        ingredients,
        scalar_node,
        wrapped_node,
        wrapped_first,
        mode,
        script,
    );
    match result {
        Ok(AttemptOutcome::Complete(Ok(value))) => value,
        _ => panic!("typed fetch did not complete"),
    }
}

fn memo<'db, C: Configuration>(
    db: &'db DatabaseImpl,
    ingredient: &IngredientImpl<C>,
    node: Node,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            node.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), node.as_id()),
        )
        .expect("query has a memo")
}

fn assert_idle<C: Configuration>(db: &DatabaseImpl, ingredient: &IngredientImpl<C>, node: Node) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        node.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim.abort(),
        _ => panic!("fetch retained its claim"),
    }
}

fn chain() -> (DatabaseImpl, [Node; 3]) {
    let db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, 4);
    let middle = Node::new(&db, Some(leaf), 0);
    let root = Node::new(&db, Some(middle), 0);
    (db, [root, middle, leaf])
}

fn fetch_chain<'db>(db: &'db DatabaseImpl, root: Node, script: &Script) -> Fetched<'db> {
    fetch(
        db,
        (
            scalar::fn_ingredient_(db, db.zalsa()),
            wrapped::fn_ingredient_(db, db.zalsa()),
        ),
        root,
        None,
        false,
        Mode::Chain,
        script,
    )
}

#[test]
fn cold_and_hot_fetch_retain_typed_values_and_record_child_reads() {
    let (db, [root, middle, leaf]) = chain();
    let script = Script::default();
    let (first, observed) = observation::collect(|| fetch_chain(&db, root, &script));
    assert_eq!(*first.scalar, 2);
    assert_eq!(
        script.bodies(),
        [
            (Kind::Scalar, root.as_id()),
            (Kind::Wrapped, middle.as_id()),
            (Kind::Scalar, leaf.as_id())
        ]
    );
    assert_eq!(observed.max_active_polls, 1);
    let scalar_ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let wrapped_ingredient = wrapped::fn_ingredient_(&db, db.zalsa());
    assert!(std::ptr::eq(
        first.scalar,
        memo(&db, scalar_ingredient, root).value().unwrap()
    ));
    assert_eq!(
        memo(&db, scalar_ingredient, root)
            .header
            .origin()
            .inputs()
            .filter(|key| *key == wrapped_ingredient.database_key_index(middle.as_id()))
            .count(),
        1
    );
    assert_eq!(
        memo(&db, wrapped_ingredient, middle)
            .header
            .origin()
            .inputs()
            .filter(|key| *key == scalar_ingredient.database_key_index(leaf.as_id()))
            .count(),
        1
    );
    let (again, hot) = observation::collect(|| fetch_chain(&db, root, &script));
    assert!(std::ptr::eq(first.scalar, again.scalar));
    assert!(hot.events.is_empty());
    assert_eq!(script.bodies().len(), 3);
    let (ordinary, [ordinary_root, _, _]) = chain();
    assert_eq!(scalar(&ordinary, ordinary_root), *first.scalar);
    for node in [root, leaf] {
        assert_idle(&db, scalar_ingredient, node);
    }
    assert_idle(&db, wrapped_ingredient, middle);
}

#[test]
fn edited_fetch_backdates_equal_leaf_and_reexecutes_changed_ancestors() {
    let (mut db, [root, middle, leaf]) = chain();
    assert_eq!(*fetch_chain(&db, root, &Script::default()).scalar, 2);
    let old_revision = db.zalsa().current_revision();
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let old_root = std::ptr::from_ref(memo(&db, ingredient, root)).addr();
    let old_leaf = std::ptr::from_ref(memo(&db, ingredient, leaf)).addr();
    let old_changed = memo(&db, ingredient, leaf).header.revisions.changed_at;
    leaf.set_value(&mut db).to(6);
    let script = Script::default();
    let (result, observed) = observation::collect(|| fetch_chain(&db, root, &script));
    assert_eq!(*result.scalar, 2);
    assert_eq!(script.bodies(), [(Kind::Scalar, leaf.as_id())]);
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        std::ptr::from_ref(memo(&db, ingredient, root)).addr(),
        old_root
    );
    assert_eq!(
        memo(&db, ingredient, leaf).header.revisions.changed_at,
        old_changed
    );
    assert_eq!(
        memo(&db, ingredient, root).header.verified_at.load(),
        db.zalsa().current_revision()
    );
    let leaf_key = ingredient.database_key_index(leaf.as_id());
    let claims = observed
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Claim {
                key,
                serial,
                operation,
            } if *key == leaf_key => Some((*serial, *operation)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(serial, operation)] = claims[..] else {
        panic!("leaf must retain one claim")
    };
    assert!(observed.events.contains(&Event::Execute {
        key: leaf_key,
        serial,
        operation,
        old_memo: Some(old_leaf)
    }));
    leaf.set_value(&mut db).to(7);
    let script = Script::default();
    assert_eq!(*fetch_chain(&db, root, &script).scalar, 3);
    let bodies = script.bodies();
    assert_eq!(bodies.len(), 3);
    for body in [
        (Kind::Scalar, root.as_id()),
        (Kind::Wrapped, middle.as_id()),
        (Kind::Scalar, leaf.as_id()),
    ] {
        assert!(bodies.contains(&body));
    }
    assert!(
        memo(&db, scalar::fn_ingredient_(&db, db.zalsa()), root)
            .header
            .revisions
            .changed_at
            > old_revision
    );
    let (mut ordinary, [ordinary_root, _, ordinary_leaf]) = chain();
    assert_eq!(scalar(&ordinary, ordinary_root), 2);
    ordinary_leaf.set_value(&mut ordinary).to(6);
    assert_eq!(scalar(&ordinary, ordinary_root), 2);
    ordinary_leaf.set_value(&mut ordinary).to(7);
    assert_eq!(scalar(&ordinary, ordinary_root), 3);
    assert_idle(&db, scalar::fn_ingredient_(&db, db.zalsa()), root);
    assert_idle(&db, wrapped::fn_ingredient_(&db, db.zalsa()), middle);
}

fn cycle_nodes(two: bool, fallback_values: bool) -> (DatabaseImpl, Node, Node) {
    let mut db = DatabaseImpl::default();
    let first = Node::new(&db, None, if fallback_values { 10 } else { 0 });
    let second = if two {
        Node::new(&db, Some(first), if fallback_values { 20 } else { 1 })
    } else {
        first
    };
    first.set_next(&mut db).to(Some(second));
    (db, first, second)
}

fn assert_query_callbacks(script: &Script) {
    let visits = script.visits.borrow();
    assert!(visits.iter().any(|visit| visit.phase == Phase::Initial));
    assert!(visits.iter().any(|visit| visit.phase == Phase::Recovery));
    assert!(
        visits
            .iter()
            .any(|visit| visit.phase == Phase::Body && visit.kind == Kind::Seed)
    );
}

#[test]
fn productive_self_fetch_queries_seed_and_recovery_on_the_same_stack() {
    let (db, node, _) = cycle_nodes(false, false);
    let ingredient = self_cycle::fn_ingredient_(&db, db.zalsa());
    let script = Script::default();
    let (result, observed) = observation::collect(|| {
        fetch(
            &db,
            (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
            node,
            None,
            false,
            Mode::SelfCycle,
            &script,
        )
    });
    assert_eq!(*result.scalar, 3);
    assert_query_callbacks(&script);
    assert!(
        script
            .bodies()
            .iter()
            .filter(|(kind, _)| *kind == Kind::Scalar)
            .count()
            > 1
    );
    assert_eq!(observed.max_active_polls, 1);
    assert!(!memo(&db, ingredient, node).header.may_be_provisional());
    assert!(
        memo(&db, ingredient, node).header.origin().inputs().any(
            |key| key == seed::fn_ingredient_(&db, db.zalsa()).database_key_index(node.as_id())
        )
    );
    let (ordinary, ordinary_node, _) = cycle_nodes(false, false);
    assert_eq!(self_cycle(&ordinary, ordinary_node), *result.scalar);
    assert_idle(&db, ingredient, node);
}

#[test]
fn mutual_typed_fetch_converges_from_either_cold_entry() {
    for wrapped_first in [false, true] {
        let (db, first, second) = cycle_nodes(true, false);
        let scalar_ingredient = mutual_scalar::fn_ingredient_(&db, db.zalsa());
        let wrapped_ingredient = mutual_wrapped::fn_ingredient_(&db, db.zalsa());
        let script = Script::default();
        let (result, observed) = observation::collect(|| {
            fetch(
                &db,
                (scalar_ingredient, wrapped_ingredient),
                first,
                Some(second),
                wrapped_first,
                Mode::MutualCycle,
                &script,
            )
        });
        assert_eq!(*result.scalar, 3);
        assert_eq!(result.wrapped, Some(&Count(4)));
        assert_query_callbacks(&script);
        assert_eq!(observed.max_active_polls, 1);
        // Reentering A from B (or B from A) invokes initial under the existing caller.
        assert!(
            script
                .visits
                .borrow()
                .iter()
                .any(|visit| visit.phase == Phase::Initial
                    && visit.caller
                        == if visit.kind == Kind::Scalar {
                            wrapped_ingredient.database_key_index(second.as_id())
                        } else {
                            scalar_ingredient.database_key_index(first.as_id())
                        })
        );
        let (ordinary, ordinary_first, ordinary_second) = cycle_nodes(true, false);
        if wrapped_first {
            assert_eq!(mutual_wrapped(&ordinary, ordinary_second), Count(4));
        }
        assert_eq!(mutual_scalar(&ordinary, ordinary_first), *result.scalar);
        assert_eq!(
            mutual_wrapped(&ordinary, ordinary_second),
            *result.wrapped.unwrap()
        );
        assert_eq!(
            memo(&db, scalar_ingredient, first)
                .header
                .may_be_provisional(),
            memo(
                &ordinary,
                mutual_scalar::fn_ingredient_(&ordinary, ordinary.zalsa()),
                ordinary_first
            )
            .header
            .may_be_provisional()
        );
        assert_eq!(
            memo(&db, wrapped_ingredient, second)
                .header
                .may_be_provisional(),
            memo(
                &ordinary,
                mutual_wrapped::fn_ingredient_(&ordinary, ordinary.zalsa()),
                ordinary_second
            )
            .header
            .may_be_provisional()
        );
        assert_idle(&db, scalar_ingredient, first);
        assert_idle(&db, wrapped_ingredient, second);
    }
}

#[test]
fn immediate_fallback_fetch_preserves_self_and_participant_results() {
    for two in [false, true] {
        for reverse in [false, true] {
            if reverse && !two {
                continue;
            }
            let (db, first, second) = cycle_nodes(two, true);
            let (first, second) = if reverse {
                (second, first)
            } else {
                (first, second)
            };
            let expected = if reverse {
                [20, 10]
            } else if two {
                [10, 20]
            } else {
                [10, 10]
            };
            let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
            let script = Script::default();
            let result = fetch(
                &db,
                (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                first,
                None,
                false,
                Mode::Fallback,
                &script,
            );
            assert_eq!(*result.scalar, expected[0]);
            let before = memo(&db, ingredient, second);
            let before_participant = before.header.may_be_provisional();
            let before_execution = before.header.revisions.execution_revision();
            let visits = script.visits.borrow().len();
            let (ordinary, ordinary_first, ordinary_second) = cycle_nodes(two, true);
            let (ordinary_first, ordinary_second) = if reverse {
                (ordinary_second, ordinary_first)
            } else {
                (ordinary_first, ordinary_second)
            };
            assert_eq!(fallback(&ordinary, ordinary_first), *result.scalar);
            let ordinary_ingredient = fallback::fn_ingredient_(&ordinary, ordinary.zalsa());
            assert_eq!(
                before_participant,
                memo(&ordinary, ordinary_ingredient, ordinary_second)
                    .header
                    .may_be_provisional()
            );
            let participant = fetch(
                &db,
                (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                second,
                None,
                false,
                Mode::Fallback,
                &script,
            );
            assert_eq!(*participant.scalar, expected[1]);
            assert_eq!(*participant.scalar, fallback(&ordinary, ordinary_second));
            assert_eq!(
                script.visits.borrow().len(),
                visits,
                "completed participant executed a callback"
            );
            assert!(std::ptr::eq(before, memo(&db, ingredient, second)));
            assert_eq!(
                before.header.revisions.execution_revision(),
                before_execution
            );
            assert_eq!(
                memo(&db, ingredient, second).header.may_be_provisional(),
                memo(&ordinary, ordinary_ingredient, ordinary_second)
                    .header
                    .may_be_provisional()
            );
            assert!(
                script
                    .visits
                    .borrow()
                    .iter()
                    .any(|visit| visit.phase == Phase::Initial)
            );
            assert_idle(&db, ingredient, first);
            assert_idle(&db, ingredient, second);
        }
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn registered_fetch_reuses_completed_foreign_participants_without_callbacks() {
    #[derive(Clone, Copy)]
    enum Producer {
        Running,
        Complete,
        Refused(Incomplete),
    }
    for producer in [
        Producer::Running,
        Producer::Complete,
        Producer::Refused(Incomplete::Allowance),
        Producer::Refused(Incomplete::Interrupted),
    ] {
        for reverse in [false, true] {
            let (db, first, second) = cycle_nodes(true, true);
            let (root, child, expected) = if reverse {
                (second, first, [20, 10])
            } else {
                (first, second, [10, 20])
            };
            let (ready_tx, ready_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let (left, right) = run_pair(
                &db,
                move |db, participant| {
                    let outcome = participant.run(&db, 100, || {
                        assert_eq!(fallback(&db, root), expected[0]);
                        if matches!(producer, Producer::Running) {
                            ready_tx.send(()).unwrap();
                            done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        }
                        if let Producer::Refused(reason) = producer {
                            attempt_probe::report_incomplete(&db, reason);
                        }
                    });
                    match producer {
                        Producer::Running | Producer::Complete => {
                            assert_eq!(outcome, Ok(AttemptOutcome::Complete(())))
                        }
                        Producer::Refused(reason) => {
                            assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(reason)))
                        }
                    }
                    if !matches!(producer, Producer::Running) {
                        ready_tx.send(()).unwrap();
                        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                },
                move |db, participant| {
                    participant.run(&db, 100, || {
                        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
                        let old = memo(&db, ingredient, child);
                        assert!(old.header.may_be_provisional());
                        let stamp = old.header.revisions.execution_revision();
                        let script = Script::default();
                        let result = fetch_in_attempt(
                            &db,
                            (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                            child,
                            None,
                            false,
                            Mode::Fallback,
                            &script,
                        )
                        .unwrap();
                        assert_eq!(*result.scalar, expected[1]);
                        assert!(script.visits.borrow().is_empty());
                        assert!(std::ptr::eq(old, memo(&db, ingredient, child)));
                        assert!(!old.header.may_be_provisional());
                        assert_eq!(old.header.revisions.execution_revision(), stamp);
                        let root_result = fetch_in_attempt(
                            &db,
                            (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                            root,
                            None,
                            false,
                            Mode::Fallback,
                            &script,
                        )
                        .unwrap();
                        assert_eq!(*root_result.scalar, expected[0]);
                        assert!(script.visits.borrow().is_empty());
                        done_tx.send(()).unwrap();
                    })
                },
            )
            .unwrap();
            left.unwrap();
            assert_eq!(right.unwrap(), Ok(AttemptOutcome::Complete(())));
            let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
            assert_idle(&db, ingredient, root);
            assert_idle(&db, ingredient, child);
        }
    }
}

#[test]
fn cold_initial_direct_refusal_preserves_first_reason_and_can_retry() {
    let (db, node, _) = cycle_nodes(false, false);
    let ingredient = self_cycle::fn_ingredient_(&db, db.zalsa());
    let wrapped_ingredient = wrapped::fn_ingredient_(&db, db.zalsa());
    let refusing = Script {
        refuse_initial: true,
        ..Script::default()
    };
    // This error comes directly from the callback, before endpoint admission or seed fetching
    // can record it. Discarding the cold-initial owner must not replace it with Interrupted.
    assert!(matches!(
        try_fetch(
            &db,
            (ingredient, wrapped_ingredient),
            node,
            None,
            false,
            Mode::SelfCycle,
            &refusing
        ),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    ));
    assert!(
        refusing
            .visits
            .borrow()
            .iter()
            .any(|visit| visit.phase == Phase::Initial)
    );
    assert!(
        !refusing
            .bodies()
            .iter()
            .any(|(kind, _)| *kind == Kind::Seed)
    );
    assert_idle(&db, ingredient, node);
    assert_eq!(
        *fetch(
            &db,
            (ingredient, wrapped_ingredient),
            node,
            None,
            false,
            Mode::SelfCycle,
            &Script::default()
        )
        .scalar,
        3
    );
    assert_idle(&db, ingredient, node);
}
