use std::cell::{Cell, RefCell};
use std::future::Future;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::super::field_run::{BorrowOrCopy, FieldReadProfile, FieldRequest, FieldReturnMode};
use super::super::native_values::NativeValueQuote;
use super::super::registration::{
    NativeCallbackLimits, RegistryBuilder, TaskEndpoint, with_native_callback,
};
use super::super::{RunError, RunResult};
use crate::active_query::read_storage::ReadState;
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, MemoReuse, try_with_execution_budget,
};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Memo, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseImpl, DatabaseKeyIndex, Durability, Id, Revision, Setter};

const RESERVE: usize = 1_000_000;

fn limits() -> ExecutionLimits {
    ExecutionLimits {
        semantic_work: RESERVE,
        requested_bytes: RESERVE,
    }
}

#[derive(Debug, Eq, Hash, PartialEq, crate::SalsaValue)]
struct Owned(Box<[u8; 8]>);

impl Clone for Owned {
    fn clone(&self) -> Self {
        note("clone");
        if OBSERVE_CLONE.get() {
            CLONED.with_borrow_mut(|cloned| {
                *cloned = crate::attach::with_attached_database(read_state).flatten();
            });
        }
        Self(Box::new(*self.0))
    }
}

#[crate::input(field_requests = read_fields)]
struct Input {
    #[returns(copy)]
    #[get(number)]
    value: u32,
    label: String,
    #[returns(clone)]
    owned: Owned,
}

#[crate::tracked(field_requests = read_fields)]
struct Mixed<'store> {
    #[returns(copy)]
    identity: u32,
    #[tracked]
    #[returns(copy)]
    #[get(number)]
    value: u32,
    #[tracked]
    label: String,
}

#[crate::interned(field_view = fields, field_requests = read_fields)]
struct Interned<'db> {
    #[returns(copy)]
    #[get(number)]
    value: u32,
    #[returns(ref)]
    label: String,
    #[returns(clone)]
    owned: Owned,
}

#[crate::tracked(returns(copy))]
fn make_mixed(db: &dyn Database, input: Input) -> Mixed<'_> {
    Mixed::new(db, 41, input.number(db), input.label(db).clone())
}

#[crate::tracked(returns(copy))]
fn ordinary_getters(db: &dyn Database, input: Input) -> ((u32, usize), (u32, u32, usize), u8) {
    let first = input.number(db);
    let label = input.label(db);
    let second = input.number(db);
    let mixed = make_mixed(db, input);
    let identity = mixed.identity(db);
    let number = mixed.number(db);
    let mixed_label = mixed.label(db);
    let owned = input.owned(db);
    BEFORE.with_borrow_mut(|before| *before = read_state(db));
    (
        (first + second, label.len()),
        (identity, number, mixed_label.len()),
        owned.0[0],
    )
}

#[crate::tracked(returns(copy))]
fn ordinary_requests(db: &dyn Database, input: Input) -> ((u32, usize), (u32, u32, usize), u8) {
    let fields = input.read_fields(db);
    let first = fields.number().read_ordinary();
    let label = fields.label().read_ordinary();
    let second = fields.number().read_ordinary();
    let mixed = make_mixed(db, input).read_fields(db);
    let identity = mixed.identity().read_ordinary();
    let number = mixed.number().read_ordinary();
    let mixed_label = mixed.label().read_ordinary();
    let owned = fields.owned().read_ordinary();
    BEFORE.with_borrow_mut(|before| *before = read_state(db));
    (
        (first + second, label.len()),
        (identity, number, mixed_label.len()),
        owned.0[0],
    )
}

#[crate::tracked(returns(copy))]
fn ordinary_interned<'db>(db: &'db dyn Database, value: Interned<'db>) -> (u32, usize) {
    let fields = value.read_fields(db);
    let number = fields.number().read_ordinary();
    let label = fields.label().read_ordinary();
    (number, std::ptr::from_ref(label).addr())
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn read_input(db: &dyn Database, input: Input) -> (u32, usize) {
    match with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        RegistryBuilder::for_native_callback_with_budget(db, &entry)?
            .seal()?
            .run(|endpoint| async move {
                let number = input.read_fields(db).number();
                let label = input.read_fields(db).label();
                let first = endpoint.read_field(number, &BorrowOrCopy).await;
                let second = endpoint
                    .read_field(input.read_fields(db).number(), &BorrowOrCopy)
                    .await;
                let label = endpoint.read_field(label, &BorrowOrCopy).await;
                Ok((first + second, label.len()))
            })
    }) {
        Ok(value) => value,
        Err(RunError::Refused(_)) => (0, 0),
        Err(error) => panic!("input field entry failed: {error:?}"),
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn read_mixed<'db>(db: &'db dyn Database, value: Mixed<'db>) -> (u32, u32, usize) {
    match with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        RegistryBuilder::for_native_callback_with_budget(db, &entry)?
            .seal()?
            .run(|endpoint| async move {
                let identity = value.read_fields(db).identity();
                let number = value.read_fields(db).number();
                let label = value.read_fields(db).label();
                let identity = endpoint.read_field(identity, &BorrowOrCopy).await;
                let number = endpoint.read_field(number, &BorrowOrCopy).await;
                let label = endpoint.read_field(label, &BorrowOrCopy).await;
                Ok((identity, number, label.len()))
            })
    }) {
        Ok(value) => value,
        Err(RunError::Refused(_)) => (0, 0, 0),
        Err(error) => panic!("tracked field entry failed: {error:?}"),
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn read_interned<'db>(db: &'db dyn Database, value: Interned<'db>) -> (u32, usize) {
    with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        RegistryBuilder::for_native_callback_with_budget(db, &entry)?
            .seal()?
            .run(|endpoint| async move {
                let fields = value.read_fields(endpoint.field_request_context());
                let number = endpoint.read_field(fields.number(), &BorrowOrCopy).await;
                let label = endpoint.read_field(fields.label(), &BorrowOrCopy).await;
                Ok((number, std::ptr::from_ref(label).addr()))
            })
    })
    .unwrap()
}

fn stored<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> Option<&'db Memo<C>> {
    ingredient.get_memo_from_table_for(
        db.zalsa(),
        id,
        ingredient.memo_ingredient_index(db.zalsa(), id),
    )
}

fn field_key(owner: DatabaseKeyIndex, index: usize) -> DatabaseKeyIndex {
    DatabaseKeyIndex::new(owner.ingredient_index().successor(index), owner.key_index())
}

fn memo_observation<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> Option<(Vec<DatabaseKeyIndex>, Durability, Revision)> {
    stored(db, ingredient, id).map(|memo| {
        (
            memo.header.origin().inputs().collect(),
            memo.header.revisions.durability,
            memo.header.revisions.changed_at,
        )
    })
}

#[test]
fn ordinary_requests_preserve_metadata_before_conversion_and_after_input_changes() {
    let mut db = DatabaseImpl::default();
    let input = Input::new(&db, 23, "fields".to_owned(), Owned(Box::new([7; 8])));
    for number in [23, 29] {
        if number != 23 {
            input.set_value(&mut db).to(number);
        }
        reset(Fault::None);
        OBSERVE_CLONE.set(true);
        let expected = crate::attach(&db, || ordinary_getters(&db, input));
        let getter_conversion = CLONED.take();
        assert!(getter_conversion.is_some());
        assert_eq!(getter_conversion, BEFORE.take());
        EVENTS.with_borrow(|events| assert_eq!(events, &["clone"]));

        reset(Fault::None);
        OBSERVE_CLONE.set(true);
        let actual = crate::attach(&db, || ordinary_requests(&db, input));
        assert_eq!(actual, expected);
        assert_eq!(actual, ((number * 2, 6), (41, number, 6), 7));
        assert_eq!(CLONED.take(), getter_conversion);
        assert_eq!(BEFORE.take(), getter_conversion);
        EVENTS.with_borrow(|events| assert_eq!(events, &["clone"]));

        let input_owner = Input::ingredient(&db).database_key_index(input.as_id());
        let mixed = make_mixed(&db, input);
        let mixed_owner = Mixed::ingredient(&db).database_key_index(mixed.as_id());
        let expected_metadata = Some((
            vec![
                field_key(input_owner, 0),
                field_key(input_owner, 1),
                make_mixed::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
                field_key(mixed_owner, 0),
                field_key(mixed_owner, 1),
                field_key(input_owner, 2),
            ],
            Durability::LOW,
            db.zalsa().current_revision(),
        ));
        assert_eq!(
            memo_observation(
                &db,
                ordinary_getters::fn_ingredient_(&db, db.zalsa()),
                input.as_id()
            ),
            expected_metadata,
        );
        assert_eq!(
            memo_observation(
                &db,
                ordinary_requests::fn_ingredient_(&db, db.zalsa()),
                input.as_id()
            ),
            expected_metadata,
        );
        assert!(std::ptr::eq(
            input.read_fields(&db).label().read_ordinary(),
            input.label(&db)
        ));
        assert!(std::ptr::eq(
            mixed.read_fields(&db).label().read_ordinary(),
            mixed.label(&db)
        ));
        assert_idle(&db, input);
    }
    reset(Fault::None);
}

#[test]
fn generated_requests_preserve_values_and_canonical_field_edges() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 23, "fields".to_owned(), Owned(Box::new([7; 8])));
    let mixed = make_mixed(&db, input);
    let expected_input = (input.number(&db) * 2, input.label(&db).len());
    let expected_mixed = (
        mixed.identity(&db),
        mixed.number(&db),
        mixed.label(&db).len(),
    );

    assert_eq!(
        try_with_execution_budget(&db, limits(), |_| read_input(&db, input)),
        Ok(AttemptOutcome::Complete(expected_input)),
    );
    assert_eq!(
        try_with_execution_budget(&db, limits(), |_| read_mixed(&db, mixed)),
        Ok(AttemptOutcome::Complete(expected_mixed)),
    );

    let input_owner = Input::ingredient(&db).database_key_index(input.as_id());
    let input_memo = stored(
        &db,
        read_input::fn_ingredient_(&db, db.zalsa()),
        input.as_id(),
    )
    .unwrap();
    assert_eq!(
        input_memo.header.origin().inputs().collect::<Vec<_>>(),
        [field_key(input_owner, 0), field_key(input_owner, 1)]
    );
    let mixed_owner = Mixed::ingredient(&db).database_key_index(mixed.as_id());
    let mixed_memo = stored(
        &db,
        read_mixed::fn_ingredient_(&db, db.zalsa()),
        mixed.as_id(),
    )
    .unwrap();
    assert_eq!(
        mixed_memo.header.origin().inputs().collect::<Vec<_>>(),
        [field_key(mixed_owner, 0), field_key(mixed_owner, 1)]
    );
    assert_eq!(input_memo.header.revisions.durability, Durability::LOW);
    assert_eq!(mixed_memo.header.revisions.durability, Durability::LOW);
    assert_eq!(
        input_memo.header.revisions.changed_at,
        db.zalsa().current_revision()
    );
    assert_eq!(
        mixed_memo.header.revisions.changed_at,
        db.zalsa().current_revision()
    );
    assert_idle(&db, input);
}

#[test]
fn interned_requests_preserve_canonical_borrows_without_adding_edges() {
    let db = DatabaseImpl::default();
    let value = Interned::new(&db, 23, "interned".to_owned(), Owned(Box::new([7; 8])));
    let expected = (
        value.number(&db),
        std::ptr::from_ref(value.label(&db)).addr(),
    );
    reset(Fault::None);
    let fields = value.read_fields(&db);
    let _request = fields.owned();
    EVENTS.with_borrow(|events| assert!(events.is_empty()));
    let result = try_with_execution_budget(&db, limits(), |_| read_interned(&db, value));
    assert_eq!(result, Ok(AttemptOutcome::Complete(expected)));
    let Ok(AttemptOutcome::Complete((_, label))) = result else {
        panic!("interned field read did not complete");
    };
    assert_eq!(
        label,
        std::ptr::from_ref(value.fields(crate::FieldReads::new(&db)).label()).addr()
    );
    let memo = stored(
        &db,
        read_interned::fn_ingredient_(&db, db.zalsa()),
        value.as_id(),
    )
    .unwrap();
    assert!(memo.header.origin().inputs().next().is_none());
    assert!(memo.header.outputs_are_empty());
    assert_eq!(ordinary_interned(&db, value), expected);
    assert_eq!(
        memo_observation(
            &db,
            ordinary_interned::fn_ingredient_(&db, db.zalsa()),
            value.as_id()
        ),
        Some((
            Vec::new(),
            memo.header.revisions.durability,
            memo.header.revisions.changed_at
        )),
    );
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    Work,
    Bytes,
    ChildRefusal,
    ChildCancellation,
}

thread_local! {
    static FAULT: Cell<Fault> = const { Cell::new(Fault::None) };
    static EVENTS: RefCell<Vec<&'static str>> = RefCell::new(Vec::with_capacity(32));
    static BEFORE: RefCell<Option<ReadState>> = const { RefCell::new(None) };
    static DROPPED: RefCell<Vec<(&'static str, Option<ReadState>)>> = RefCell::new(Vec::with_capacity(4));
    static OBSERVE_CLONE: Cell<bool> = const { Cell::new(false) };
    static CLONED: RefCell<Option<ReadState>> = const { RefCell::new(None) };
}

fn note(event: &'static str) {
    EVENTS.with_borrow_mut(|events| {
        assert!(events.len() < events.capacity());
        events.push(event);
    });
}

fn read_state(db: &dyn Database) -> Option<ReadState> {
    db.zalsa_local()
        .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
        .flatten()
}

struct Witness<'db> {
    db: &'db dyn Database,
    event: &'static str,
}

impl Drop for Witness<'_> {
    fn drop(&mut self) {
        note(self.event);
        DROPPED.with_borrow_mut(|dropped| dropped.push((self.event, read_state(self.db))));
    }
}

struct OwnedProfile;

impl FieldReadProfile<Owned> for OwnedProfile {
    fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call Owned,
        mode: FieldReturnMode,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call {
        async move {
            assert_eq!(mode, FieldReturnMode::Clone);
            note("quote");
            BEFORE.with_borrow_mut(|before| *before = read_state(endpoint.inner.context.db));
            match FAULT.get() {
                Fault::ChildRefusal | Fault::ChildCancellation => {
                    let child = Witness {
                        db: endpoint.inner.context.db,
                        event: "child",
                    };
                    let _reply = endpoint.demand(move || async move {
                        note("child ran");
                        drop(child);
                        Ok(())
                    })?;
                    if matches!(FAULT.get(), Fault::ChildCancellation) {
                        endpoint.inner.context.db.cancellation_token().cancel();
                        endpoint.check_completion()?;
                    }
                    return Err(RunError::Refused(Incomplete::Allowance));
                }
                _ => {}
            }
            Ok(NativeValueQuote {
                work: if matches!(FAULT.get(), Fault::Work) {
                    RESERVE + 1
                } else {
                    9
                },
                requested_bytes: if matches!(FAULT.get(), Fault::Bytes) {
                    RESERVE + 1
                } else {
                    8
                },
                cleanup_work: 1,
            })
        }
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn read_owned(db: &dyn Database, input: Input) -> u8 {
    match with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        RegistryBuilder::for_native_callback_with_budget(db, &entry)?
            .seal()?
            .run(|endpoint| async move {
                let _parent = Witness {
                    db,
                    event: "parent",
                };
                let request = input.read_fields(db).owned();
                let value = endpoint.read_field(request, &OwnedProfile).await;
                Ok(value.0[0])
            })
    }) {
        Ok(value) => value,
        Err(RunError::Refused(_)) => 0,
        Err(error) => panic!("owned field entry failed: {error:?}"),
    }
}

fn reset(fault: Fault) {
    FAULT.set(fault);
    EVENTS.with_borrow_mut(Vec::clear);
    BEFORE.with_borrow_mut(|before| *before = None);
    DROPPED.with_borrow_mut(Vec::clear);
    OBSERVE_CLONE.set(false);
    CLONED.with_borrow_mut(|cloned| *cloned = None);
}

fn assert_idle(db: &dyn Database, input: Input) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(matches!(
        read_owned::fn_ingredient_(db, db.zalsa())
            .sync_table
            .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
        ClaimResult::Claimed(()),
    ));
}

fn assert_refused_before_conversion(
    db: &dyn Database,
    input: Input,
    events: &[&str],
    returned_refusal: Option<Incomplete>,
) {
    EVENTS.with_borrow(|actual| assert_eq!(actual, events));
    let before = BEFORE.with_borrow(Clone::clone);
    assert!(before.is_some());
    DROPPED.with_borrow(|dropped| {
        for (_, state) in dropped {
            assert_eq!(state, &before);
        }
    });
    let memo = stored(
        db,
        read_owned::fn_ingredient_(db, db.zalsa()),
        input.as_id(),
    );
    if let Some(reason) = returned_refusal {
        // The native caller returns a typed fallback after refusal. Its incomplete support
        // prevents that value from being reused by another attempt or ordinary execution.
        let memo = memo.expect("the native caller retains its incomplete fallback");
        assert_eq!(memo.value(), Some(&0));
        assert!(memo.header.has_incomplete_attempt());
        assert_eq!(
            memo.header
                .revisions
                .attempt_support()
                .and_then(|support| support.reason()),
            Some(reason)
        );
        assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
        assert!(!memo.should_serialize());
        assert!(memo.header.origin().inputs().next().is_none());
        assert!(memo.header.outputs_are_empty());
    } else {
        assert!(
            memo.is_none(),
            "native cancellation cannot publish a fallback"
        );
    }
    assert_idle(db, input);
}

#[test]
fn native_conversion_refusal_preserves_metadata_and_same_revision_retry() {
    for (fault, reason) in [
        (Fault::Work, Incomplete::Allowance),
        (Fault::Bytes, Incomplete::RequestedAllocation),
    ] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 23, "fields".to_owned(), Owned(Box::new([7; 8])));
        let stamp = Stamp::current(&db);
        reset(fault);
        assert_eq!(
            try_with_execution_budget(&db, limits(), |_| read_owned(&db, input)),
            Ok(AttemptOutcome::Incomplete(reason))
        );
        assert_refused_before_conversion(&db, input, &["quote", "parent"], Some(reason));
        assert_eq!(Stamp::current(&db), stamp);

        reset(Fault::None);
        assert_eq!(
            try_with_execution_budget(&db, limits(), |_| read_owned(&db, input)),
            Ok(AttemptOutcome::Complete(7))
        );
        EVENTS.with_borrow(|events| assert_eq!(events, &["quote", "clone", "parent"]));
        let memo = stored(
            &db,
            read_owned::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        )
        .unwrap();
        assert_eq!(memo.value(), Some(&7));
        assert!(!memo.header.has_incomplete_attempt());
        assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
        let owner = Input::ingredient(&db).database_key_index(input.as_id());
        assert_eq!(
            memo.header.origin().inputs().collect::<Vec<_>>(),
            [field_key(owner, 2)]
        );
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db, input);
    }
}

#[test]
fn queued_children_drain_before_field_read_owners_on_refusal_and_cancellation() {
    for fault in [Fault::ChildRefusal, Fault::ChildCancellation] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 23, "fields".to_owned(), Owned(Box::new([7; 8])));
        let stamp = Stamp::current(&db);
        reset(fault);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_execution_budget(&db, limits(), |_| read_owned(&db, input))
        }));
        db.zalsa_local().uncancel();
        match fault {
            Fault::ChildCancellation => {
                let payload = outcome.expect_err("Salsa cancellation reaches the caller");
                assert!(matches!(
                    payload.downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::Local)
                ));
            }
            _ => assert_eq!(
                outcome.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            ),
        }
        let returned_refusal = match fault {
            Fault::ChildCancellation => None,
            _ => Some(Incomplete::Allowance),
        };
        assert_refused_before_conversion(
            &db,
            input,
            &["quote", "child", "parent"],
            returned_refusal,
        );
        assert_eq!(Stamp::current(&db), stamp);
        reset(Fault::None);
        assert_eq!(
            try_with_execution_budget(&db, limits(), |_| read_owned(&db, input)),
            Ok(AttemptOutcome::Complete(7))
        );
        EVENTS.with_borrow(|events| assert_eq!(events, &["quote", "clone", "parent"]));
        let memo = stored(
            &db,
            read_owned::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        )
        .unwrap();
        assert_eq!(memo.value(), Some(&7));
        assert!(!memo.header.has_incomplete_attempt());
        assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db, input);
    }
}

#[test]
fn interned_conversion_refusal_and_cancellation_allow_same_revision_retry() {
    for fault in [
        Fault::Work,
        Fault::Bytes,
        Fault::ChildRefusal,
        Fault::ChildCancellation,
    ] {
        let db = DatabaseImpl::default();
        let value = Interned::new(&db, 23, "interned".to_owned(), Owned(Box::new([7; 8])));
        let stamp = Stamp::current(&db);
        let run = || {
            try_with_execution_budget(&db, limits(), |budget| {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| {
                        let db = &db;
                        async move {
                            let _parent = Witness {
                                db,
                                event: "parent",
                            };
                            let request =
                                value.read_fields(endpoint.field_request_context()).owned();
                            let owned = endpoint.read_field(request, &OwnedProfile).await;
                            Ok(owned.0[0])
                        }
                    })
            })
        };
        reset(fault);
        let result = catch_unwind(AssertUnwindSafe(run));
        db.zalsa_local().uncancel();
        match fault {
            Fault::ChildCancellation => {
                let payload = result.expect_err("Salsa cancellation reaches the caller");
                assert!(matches!(
                    payload.downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::Local)
                ));
            }
            _ => assert_eq!(
                result.unwrap(),
                Ok(AttemptOutcome::Incomplete(
                    if matches!(fault, Fault::Bytes) {
                        Incomplete::RequestedAllocation
                    } else {
                        Incomplete::Allowance
                    }
                ))
            ),
        }
        let expected: &[&str] = if matches!(fault, Fault::ChildRefusal | Fault::ChildCancellation) {
            &["quote", "child", "parent"]
        } else {
            &["quote", "parent"]
        };
        EVENTS.with_borrow(|events| assert_eq!(events, expected));
        assert_eq!(Stamp::current(&db), stamp);
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(
            crate::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(())
        );

        reset(Fault::None);
        assert_eq!(run(), Ok(AttemptOutcome::Complete(Ok(7))));
        EVENTS.with_borrow(|events| assert_eq!(events, &["quote", "clone", "parent"]));
        assert_eq!(Stamp::current(&db), stamp);
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
    }
}
