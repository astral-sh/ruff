use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route, RouteProvider,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::observation::{self, Event};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::maybe_changed_after::VerifyResult;
use crate::function::maybe_changed_after::validation::{Validation, ValidationStep};
use crate::function::sync::ClaimResult;
use crate::function::{Configuration, IngredientImpl, Memo, Reentrancy};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Durability, Id, Revision, Setter};

mod trace;
mod verification_event;

#[crate::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
    #[returns(copy)]
    value: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Count(u32);

trait TestOutput {
    const SCALAR: bool;
    fn new(value: u32) -> Self;
}

impl TestOutput for u32 {
    const SCALAR: bool = true;
    fn new(value: u32) -> Self {
        value
    }
}

impl TestOutput for Count {
    const SCALAR: bool = false;
    fn new(value: u32) -> Self {
        Self(value)
    }
}

/// Ordinary and controlled adapters share the body; the latter explicitly lacks child fetch.
fn body<O: TestOutput>(
    db: &dyn Database,
    node: Node,
    child: impl FnOnce(Node) -> RunResult<u32>,
) -> RunResult<O> {
    let value = match node.next(db) {
        Some(next) => child(next)? + 1,
        None => node.value(db) % 2,
    };
    Ok(O::new(value))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn scalar(db: &dyn Database, node: Node) -> u32 {
    body(db, node, |next| Ok(wrapped(db, next).0)).expect("ordinary dependencies complete")
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn wrapped(db: &dyn Database, node: Node) -> Count {
    body(db, node, |next| Ok(scalar(db, next))).expect("ordinary dependencies complete")
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, lru = 1)]
fn evictable(db: &dyn Database, node: Node) -> u32 {
    body(db, node, |next| Ok(wrapped(db, next).0)).expect("ordinary dependencies complete")
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_result = cyclic_initial)]
fn cyclic(db: &dyn Database, node: Node) -> u32 {
    node.next(db).map_or(0, |next| cyclic(db, next)) + 1
}

fn cyclic_initial(db: &dyn Database, _id: Id, node: Node) -> u32 {
    node.value(db)
}

struct Admission {
    work: Cell<usize>,
    stop: Option<usize>,
    panic: bool,
}

impl Admission {
    fn unrestricted() -> Self {
        Self {
            work: Cell::new(0),
            stop: None,
            panic: false,
        }
    }
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Work { .. }) {
            let ordinal = self.work.get();
            self.work.set(ordinal + 1);
            if self.stop == Some(ordinal) {
                assert!(!self.panic, "validation admission panic");
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Script {
    bodies: RefCell<Vec<(bool, Id, usize)>>,
    fail: Option<Id>,
    panic: bool,
    cancel: Option<Id>,
    nested: Option<(Id, DatabaseKeyIndex, Revision)>,
}

struct Providers<'a, 'db, A: Configuration, B: Configuration> {
    scalar: Route<'db, A>,
    wrapped: Route<'db, B>,
    script: &'a Script,
}

impl<'run, 'db: 'run, A, B, C> ExecutableRouteProvider<'run, 'db, C> for Providers<'run, 'db, A, B>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node>,
    C: Configuration<DbView = dyn Database, Input<'db> = Node>,
    C::Output<'db>: TestOutput,
{
    // Node conversion is handle-only; TestOutput contains u32 or its Count wrapper.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<C::Output<'db>> {
        let scalar = <C::Output<'db> as TestOutput>::SCALAR;
        let key = if scalar {
            self.scalar.database_key(node.as_id())
        } else {
            self.wrapped.database_key(node.as_id())
        };
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(key)
        );
        super::validation_trace::record(super::validation_trace::TraceEvent::Body { key });
        self.script.bodies.borrow_mut().push((
            scalar,
            node.as_id(),
            attempt_probe::stack_depths().0,
        ));
        // A non-query child establishes a real suspension under the execution frame.
        context.endpoint().demand(|| async { Ok(()) })?.await?;
        if self.script.cancel == Some(node.as_id()) {
            db.zalsa().runtime().set_cancellation_flag();
        }
        if self.script.fail == Some(node.as_id()) {
            assert!(!self.script.panic, "validation body panic");
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        if let Some((owner, key, revision)) = self.script.nested
            && owner == node.as_id()
        {
            context.endpoint().validate(key, revision)?.await?;
            assert_eq!(
                db.zalsa_local().active_query().map(|(key, _)| key),
                Some(self.scalar.database_key(node.as_id()))
            );
        }
        body(db, node, |_| Err(RunError::RequiresFetch))
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _node: Node,
    ) -> RunResult<C::Output<'db>> {
        Err(RunError::Contract(
            "acyclic fixture requested an initial value",
        ))
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call C::Output<'db>,
        _value: C::Output<'db>,
        _node: Node,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        Err(RunError::Contract("acyclic fixture requested recovery"))
    }
}

// A separate proof callback must never be accepted as runtime memo verification.
impl<'run, 'db: 'run, A, B, C> RouteProvider<'run, 'db, C> for Providers<'run, 'db, A, B>
where
    A: Configuration,
    B: Configuration,
    C: Configuration,
{
    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db C::DbView,
        _input: C::Input<'db>,
    ) -> RunResult<C::Output<'db>> {
        panic!("proof body is not an executable route")
    }
    async fn verify(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db C::DbView,
        _id: Id,
        _revision: Revision,
    ) -> RunResult<VerifyResult> {
        panic!("provider-authored validity must not run")
    }
}

#[derive(Clone, Copy)]
enum RouteMode {
    Both,
    MissingWrapped,
    ProofWrapped,
}

fn run<'db, A, B>(
    db: &'db dyn Database,
    (scalar_ingredient, wrapped_ingredient): (&'db IngredientImpl<A>, &'db IngredientImpl<B>),
    node: Node,
    revision: Revision,
    script: &Script,
    admission: &dyn ExecutionAdmission,
    mode: RouteMode,
) -> RunResult<VerifyResult>
where
    A: Configuration<DbView = dyn Database, Input<'db> = Node>,
    B: Configuration<DbView = dyn Database, Input<'db> = Node>,
    A::Output<'db>: TestOutput,
    B::Output<'db>: TestOutput,
{
    let mut registry = RegistryBuilder::new(db, admission)?;
    let scalar = registry.reserve(db, scalar_ingredient)?;
    let wrapped = match mode {
        RouteMode::MissingWrapped => {
            let mut other = RegistryBuilder::new(db, admission)?;
            other.reserve(db, wrapped_ingredient)?
        }
        _ => registry.reserve(db, wrapped_ingredient)?,
    };
    let providers = Providers {
        scalar: scalar.clone(),
        wrapped: wrapped.clone(),
        script,
    };
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    let wrong_binding = registry.provider(&providers)?;
    registry.bind_executable(&scalar, &binding)?;
    match mode {
        RouteMode::Both => registry.bind_executable(&wrapped, &binding)?,
        RouteMode::ProofWrapped => registry.bind(&wrapped, &binding)?,
        RouteMode::MissingWrapped => {}
    }
    registry.seal()?.run(move |endpoint| async move {
        let wrong = endpoint.provider(wrong_binding)?;
        assert!(matches!(
            wrong.validate(&scalar, node.as_id(), revision),
            Err(RunError::Contract(
                "query route has a different provider binding"
            ))
        ));
        assert!(matches!(
            endpoint.verify_callback(scalar.database_key(node.as_id()), revision),
            Err(RunError::Contract("verification route is not bound"))
        ));
        let context = endpoint.provider(binding)?;
        let typed = context.validate(&scalar, node.as_id(), revision)?.await?;
        let erased = endpoint
            .validate(scalar.database_key(node.as_id()), revision)?
            .await?;
        assert_eq!(typed.is_unchanged(), erased.is_unchanged());
        Ok(typed)
    })
}

fn validate(
    db: &DatabaseImpl,
    node: Node,
    revision: Revision,
    script: &Script,
    admission: &dyn ExecutionAdmission,
    mode: RouteMode,
) -> RunResult<VerifyResult> {
    run(
        db,
        (
            scalar::fn_ingredient_(db, db.zalsa()),
            wrapped::fn_ingredient_(db, db.zalsa()),
        ),
        node,
        revision,
        script,
        admission,
        mode,
    )
}

fn fixture(depth: usize) -> (DatabaseImpl, Node, Node, Revision) {
    let mut db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, 4);
    let mut root = leaf;
    for _ in 1..depth {
        root = Node::new(&db, Some(root), 0);
    }
    assert_eq!(scalar(&db, root), (depth - 1) as u32);
    let revision = db.zalsa().current_revision();
    leaf.set_value(&mut db).to(6);
    (db, root, leaf, revision)
}

fn assert_idle(db: &DatabaseImpl, node: Node) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    let ingredient = scalar::fn_ingredient_(db, db.zalsa());
    match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        node.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim.abort(),
        _ => panic!("validation retained its claim"),
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
        .expect("fixture created this memo")
}

#[test]
fn hot_higher_durability_and_missing_memos_do_not_claim_or_execute() {
    for decision in ["hot", "higher durability", "missing"] {
        let mut db = DatabaseImpl::default();
        let node = Node::builder(None, 4).durability(Durability::HIGH).new(&db);
        if decision != "missing" {
            assert_eq!(scalar(&db, node), 0);
        }
        let revision = db.zalsa().current_revision();
        if decision == "higher durability" {
            db.synthetic_write(Durability::LOW);
        }
        let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
        if decision == "higher durability" {
            assert!(
                memo(&db, ingredient, node).header.verified_at.load()
                    < db.zalsa().current_revision()
            );
        }
        let script = Script::default();
        let (result, observed) = observation::collect(|| {
            try_with_attempt(&db, 10_000, || {
                validate(
                    &db,
                    node,
                    revision,
                    &script,
                    &Admission::unrestricted(),
                    RouteMode::Both,
                )
            })
        });
        match result {
            Ok(AttemptOutcome::Complete(Ok(result))) => {
                assert_eq!(result.is_unchanged(), decision != "missing", "{decision}");
            }
            other => panic!("{decision}: {other:?}"),
        }
        assert!(
            observed.events.is_empty(),
            "{decision}: {:?}",
            observed.events
        );
        assert!(script.bodies.borrow().is_empty());
        if decision != "missing" {
            assert_eq!(
                memo(&db, ingredient, node).header.verified_at.load(),
                db.zalsa().current_revision()
            );
        }
        assert_idle(&db, node);
    }
}

#[test]
fn evicted_memo_verifies_metadata_but_cannot_backdate_an_invalid_value() {
    for changed in [false, true] {
        let mut db = DatabaseImpl::default();
        let node = Node::new(&db, None, 4);
        let recent = Node::new(&db, None, 6);
        assert_eq!(evictable(&db, node), 0);
        assert_eq!(evictable(&db, recent), 0);
        let revision = db.zalsa().current_revision();
        if changed {
            node.set_value(&mut db).to(6);
        } else {
            db.synthetic_write(Durability::LOW);
        }
        let ingredient = evictable::fn_ingredient_(&db, db.zalsa());
        let old = memo(&db, ingredient, node);
        assert!(
            old.value().is_none(),
            "LRU must actually evict the fixture value"
        );
        let script = Script::default();
        let (result, observed) = observation::collect(|| {
            try_with_attempt(&db, 10_000, || {
                run(
                    &db,
                    (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                    node,
                    revision,
                    &script,
                    &Admission::unrestricted(),
                    RouteMode::Both,
                )
            })
        });
        match result {
            Ok(AttemptOutcome::Complete(Ok(result))) => assert_eq!(result.is_unchanged(), !changed),
            other => panic!("evicted validation: {other:?}"),
        }
        assert!(script.bodies.borrow().is_empty());
        assert!(
            observed
                .events
                .iter()
                .any(|event| matches!(event, Event::Claim { .. }))
        );
        assert!(
            !observed
                .events
                .iter()
                .any(|event| matches!(event, Event::Execute { .. }))
        );
        assert!(std::ptr::eq(old, memo(&db, ingredient, node)));
        assert!(old.value().is_none());
    }
}

#[test]
fn provisional_participant_is_finalized_or_reported_changed_without_execution() {
    for changed in [false, true] {
        let mut db = DatabaseImpl::default();
        let head = Node::new(&db, None, 10);
        let participant = Node::new(&db, Some(head), 20);
        head.set_next(&mut db).to(Some(participant));
        assert_eq!(cyclic(&db, head), 10);
        let revision = db.zalsa().current_revision();
        assert!(
            memo(&db, cyclic::fn_ingredient_(&db, db.zalsa()), participant)
                .header
                .may_be_provisional()
        );
        if changed {
            head.set_value(&mut db).to(11);
        }
        let ingredient = cyclic::fn_ingredient_(&db, db.zalsa());
        let old = memo(&db, ingredient, participant);
        assert!(old.header.may_be_provisional());
        let script = Script::default();
        let result = try_with_attempt(&db, 10_000, || {
            run(
                &db,
                (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                participant,
                revision,
                &script,
                &Admission::unrestricted(),
                RouteMode::Both,
            )
        });
        match result {
            Ok(AttemptOutcome::Complete(Ok(result))) => assert_eq!(result.is_unchanged(), !changed),
            other => panic!("provisional validation: {other:?}"),
        }
        assert!(script.bodies.borrow().is_empty());
        assert!(std::ptr::eq(old, memo(&db, ingredient, participant)));
        assert_eq!(old.header.may_be_provisional(), changed);
    }
}

#[test]
fn registered_validation_preserves_completed_participants_after_head_revalidation() {
    for refusal in [
        None,
        Some(Incomplete::Allowance),
        Some(Incomplete::Interrupted),
    ] {
        for head_first in [false, true] {
            let mut db = DatabaseImpl::default();
            let head = Node::builder(None, 10)
                .durability(Durability::HIGH)
                .new(&db);
            let participant = Node::builder(Some(head), 20)
                .durability(Durability::HIGH)
                .new(&db);
            head.set_next(&mut db)
                .with_durability(Durability::HIGH)
                .to(Some(participant));
            let outcome = try_with_attempt(&db, 100, || {
                assert_eq!(cyclic(&db, head), 10);
                if let Some(reason) = refusal {
                    attempt_probe::report_incomplete(&db, reason);
                }
            });
            assert_eq!(
                outcome,
                Ok(match refusal {
                    None => AttemptOutcome::Complete(()),
                    Some(reason) => AttemptOutcome::Incomplete(reason),
                })
            );
            let revision = db.zalsa().current_revision();
            let ingredient = cyclic::fn_ingredient_(&db, db.zalsa());
            let head_identity = std::ptr::from_ref(memo(&db, ingredient, head));
            let participant_identity = std::ptr::from_ref(memo(&db, ingredient, participant));
            assert!(
                memo(&db, ingredient, participant)
                    .header
                    .may_be_provisional()
            );
            db.synthetic_write(Durability::LOW);
            let ingredient = cyclic::fn_ingredient_(&db, db.zalsa());
            if head_first {
                assert_eq!(cyclic(&db, head), 10);
            }
            let old = memo(&db, ingredient, participant);
            let script = Script::default();
            let result = try_with_attempt(&db, 100, || {
                run(
                    &db,
                    (ingredient, wrapped::fn_ingredient_(&db, db.zalsa())),
                    participant,
                    revision,
                    &script,
                    &Admission::unrestricted(),
                    RouteMode::Both,
                )
            });
            assert!(matches!(
                result,
                Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
            ));
            assert!(script.bodies.borrow().is_empty());
            assert_eq!(old.value(), Some(&20));
            assert!(!old.header.may_be_provisional());
            assert_eq!(std::ptr::from_ref(old), participant_identity);
            assert_eq!(
                std::ptr::from_ref(memo(&db, ingredient, head)),
                head_identity
            );
            assert_eq!(old.header.revisions.execution_revision(), Some(revision));
            assert_eq!(old.header.verified_at.load(), db.zalsa().current_revision());
            assert_eq!(cyclic(&db, participant), 20);
            assert_eq!(cyclic(&db, head), 10);
            assert!(std::ptr::eq(old, memo(&db, ingredient, participant)));
        }
    }
}

#[test]
fn reexecution_receives_the_original_claim_operation_and_old_memo() {
    let (db, root, leaf, revision) = fixture(3);
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let leaf_key = ingredient.database_key_index(leaf.as_id());
    let old = memo(&db, ingredient, leaf);
    let script = Script::default();
    let (result, observed) = observation::collect(|| {
        try_with_attempt(&db, 10_000, || {
            validate(
                &db,
                root,
                revision,
                &script,
                &Admission::unrestricted(),
                RouteMode::Both,
            )
        })
    });
    assert!(matches!(
        result,
        Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
    ));
    let claims = observed
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Claim {
                key,
                serial,
                operation,
            } => Some((*key, *serial, *operation)),
            Event::Execute { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 3);
    let [(key, serial, operation)] = claims
        .iter()
        .filter(|(key, _, _)| *key == leaf_key)
        .copied()
        .collect::<Vec<_>>()[..]
    else {
        panic!("leaf must have one original claim: {claims:?}");
    };
    assert_eq!(
        observed
            .events
            .iter()
            .filter(|event| matches!(event, Event::Execute { .. }))
            .copied()
            .collect::<Vec<_>>(),
        [Event::Execute {
            key,
            serial,
            operation,
            old_memo: Some(std::ptr::from_ref(old).addr())
        }]
    );
    assert_eq!(operation, 2);
    assert!(!std::ptr::eq(old, memo(&db, ingredient, leaf)));
    assert_eq!(
        memo(&db, ingredient, leaf).header.revisions.changed_at,
        old.header.revisions.changed_at
    );
    assert_eq!(&*script.bodies.borrow(), &[(true, leaf.as_id(), 3)]);
    assert_idle(&db, root);
    assert_idle(&db, leaf);
}

#[test]
fn execution_without_a_selected_memo_restarts_the_shared_probe() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 4);
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let revision = db.zalsa().current_revision();
    let validation = Validation::new(ingredient, &db, node.as_id(), revision);
    // The real executor produces None only after worker transfer. Exercise its common
    // continuation without pretending that the controlled driver supports that transfer.
    let ValidationStep::Probe(validation) = validation.executed(None) else {
        panic!("no selected memo must retry rather than complete");
    };
    assert!(matches!(validation.probe(), Some(VerifyResult::Changed)));
    assert_eq!(scalar(&db, node), 0);
    let ValidationStep::Probe(validation) = validation.executed(None) else {
        panic!("retry must retain the validation request");
    };
    assert!(matches!(
        validation.probe(),
        Some(VerifyResult::Unchanged { .. })
    ));
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn long_validation_chains_do_not_nest_runtime_task_polls() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            for depth in [16, 256] {
                validation_chain(depth);
            }
        })
        .expect("start a thread with a bounded native stack")
        .join()
        .expect("validation chain completes");
}

#[cfg(not(feature = "shuttle"))]
fn validation_chain(depth: u32) {
    let mut db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, 4);
    let mut root = leaf;
    // Warm both routes bottom-up, so the setup itself does not need a deep native stack.
    for level in 0..depth {
        if level > 0 {
            root = Node::new(&db, Some(root), 0);
        }
        assert_eq!(scalar(&db, root), level);
        assert_eq!(wrapped(&db, root).0, level);
    }
    let revision = db.zalsa().current_revision();
    leaf.set_value(&mut db).to(6);
    let script = Script::default();
    let (result, observed) = observation::collect(|| {
        try_with_attempt(&db, 100_000, || {
            validate(
                &db,
                root,
                revision,
                &script,
                &Admission::unrestricted(),
                RouteMode::Both,
            )
        })
    });
    assert!(matches!(
        result,
        Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
    ));
    assert_eq!(
        &*script.bodies.borrow(),
        &[(false, leaf.as_id(), depth as usize)]
    );
    assert_eq!(observed.max_active_polls, 1);
    assert!(observed.polls > depth as usize);
    assert_eq!(
        observed
            .events
            .iter()
            .filter(|event| matches!(event, Event::Claim { .. }))
            .count(),
        depth as usize
    );
    assert_idle(&db, root);
}

#[cfg(not(feature = "shuttle"))]
struct CancelValidation<'db> {
    db: &'db DatabaseImpl,
    cancelled: Cell<bool>,
}

#[cfg(not(feature = "shuttle"))]
impl ExecutionAdmission for CancelValidation<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Work { .. })
            && attempt_probe::stack_depths().0 == 2
            && !self.cancelled.replace(true)
        {
            assert!(self.db.zalsa_local().active_query().is_none());
            self.db.zalsa().runtime().set_cancellation_flag();
        }
        Ok(())
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn cancellation_preserves_payload_and_cleans_frame_free_and_executing_owners() {
    for during_body in [false, true] {
        let (db, root, leaf, revision) = fixture(3);
        let script = Script {
            cancel: during_body.then_some(leaf.as_id()),
            ..Script::default()
        };
        let cancelling = CancelValidation {
            db: &db,
            cancelled: Cell::new(false),
        };
        let unrestricted = Admission::unrestricted();
        let admission: &dyn ExecutionAdmission = if during_body {
            &unrestricted
        } else {
            &cancelling
        };
        let (cancelled, observed) = observation::collect(|| {
            crate::Cancelled::catch(AssertUnwindSafe(|| {
                try_with_attempt(&db, 10_000, || {
                    validate(&db, root, revision, &script, admission, RouteMode::Both)
                })
            }))
        });
        db.zalsa().runtime().reset_cancellation_flag();
        assert!(matches!(cancelled, Err(crate::Cancelled::PendingWrite)));
        assert!(
            observed
                .events
                .iter()
                .any(|event| matches!(event, Event::Claim { .. }))
        );
        assert_eq!(
            observed
                .events
                .iter()
                .any(|event| matches!(event, Event::Execute { .. })),
            during_body
        );
        assert_idle(&db, root);
        assert_idle(&db, leaf);
        assert!(matches!(
            try_with_attempt(&db, 10_000, || validate(
                &db,
                root,
                revision,
                &Script::default(),
                &Admission::unrestricted(),
                RouteMode::Both
            )),
            Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
        ));
    }
}

#[test]
fn heterogeneous_validation_reexecutes_only_the_leaf_and_backdates() {
    let (db, root, leaf, revision) = fixture(96);
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let old = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            root.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), root.as_id()),
        )
        .unwrap();
    let script = Script::default();
    let result = try_with_attempt(&db, 100_000, || {
        validate(
            &db,
            root,
            revision,
            &script,
            &Admission::unrestricted(),
            RouteMode::Both,
        )
    });
    assert!(matches!(
        result,
        Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
    ));
    assert_eq!(&*script.bodies.borrow(), &[(false, leaf.as_id(), 96)]);
    let current = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            root.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), root.as_id()),
        )
        .unwrap();
    assert!(std::ptr::eq(old, current));
    assert_eq!(
        current.header.verified_at.load(),
        db.zalsa().current_revision()
    );
    assert!(current.header.revisions.changed_at <= revision);
    assert_idle(&db, root);
}

#[test]
fn every_validation_work_refusal_releases_claims_and_can_retry() {
    let (db, root, _, revision) = fixture(3);
    let admission = Admission::unrestricted();
    assert!(matches!(
        try_with_attempt(&db, 10_000, || validate(
            &db,
            root,
            revision,
            &Script::default(),
            &admission,
            RouteMode::Both
        )),
        Ok(AttemptOutcome::Complete(Ok(_)))
    ));
    for ordinal in 0..admission.work.get() {
        let (db, root, _, revision) = fixture(3);
        let stop = Admission {
            work: Cell::new(0),
            stop: Some(ordinal),
            panic: false,
        };
        assert_eq!(
            try_with_attempt(&db, 10_000, || validate(
                &db,
                root,
                revision,
                &Script::default(),
                &stop,
                RouteMode::Both
            ))
            .map(|outcome| matches!(outcome, AttemptOutcome::Incomplete(Incomplete::Allowance))),
            Ok(true),
            "work {ordinal}"
        );
        assert_idle(&db, root);
        assert!(
            matches!(
                try_with_attempt(&db, 10_000, || validate(
                    &db,
                    root,
                    revision,
                    &Script::default(),
                    &Admission::unrestricted(),
                    RouteMode::Both
                )),
                Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
            ),
            "retry {ordinal}"
        );
    }
}

#[test]
fn pending_frame_free_claims_preserve_admission_panic() {
    let (db, root, _, revision) = fixture(3);
    let admission = Admission {
        work: Cell::new(0),
        stop: Some(8),
        panic: true,
    };
    let panic = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 10_000, || {
            validate(
                &db,
                root,
                revision,
                &Script::default(),
                &admission,
                RouteMode::Both,
            )
        })
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"validation admission panic")
    );
    assert_idle(&db, root);
    assert!(matches!(
        try_with_attempt(&db, 10_000, || validate(
            &db,
            root,
            revision,
            &Script::default(),
            &Admission::unrestricted(),
            RouteMode::Both
        )),
        Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
    ));
}

#[test]
fn missing_and_proof_routes_do_not_supply_validity() {
    for mode in [RouteMode::MissingWrapped, RouteMode::ProofWrapped] {
        let (db, root, _, revision) = fixture(2);
        let script = Script::default();
        assert!(matches!(
            try_with_attempt(&db, 10_000, || validate(
                &db,
                root,
                revision,
                &script,
                &Admission::unrestricted(),
                mode
            )),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ));
        assert!(script.bodies.borrow().is_empty());
        assert_idle(&db, root);
    }
}

#[test]
fn first_changed_input_skips_unregistered_later_dependency() {
    let (mut db, root, _, revision) = fixture(2);
    root.set_next(&mut db).to(None);
    let script = Script::default();
    assert!(matches!(
        try_with_attempt(&db, 10_000, || validate(
            &db,
            root,
            revision,
            &script,
            &Admission::unrestricted(),
            RouteMode::MissingWrapped
        )),
        Ok(AttemptOutcome::Complete(Ok(VerifyResult::Changed)))
    ));
    assert_eq!(script.bodies.borrow().len(), 1);
    assert_eq!(scalar(&db, root), 0);
}

#[test]
fn nested_validation_abort_and_panic_leave_the_callers_frame_owned() {
    for panic in [false, true] {
        let mut db = DatabaseImpl::default();
        let root = Node::new(&db, None, 4);
        let child = Node::new(&db, None, 4);
        assert_eq!(scalar(&db, root), 0);
        assert_eq!(scalar(&db, child), 0);
        let revision = db.zalsa().current_revision();
        root.set_value(&mut db).to(6);
        child.set_value(&mut db).to(6);
        let key = scalar::fn_ingredient_(&db, db.zalsa()).database_key_index(child.as_id());
        let script = Script {
            fail: Some(child.as_id()),
            panic,
            nested: Some((root.as_id(), key, revision)),
            ..Script::default()
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 10_000, || {
                validate(
                    &db,
                    root,
                    revision,
                    &script,
                    &Admission::unrestricted(),
                    RouteMode::Both,
                )
            })
        }));
        if panic {
            assert_eq!(
                outcome.unwrap_err().downcast_ref::<&str>(),
                Some(&"validation body panic")
            );
        } else {
            assert!(matches!(
                outcome,
                Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)))
            ));
        }
        assert_eq!(
            script
                .bodies
                .borrow()
                .iter()
                .map(|(_, id, _)| *id)
                .collect::<Vec<_>>(),
            [root.as_id(), child.as_id()]
        );
        assert_idle(&db, root);
        assert_idle(&db, child);
        let retry = Script {
            nested: script.nested,
            ..Script::default()
        };
        assert!(matches!(
            try_with_attempt(&db, 10_000, || validate(
                &db,
                root,
                revision,
                &retry,
                &Admission::unrestricted(),
                RouteMode::Both
            )),
            Ok(AttemptOutcome::Complete(Ok(VerifyResult::Unchanged { .. })))
        ));
        let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
        let memo = ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                root.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), root.as_id()),
            )
            .unwrap();
        assert!(
            !memo
                .header
                .origin()
                .inputs()
                .any(|dependency| dependency == key),
            "validation must not run a fetch-read epilogue"
        );
    }
}
