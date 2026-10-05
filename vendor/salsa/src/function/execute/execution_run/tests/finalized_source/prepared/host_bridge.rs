use super::*;
use crate::execution_probe::{BorrowOrCopy, Demand, TaskEndpoint};

#[crate::input(field_requests = read_fields)]
struct Input {
    #[returns(copy)]
    enabled: bool,
    #[returns(copy)]
    extra: u32,
}

#[crate::tracked(returns(ref), attempt = CompleteOnly)]
fn source(db: &dyn Db, input: Input) -> u32 {
    db.counts().source.fetch_add(1, Ordering::Relaxed);
    input.extra(db)
}

fn ordinary_read(db: &dyn Db, input: Input) -> u32 {
    let enabled = input.enabled(db);
    let value = source(db, input);
    if enabled {
        let _ = input.extra(db);
    }
    let _ = input.enabled(db);
    *value
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn ordinary_consumer(db: &dyn Db, input: Input) -> u32 {
    ordinary_read(db, input)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn consumer(db: &dyn Db, input: Input) -> u32 {
    ordinary_read(db, input)
}

trait HostReads<'db> {
    fn read<'run>(&'run self, endpoint: TaskEndpoint<'run, 'db>) -> RunResult<Demand<&'db u32>>
    where
        'db: 'run;
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    Refuse,
    Cancel,
}

struct Host<'db> {
    db: &'db dyn Db,
    input: Input,
    source: PreparedSourceMemo<'db, u32>,
    recipient: Option<DatabaseKeyIndex>,
    fault: Fault,
    reads: Cell<usize>,
}

impl Host<'_> {
    fn check_recipient(&self) {
        assert_eq!(
            self.db.zalsa_local().try_with_query_stack(|stack| {
                stack.last().map(|query| query.database_key_index)
            }),
            Some(self.recipient),
        );
    }
}

impl<'db> HostReads<'db> for Host<'db> {
    fn read<'run>(&'run self, endpoint: TaskEndpoint<'run, 'db>) -> RunResult<Demand<&'db u32>>
    where
        'db: 'run,
    {
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            self.check_recipient();
            self.reads.set(self.reads.get() + 1);
            let enabled = child
                .read_field(self.input.read_fields(self.db).enabled(), &BorrowOrCopy)
                .await;
            let value = child
                .read_prepared_source(source::prepared_read(&self.source))
                .await;
            self.check_recipient();
            child
                .local_call(|| match self.fault {
                    Fault::None => Ok(()),
                    Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
                    Fault::Cancel => {
                        self.db.cancellation_token().cancel();
                        Ok(())
                    }
                })
                .await;
            if enabled {
                child
                    .read_field(self.input.read_fields(self.db).extra(), &BorrowOrCopy)
                    .await;
            }
            child
                .read_field(self.input.read_fields(self.db).enabled(), &BorrowOrCopy)
                .await;
            self.check_recipient();
            Ok(value)
        })
    }
}

struct Consumer<'run, 'db> {
    host: &'run dyn HostReads<'db>,
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Consumer<'run, 'db>
where
    C: Configuration<DbView = dyn Db, Input<'db> = Input, Output<'db> = u32>,
{
    // Input conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _input: Input,
    ) -> RunResult<u32> {
        let endpoint = context.endpoint();
        endpoint.local_call(|| endpoint.admit_work(4)).await;
        let value = endpoint
            .child_call(|| async { self.host.read(endpoint.clone())?.await })
            .await;
        Ok(*value)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _id: Id,
        _input: Input,
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
        _input: Input,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn run<'db>(db: &'db dyn Db, input: Input, host: &dyn HostReads<'db>) -> RunResult<u32> {
    let admission = Admission::default();
    let provider = Consumer { host };
    let mut registry = RegistryBuilder::new(db, &admission)?;
    let route = registry.reserve(db, consumer::fn_ingredient_(db, db.zalsa()))?;
    let binding = registry.provider(&provider)?;
    registry.bind_executable(&route, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&route, input.as_id())?
            .await?)
    })
}

fn host(db: &dyn Db, input: Input, fault: Fault) -> Result<Host<'_>, PreparedSourceError> {
    Ok(Host {
        db,
        input,
        source: source::prepare_memo(db, input)?,
        recipient: Some(consumer::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id())),
        fault,
        reads: Cell::new(0),
    })
}

#[test]
fn demand_host_records_ordered_direct_edges_in_its_semantic_caller()
-> Result<(), PreparedSourceError> {
    for enabled in [false, true] {
        let db = TestDb::default();
        let input = Input::new(&db, enabled, 7);
        assert_eq!(ordinary_consumer(&db, input), 7);
        let host = Rc::new(host(&db, input, Fault::None)?);
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, input, &*host)),
            Ok(AttemptOutcome::Complete(Ok(7))),
        );
        assert_eq!(host.reads.get(), 1);
        assert_eq!(db.counts.source.load(Ordering::Relaxed), 1);

        let ordinary = stored(
            &db,
            ordinary_consumer::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        );
        let controlled = stored(
            &db,
            consumer::fn_ingredient_(&db, db.zalsa()),
            input.as_id(),
        );
        let owner = Input::ingredient(&db).database_key_index(input.as_id());
        let mut expected = vec![
            DatabaseKeyIndex::new(owner.ingredient_index().successor(0), input.as_id()),
            host.source.database_key(),
        ];
        if enabled {
            expected.push(DatabaseKeyIndex::new(
                owner.ingredient_index().successor(1),
                input.as_id(),
            ));
        }
        assert_eq!(
            ordinary.header.origin().inputs().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            controlled.header.origin().inputs().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            controlled.header.revisions.durability,
            ordinary.header.revisions.durability
        );
        assert_eq!(
            controlled.header.revisions.changed_at,
            ordinary.header.revisions.changed_at
        );
        assert!(controlled.header.cycle_heads().is_empty());
        assert!(controlled.header.outputs_are_empty());
        assert_idle(&db);
    }
    Ok(())
}

#[test]
fn demand_host_without_a_query_returns_a_database_borrow() -> Result<(), PreparedSourceError> {
    let db = TestDb::default();
    let input = Input::new(&db, true, 7);
    let expected = source(&db, input);
    let mut host = host(&db, input, Fault::None)?;
    host.recipient = None;
    let host = Rc::new(host);
    let host_ref: &dyn HostReads<'_> = &*host;
    let admission = Admission::default();
    let outcome = try_with_attempt(&db, 100_000, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                endpoint.local_call(|| endpoint.admit_work(4)).await;
                Ok(endpoint
                    .child_call(|| async { host_ref.read(endpoint.clone())?.await })
                    .await)
            })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(expected))));
    assert_eq!(host.reads.get(), 1);
    drop(host);
    assert!(
        matches!(outcome, Ok(AttemptOutcome::Complete(Ok(value))) if std::ptr::eq(value, expected))
    );
    assert_idle(&db);
    Ok(())
}

#[test]
fn demand_host_refusal_and_cancellation_leave_its_parent_unpublished()
-> Result<(), PreparedSourceError> {
    for fault in [Fault::Refuse, Fault::Cancel] {
        let db = TestDb::default();
        let input = Input::new(&db, true, 7);
        source(&db, input);
        let stamp = Stamp::current(&db);
        let mut host = host(&db, input, fault)?;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || run(&db, input, &host))
        }));
        db.zalsa_local().uncancel();
        match fault {
            Fault::Refuse => assert!(matches!(
                outcome,
                Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))),
            )),
            Fault::Cancel => assert!(matches!(
                outcome,
                Err(payload) if matches!(payload.downcast_ref::<crate::Cancelled>(), Some(crate::Cancelled::Local)),
            )),
            Fault::None => {}
        }
        let ingredient = consumer::fn_ingredient_(&db, db.zalsa());
        assert!(
            ingredient
                .get_memo_from_table_for(
                    db.zalsa(),
                    input.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
                )
                .is_none()
        );
        assert_eq!(host.reads.get(), 1);
        assert_eq!(Stamp::current(&db), stamp);
        host.source.check_current()?;
        assert_idle(&db);

        host.fault = Fault::None;
        assert_eq!(
            try_with_attempt(&db, 100_000, || run(&db, input, &host)),
            Ok(AttemptOutcome::Complete(Ok(7))),
        );
        assert_eq!(host.reads.get(), 2);
        assert_eq!(db.counts.source.load(Ordering::Relaxed), 1);
        assert_eq!(Stamp::current(&db), stamp);
        assert_idle(&db);
    }
    Ok(())
}
