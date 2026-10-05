use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;
use crate::execution_probe::{FinalSourceError, PreparedSourceError, PreparedSourceMemo};
use crate::function::execute::participant::Consumer;
use crate::function::{ClaimResult, Reentrancy};
use crate::prepared_source_probe::try_with_preparation;

mod host_bridge;

#[crate::tracked(debug)]
struct Product<'db> {
    value: u32,
}

// A certificate can be cloned even when the memo's value cannot be cloned.
#[derive(Debug, Eq, PartialEq, crate::SalsaValue)]
struct Value<'db> {
    product: Product<'db>,
    value: u32,
}

#[crate::tracked(returns(ref), attempt = CompleteOnly)]
fn producer<'db>(db: &'db dyn Db, input: Number) -> Value<'db> {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    let value = input.value(db);
    Value {
        product: Product::new(db, value),
        value,
    }
}

#[crate::tracked(returns(ref), attempt = CompleteOnly)]
fn other_producer<'db>(db: &'db dyn Db, input: Number) -> Value<'db> {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    let value = input.value(db);
    Value {
        product: Product::new(db, value),
        value,
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn prepared_consumer(db: &dyn Db, input: Number) -> u32 {
    db.counts().consumer.fetch_add(1, Ordering::Relaxed);
    producer(db, input).value + 1
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn certify_in_query(db: &dyn Db, input: Number) -> bool {
    matches!(
        producer::prepare_memo(db, input),
        Err(PreparedSourceError::ActiveQuery)
    )
}

#[derive(Clone, Copy)]
enum Stop {
    None,
    WrongConfiguration,
    CancelAfterFirstRead,
}

struct PreparedConsumer<'db> {
    certificate: PreparedSourceMemo<'db, Value<'db>>,
    stop: Stop,
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for PreparedConsumer<'db>
where
    C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Db,
        input: Number,
    ) -> RunResult<u32> {
        db.counts().consumer.fetch_add(1, Ordering::Relaxed);
        let certificate = self.certificate.clone();
        assert!(matches!(
            certificate.value(),
            Err(PreparedSourceError::ActiveAttempt)
        ));
        assert_eq!(
            certificate.check_current(),
            Err(PreparedSourceError::ActiveAttempt)
        );
        assert!(matches!(
            producer::prepare_memo(db, input),
            Err(PreparedSourceError::ActiveAttempt)
        ));
        let endpoint = context.endpoint();
        if matches!(self.stop, Stop::WrongConfiguration) {
            let value = endpoint
                .read_prepared_source(other_producer::prepared_read(&certificate))
                .await;
            return Ok(value.value + 1);
        }
        let first = endpoint
            .read_prepared_source(producer::prepared_read(&certificate))
            .await;
        db.counts().source_read_completed(db);
        if matches!(self.stop, Stop::CancelAfterFirstRead) {
            db.cancellation_token().cancel();
        }
        let second = endpoint
            .read_prepared_source(producer::prepared_read(&certificate))
            .await;
        db.counts().source_read_completed(db);
        assert!(std::ptr::eq(first, second));
        Ok(second.value + 1)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _id: Id,
        _input: Number,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Number,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn run<'db>(
    db: &'db dyn Db,
    input: Number,
    certificate: &PreparedSourceMemo<'db, Value<'db>>,
    stop: Stop,
) -> RunResult<u32> {
    let admission = Admission::default();
    let provider = PreparedConsumer {
        certificate: certificate.clone(),
        stop,
    };
    let mut registry = RegistryBuilder::new(db, &admission)?;
    let route = registry.reserve(db, prepared_consumer::fn_ingredient_(db, db.zalsa()))?;
    let binding = registry.provider(&provider)?;
    registry.bind_executable(&route, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&route, input.as_id())?
            .await?)
    })
}

fn assert_unpublished(db: &TestDb, input: Number) {
    let ingredient = prepared_consumer::fn_ingredient_(db, db.zalsa());
    assert!(
        ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .is_none()
    );
    assert_idle(db);
}

#[test]
fn prepared_memo_retains_producer_outputs_and_records_the_canonical_dependency()
-> Result<(), PreparedSourceError> {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    let value = producer(&db, input);
    let source = stored(
        &db,
        producer::fn_ingredient_(&db, db.zalsa()),
        input.as_id(),
    );
    let outputs = source.header.revisions.tracked_struct_ids().to_vec();
    assert_eq!(outputs.len(), 1);
    let certificate = producer::prepare_memo(&db, input)?;
    let key = certificate.database_key();
    let cloned = certificate.clone();
    assert!(std::ptr::eq(value, cloned.value()?));
    certificate.check_current()?;
    assert!(matches!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            producer::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        ),
        Err(FinalSourceError::OutputBearingMemo)
    ));
    db.counts.events.lock().unwrap().clear();
    assert_eq!(
        try_with_attempt(&db, 100_000, || run(&db, input, &cloned, Stop::None)),
        Ok(AttemptOutcome::Complete(Ok(8)))
    );
    assert_eq!(db.counts.bodies(), (1, 1));
    assert!(
        !db.counts
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|(_, observed)| *observed == key)
    );
    let consumer = stored(
        &db,
        prepared_consumer::fn_ingredient_(&db, db.zalsa()),
        input.as_id(),
    );
    assert_eq!(consumer.header.origin().inputs().collect::<Vec<_>>(), [key]);
    assert!(consumer.header.outputs_are_empty());
    assert_eq!(
        consumer.header.attempt_reuse(db.zalsa()),
        MemoReuse::Ordinary
    );
    assert_eq!(source.header.revisions.tracked_struct_ids(), outputs);
    assert_eq!(source.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
    assert_eq!(*certificate.value()?.product.value(&db), 7);
    assert_idle(&db);
    Ok(())
}

#[test]
fn prepared_certification_requires_an_idle_complete_only_memo() -> Result<(), PreparedSourceError> {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    assert!(matches!(
        producer::prepare_memo(&db, input),
        Err(PreparedSourceError::MissingMemo)
    ));
    assert_eq!(db.counts.bodies(), (0, 0));
    assert_eq!(prepared_consumer(&db, input), 8);
    assert!(matches!(
        prepared_consumer::prepare_memo(&db, input),
        Err(PreparedSourceError::UnsupportedPolicy)
    ));
    let certificate = producer::prepare_memo(&db, input)?;
    assert!(certify_in_query(&db, input));
    let operation = attempt_probe::enter(
        db.zalsa(),
        crate::attempt_probe::QueryPolicy::CompleteOnly,
        "prepared source certification",
    );
    assert!(matches!(
        producer::prepare_memo(&db, input),
        Err(PreparedSourceError::ActiveOperation)
    ));
    assert_eq!(
        certificate.check_current(),
        Err(PreparedSourceError::ActiveOperation)
    );
    assert!(matches!(
        certificate.value(),
        Err(PreparedSourceError::ActiveOperation)
    ));
    drop(operation);
    certificate.check_current()?;
    assert_idle(&db);
    Ok(())
}

#[test]
fn prepared_certification_rejects_a_retained_memo_after_its_input_changes()
-> Result<(), PreparedSourceError> {
    let mut db = TestDb::default();
    let input = Number::new(&db, 7);
    assert_eq!(producer(&db, input).value, 7);
    producer::prepare_memo(&db, input)?.check_current()?;
    let revision = db.zalsa().current_revision();
    let identity = std::ptr::from_ref(stored(
        &db,
        producer::fn_ingredient_(&db, db.zalsa()),
        input.as_id(),
    ))
    .addr();

    input.set_value(&mut db).to(8);
    let retained = stored(
        &db,
        producer::fn_ingredient_(&db, db.zalsa()),
        input.as_id(),
    );
    assert_eq!(std::ptr::from_ref(retained).addr(), identity);
    assert_eq!(retained.value().map(|value| value.value), Some(7));
    assert_eq!(retained.header.verified_at.load(), revision);
    assert_ne!(revision, db.zalsa().current_revision());
    assert!(matches!(
        producer::prepare_memo(&db, input),
        Err(PreparedSourceError::UnverifiedMemo)
    ));
    assert_eq!(db.counts.bodies(), (1, 0));
    assert_eq!(retained.header.verified_at.load(), revision);

    assert_eq!(producer(&db, input).value, 8);
    assert_eq!(producer::prepare_memo(&db, input)?.value()?.value, 8);
    assert_eq!(db.counts.bodies(), (2, 0));
    assert_idle(&db);
    Ok(())
}

#[test]
fn prepared_source_rejects_equal_values_from_a_different_allocation()
-> Result<(), PreparedSourceError> {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    assert_eq!(producer(&db, input).value, 7);
    let ingredient = producer::fn_ingredient_(&db, db.zalsa());
    let old = stored(&db, ingredient, input.as_id());
    let certificate = producer::prepare_memo(&db, input)?;
    let stamp = Stamp::current(&db);

    // Execute the complete-only producer again through its normal claim and publication path.
    // An equal value at the same stamp does not authorize a different memo allocation.
    assert_eq!(
        try_with_preparation(&db, || {
            let ClaimResult::Claimed(claim) = ingredient.sync_table.try_claim(
                db.zalsa(),
                db.zalsa_local(),
                input.as_id(),
                Reentrancy::Deny,
            ) else {
                return Err(RunError::Contract("the prepared source is not claimed"));
            };
            ingredient
                .execute(&db, claim, Some(old), Consumer::Validation)
                .ok_or(RunError::RequiresFetch)
                .map(|_| ())
        }),
        Ok(Ok(()))
    );
    let replacement = stored(&db, ingredient, input.as_id());
    assert!(!std::ptr::eq(old, replacement));
    assert_eq!(old.value(), replacement.value());
    assert_eq!(
        old.header.revisions.changed_at,
        replacement.header.revisions.changed_at
    );
    assert_eq!(
        old.header.origin().inputs().collect::<Vec<_>>(),
        replacement.header.origin().inputs().collect::<Vec<_>>()
    );
    assert_eq!(old.header.revisions.tracked_struct_ids().len(), 1);
    assert_eq!(
        old.header.revisions.tracked_struct_ids(),
        replacement.header.revisions.tracked_struct_ids()
    );
    assert_eq!(Stamp::current(&db), stamp);
    assert_eq!(db.counts.bodies(), (2, 0));
    assert_eq!(
        certificate.check_current(),
        Err(PreparedSourceError::ReplacedMemo)
    );
    assert_eq!(certificate.value(), Err(PreparedSourceError::ReplacedMemo));

    assert_eq!(
        try_with_attempt(&db, 100_000, || {
            let result = run(&db, input, &certificate, Stop::None);
            assert_eq!(
                result,
                Err(RunError::Contract("prepared source memo was replaced"))
            );
            result
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert!(db.counts.read_allowances.lock().unwrap().is_empty());
    assert_unpublished(&db, input);
    let fresh = producer::prepare_memo(&db, input)?;
    assert_eq!(replacement.value(), Some(fresh.value()?));
    assert_eq!(
        try_with_attempt(&db, 100_000, || run(&db, input, &fresh, Stop::None)),
        Ok(AttemptOutcome::Complete(Ok(8)))
    );
    assert_eq!(db.counts.bodies(), (2, 2));
    assert_eq!(Stamp::current(&db), stamp);
    assert_idle(&db);
    Ok(())
}

#[test]
fn prepared_request_defers_configuration_checks_until_its_live_read()
-> Result<(), PreparedSourceError> {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    producer(&db, input);
    let certificate = producer::prepare_memo(&db, input)?;
    let cancellations = db.counts.cancellations.load(Ordering::Relaxed);
    let events = db.counts.events.lock().unwrap().len();
    drop(other_producer::prepared_read(&certificate));
    assert_eq!(
        db.counts.cancellations.load(Ordering::Relaxed),
        cancellations
    );
    assert_eq!(db.counts.events.lock().unwrap().len(), events);
    assert_eq!(db.counts.bodies(), (1, 0));
    assert_eq!(
        try_with_attempt(&db, 100_000, || run(
            &db,
            input,
            &certificate,
            Stop::WrongConfiguration
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(db.counts.bodies(), (1, 1));
    assert_unpublished(&db, input);
    certificate.check_current()?;
    assert_eq!(
        try_with_attempt(&db, 100_000, || run(&db, input, &certificate, Stop::None)),
        Ok(AttemptOutcome::Complete(Ok(8)))
    );
    assert_eq!(db.counts.bodies(), (1, 2));
    assert_idle(&db);
    Ok(())
}

#[test]
fn prepared_reads_preserve_the_source_across_refusal_cancellation_and_retry()
-> Result<(), PreparedSourceError> {
    let first_read_cost = {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        producer(&db, input);
        let certificate = producer::prepare_memo(&db, input)?;
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, input, &certificate, Stop::None)),
            Ok(AttemptOutcome::Complete(Ok(8)))
        );
        100_000 - db.counts.read_allowances.lock().unwrap()[0]
    };
    for stop in [Stop::None, Stop::CancelAfterFirstRead] {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        producer(&db, input);
        let certificate = producer::prepare_memo(&db, input)?;
        let source = stored(
            &db,
            producer::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        );
        let outputs = source.header.revisions.tracked_struct_ids().to_vec();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(
                &db,
                if matches!(stop, Stop::None) {
                    first_read_cost
                } else {
                    100_000
                },
                || run(&db, input, &certificate, stop),
            )
        }));
        db.zalsa_local().uncancel();
        if matches!(stop, Stop::CancelAfterFirstRead) {
            assert!(matches!(outcome, Err(payload) if matches!(
                payload.downcast_ref::<crate::Cancelled>(), Some(crate::Cancelled::Local)
            )));
        } else {
            assert!(matches!(
                outcome,
                Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)))
            ));
        }
        assert_eq!(db.counts.read_allowances.lock().unwrap().len(), 1);
        assert_unpublished(&db, input);
        certificate.check_current()?;
        assert_eq!(source.header.revisions.tracked_struct_ids(), outputs);
        assert_eq!(*certificate.value()?.product.value(&db), 7);
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, input, &certificate, Stop::None)),
            Ok(AttemptOutcome::Complete(Ok(8)))
        );
        assert_eq!(db.counts.bodies(), (1, 2));
        assert_eq!(source.header.revisions.tracked_struct_ids(), outputs);
        assert_idle(&db);
    }
    Ok(())
}
