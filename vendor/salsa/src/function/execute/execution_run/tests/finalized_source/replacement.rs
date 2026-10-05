use super::*;
use crate::attempt_probe::Incomplete;
use crate::function::execute::execution_run::{
    Driver, Endpoint, ExecutionProvider, RunOperation, execute_owned,
};
use crate::function::{ClaimResult, Reentrancy};
use crate::prepared_source_probe::{self, Stamp};

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn replaceable(db: &dyn Db, input: Number) -> u32 {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    input.value(db)
}

struct Body;
impl<'run, 'db: 'run, C> ExecutionProvider<'run, 'db, C> for Body
where
    C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(execution, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        db: &'db dyn Db,
        input: Number,
        _: Endpoint<'run, 'db>,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        db.counts().source.fetch_add(1, Ordering::Relaxed);
        Ok(input.value(db))
    }
    async fn initial<'call>(
        &'call self,
        _: &'db dyn Db,
        _: Id,
        _: Number,
        _: Endpoint<'run, 'db>,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
    async fn recover<'call>(
        &'call self,
        _: &'db dyn Db,
        _: &'call Cycle<'call>,
        _: &'call u32,
        _: u32,
        _: Number,
        _: Endpoint<'run, 'db>,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn replace<'db, C>(
    db: &'db dyn Db,
    ingredient: &'db IngredientImpl<C>,
    input: Number,
    old: &'db Memo<C>,
) -> RunResult<()>
where
    C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    Driver::run(db, move |endpoint| async move {
        let operation = RunOperation::enter::<C>(endpoint.context.clone())?;
        let claim = match ingredient.sync_table.try_claim(
            db.zalsa(),
            db.zalsa_local(),
            input.as_id(),
            Reentrancy::Deny,
        ) {
            ClaimResult::Claimed(claim) => claim,
            _ => return Err(RunError::Contract("the prepared source is not claimed")),
        };
        let replaced = execute_owned(
            endpoint,
            &operation,
            ingredient,
            db,
            claim,
            Some(old),
            crate::function::execute::participant::Consumer::capture(db.zalsa_local()),
            &Body,
        )
        .await?
        .ok_or(RunError::RequiresFetch)?;
        assert!(!std::ptr::eq(old, replaced));
        assert_eq!(old.value(), replaced.value());
        assert_eq!(
            old.header.revisions.changed_at,
            replaced.header.revisions.changed_at
        );
        Ok(())
    })
}

#[derive(Clone, Copy)]
enum Point {
    Registration,
    Read,
    Validation,
}

#[test]
fn finalized_source_rejects_equal_values_from_a_different_allocation() {
    for point in [Point::Registration, Point::Read, Point::Validation] {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        assert_eq!(replaceable(&db, input), 7);
        let ingredient = replaceable::fn_ingredient_(&db, db.zalsa());
        let old = stored(&db, ingredient, input.as_id());
        let certificate =
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
        let key = certificate.database_key();
        let stamp = Stamp::current(&db);
        let admission = Admission::default();
        let captured = prepared_source_probe::capture(&db, || {
            try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &admission)?;
                if matches!(point, Point::Registration) {
                    replace(&db, ingredient, input, old)?;
                    assert!(matches!(
                        registry.register_final_source(&db as &dyn Db, ingredient, &[certificate]),
                        Err(RunError::Contract("final source memo was replaced"))
                    ));
                    return Ok(());
                }
                let route =
                    registry.register_final_source(&db as &dyn Db, ingredient, &[certificate])?;
                let registry = registry.seal()?;
                replace(&db, ingredient, input, old)?;
                let revision = db.zalsa().current_revision();
                let result = registry.run(move |endpoint| async move {
                    if matches!(point, Point::Validation) {
                        endpoint.validate(key, revision)?.await?;
                    } else {
                        endpoint.read_final_source(&route, input.as_id()).await;
                    }
                    Ok(())
                });
                assert_eq!(
                    result,
                    Err(RunError::Contract("final source memo was replaced"))
                );
                result
            })
        })
        .unwrap();
        assert_eq!(
            captured.value,
            if matches!(point, Point::Registration) {
                Ok(AttemptOutcome::Complete(Ok(())))
            } else {
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            }
        );
        assert!(!captured.reads.iter().any(|read| read.key == key));
        assert!(!std::ptr::eq(old, stored(&db, ingredient, input.as_id())));
        assert_eq!(db.counts.bodies(), (2, 0));
        assert!(stamp.belongs_to(&db));
        assert_idle(&db);
        let fresh = FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
        assert_eq!(
            try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &admission)?;
                let route = registry.register_final_source(&db as &dyn Db, ingredient, &[fresh])?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(*endpoint.read_final_source(&route, input.as_id()).await)
                })
            }),
            Ok(AttemptOutcome::Complete(Ok(7)))
        );
        assert_eq!(db.counts.bodies(), (2, 0));
    }
}
