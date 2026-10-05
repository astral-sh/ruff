use super::*;
use crate::Accumulator;
use crate::accumulator::accumulated_map::InputAccumulatedValues;

#[crate::accumulator]
struct Warning(u32);

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn direct(db: &dyn Db, input: Number) -> u32 {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    if input.value(db) != 0 {
        Warning(7).accumulate(db);
    }
    0
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn indirect(db: &dyn Db, input: Number) -> u32 {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    direct(db, input)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn direct_consumer(db: &dyn Db, input: Number) -> u32 {
    db.counts().consumer.fetch_add(1, Ordering::Relaxed);
    direct(db, input) + 1
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn indirect_consumer(db: &dyn Db, input: Number) -> u32 {
    db.counts().consumer.fetch_add(1, Ordering::Relaxed);
    indirect(db, input) + 1
}

fn check<'db, C, D>(
    db: &'db TestDb,
    source: &'db IngredientImpl<C>,
    consumer: &'db IngredientImpl<D>,
    input: Number,
    previous: Revision,
    any: bool,
) where
    C: Configuration<DbView = dyn Db, Output<'db> = u32>,
    D: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    let certificate = FinalSourceMemo::certify(db as &dyn Db, source, input.as_id()).unwrap();
    let key = certificate.database_key();
    let before = db.counts.source.load(Ordering::Relaxed);
    let admission = Admission::default();
    let outcome = try_with_attempt(db, 100_000, || {
        let provider;
        let mut registry = RegistryBuilder::new(db, &admission)?;
        provider = Consumer {
            source: registry.register_final_source(db as &dyn Db, source, &[certificate])?,
            reads: 1,
        };
        let route = registry.reserve(db as &dyn Db, consumer)?;
        let binding = registry.provider(&provider)?;
        registry.bind_executable(&route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let result = endpoint.validate(key, previous)?.await?;
            assert!(matches!(result, VerifyResult::Unchanged { accumulated } if accumulated == if any { InputAccumulatedValues::Any } else { InputAccumulatedValues::Empty }));
            Ok(*endpoint.provider(binding)?.fetch_ref(&route, input.as_id())?.await?)
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(1))));
    assert_eq!(db.counts.source.load(Ordering::Relaxed), before);
    assert_eq!(db.counts.consumer.load(Ordering::Relaxed), 1);
    let memo = stored(db, consumer, input.as_id());
    assert_eq!(memo.header.origin().inputs().collect::<Vec<_>>(), [key]);
    assert_eq!(
        memo.header.revisions.accumulated_inputs.load().is_any(),
        any
    );
    assert_idle(db);
}

#[test]
fn finalized_source_preserves_direct_and_indirect_accumulated_inputs() {
    for through_indirect in [false, true] {
        let mut db = TestDb::default();
        let input = Number::new(&db, 0);
        assert_eq!(indirect(&db, input), 0);
        let first_revision = db.zalsa().current_revision();
        if through_indirect {
            check(
                &db,
                indirect::fn_ingredient_(&db, db.zalsa()),
                indirect_consumer::fn_ingredient_(&db, db.zalsa()),
                input,
                first_revision,
                false,
            );
        } else {
            check(
                &db,
                direct::fn_ingredient_(&db, db.zalsa()),
                direct_consumer::fn_ingredient_(&db, db.zalsa()),
                input,
                first_revision,
                false,
            );
        }
        input.set_value(&mut db).to(1);
        assert_eq!(indirect(&db, input), 0);
        let direct_memo = stored(&db, direct::fn_ingredient_(&db, db.zalsa()), input.as_id());
        let indirect_memo = stored(
            &db,
            indirect::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        );
        assert_eq!(direct_memo.header.revisions.changed_at, first_revision);
        assert_eq!(indirect_memo.header.revisions.changed_at, first_revision);
        assert!(direct_memo.header.revisions.accumulated().is_some());
        assert!(
            indirect_memo
                .header
                .revisions
                .accumulated_inputs
                .load()
                .is_any()
        );
        assert!(direct_memo.header.outputs_are_empty() && indirect_memo.header.outputs_are_empty());
        let before = db.counts.bodies();
        let warnings = if through_indirect {
            check(
                &db,
                indirect::fn_ingredient_(&db, db.zalsa()),
                indirect_consumer::fn_ingredient_(&db, db.zalsa()),
                input,
                first_revision,
                true,
            );
            indirect_consumer::accumulated::<Warning>(&db, input)
                .iter()
                .map(|warning| warning.0)
                .collect::<Vec<_>>()
        } else {
            check(
                &db,
                direct::fn_ingredient_(&db, db.zalsa()),
                direct_consumer::fn_ingredient_(&db, db.zalsa()),
                input,
                first_revision,
                true,
            );
            direct_consumer::accumulated::<Warning>(&db, input)
                .iter()
                .map(|warning| warning.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(warnings, [7]);
        assert_eq!(db.counts.bodies(), before);
    }
}
