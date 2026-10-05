use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;

use salsa::execution_probe::{Demand, TaskEndpoint};
use salsa::plumbing::function::{IngredientImpl, InternedQueryConfiguration};

use super::*;
use crate::types::ProtocolInstanceType;
use crate::types::constraints::ConstraintSet;
use crate::types::instance::protocol_object_equivalence_ingredient;
use crate::types::relation::runtime::UnsupportedPairOperation;
use crate::types::relation::runtime::protocol::{
    ProtocolObjectProvider, ProtocolQueries, ProtocolQueryAccess, ProtocolRuntimeObservations,
    ProtocolRuntimeSite, check_protocol_pair_for_test,
};
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallRelationOwners, CallResourceCapacity,
};
use crate::types::typevar::TypeVarSet;

const EMPTY: &str = "from typing import Protocol\nclass Empty(Protocol): ...\n";
const HASH: &str = "from typing import Protocol\nclass P(Protocol):\n    __hash__ = 0\n";
const DATA: &str = "from typing import Protocol\nclass P(Protocol):\n    value = 0\n";

fn protocol<'db>(db: &'db TestDb, prepared: &Prepared<'db>) -> ProtocolInstanceType<'db> {
    Type::instance(
        db,
        &db.program_environment(),
        prepared.root.identity_specialization(db),
    )
    .as_protocol_instance()
    .expect("the fixture declares a protocol")
}

fn existing_object_id<'db, C>(
    db: &'db TestDb,
    _ingredient: &'db IngredientImpl<C>,
    protocol: ProtocolInstanceType<'db>,
) -> Option<salsa::Id>
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (ProtocolInstanceType<'a>, ()),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>,
{
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| *entry.value().fields() == (protocol, ()));
    let id = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    id
}

#[derive(Clone, Copy)]
enum ObjectRequest {
    Query,
    Pair,
    Repeated(usize),
}

#[derive(Debug)]
struct CleanupSnapshot {
    comparison_scopes: usize,
    comparison_drops: usize,
    pools_dropped: [bool; 3],
    record_count: usize,
    owners: Option<[[*const (); 6]; 2]>,
}

#[derive(Default)]
struct ObjectCleanup {
    fired: Cell<usize>,
    child_factory_ran: Cell<bool>,
    after_request: Cell<bool>,
    pools_dropped: [Cell<bool>; 3],
    snapshot: RefCell<Option<CleanupSnapshot>>,
    journal: RefCell<Vec<&'static str>>,
}

struct PoolDrop<'a> {
    cleanup: Option<&'a ObjectCleanup>,
    index: usize,
    name: &'static str,
}

impl Drop for PoolDrop<'_> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup {
            cleanup.pools_dropped[self.index].set(true);
            cleanup.journal.borrow_mut().push(self.name);
        }
    }
}

struct RootDrop<'a>(&'a ObjectCleanup);

impl Drop for RootDrop<'_> {
    fn drop(&mut self) {
        self.0.journal.borrow_mut().push("root");
    }
}

struct QueuedObserver<'a> {
    cleanup: &'a ObjectCleanup,
    observations: &'a ProtocolRuntimeObservations,
}

impl Drop for QueuedObserver<'_> {
    fn drop(&mut self) {
        let records = self.observations.records.borrow();
        *self.cleanup.snapshot.borrow_mut() = Some(CleanupSnapshot {
            comparison_scopes: self.observations.comparison_scopes.get(),
            comparison_drops: self.observations.comparison_drops.get(),
            pools_dropped: std::array::from_fn(|index| self.cleanup.pools_dropped[index].get()),
            record_count: records.len(),
            owners: records
                .get(..2)
                .map(|records| [records[0].owners, records[1].owners]),
        });
        self.cleanup.journal.borrow_mut().push("child");
    }
}

struct ObjectAdmission<'run, 'db: 'run> {
    work: &'run RefCell<Vec<ExecutionWork>>,
    observations: &'run ProtocolRuntimeObservations,
    cleanup: Option<&'run ObjectCleanup>,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
}

impl ExecutionAdmission for ObjectAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.work.borrow_mut().push(work);
        let Some(cleanup) = self.cleanup else {
            return Ok(());
        };
        if cleanup.fired.get() != 0 || !matches!(work, ExecutionWork::Task { .. }) {
            return Ok(());
        }
        let records = self.observations.records.borrow();
        if !matches!(records.as_slice(), [object, direct]
            if object.site == ProtocolRuntimeSite::Object && direct.site == ProtocolRuntimeSite::Direct)
        {
            return Ok(());
        }
        drop(records);
        cleanup.fired.set(cleanup.fired.get() + 1);
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract(
                "object cleanup admission has no endpoint",
            ))?;
        let observer = QueuedObserver {
            cleanup,
            observations: self.observations,
        };
        let pending = endpoint.demand(move || {
            cleanup.child_factory_ran.set(true);
            async move {
                let _observer = observer;
                Ok(())
            }
        })?;
        *self.pending.borrow_mut() = Some(pending);
        Err(RunError::Refused(
            salsa::attempt_probe::Incomplete::Allowance,
        ))
    }
}

struct ResetSlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}

impl Drop for ResetSlots<'_, '_, '_> {
    fn drop(&mut self) {
        let pending = self.pending.borrow_mut().take();
        let endpoint = self.endpoint.borrow_mut().take();
        drop(pending);
        drop(endpoint);
    }
}

struct ObjectAttemptRecord {
    outcome: Result<RunResult<bool>, Incomplete>,
    raw_error: Option<RunError>,
    reads: Vec<Read>,
    stamp: Stamp,
    preparation: Vec<salsa::Event>,
    events: Vec<salsa::Event>,
    work: Vec<ExecutionWork>,
    observations: ProtocolRuntimeObservations,
    root_reads: Result<(), prepared_source_probe::CaptureError>,
}

impl ObjectAttemptRecord {
    fn complete(&self) -> bool {
        assert!(self.raw_error.is_none());
        match self.outcome {
            Ok(Ok(value)) => value,
            ref other => panic!("object query did not complete: {other:?}"),
        }
    }

    fn refused(&self, reason: Incomplete) {
        assert_eq!(self.outcome, Err(reason));
        assert_eq!(
            self.raw_error,
            Some(RunError::Refused(if reason == Incomplete::Allowance {
                salsa::attempt_probe::Incomplete::Allowance
            } else {
                salsa::attempt_probe::Incomplete::Interrupted
            }))
        );
        assert!(
            self.reads
                .iter()
                .all(|read| read.status == Status::Final && read.stamp == self.stamp)
        );
    }

    fn address(&self, parent: Option<DatabaseKeyIndex>, key: DatabaseKeyIndex) -> usize {
        let addresses = self
            .reads
            .iter()
            .filter(|read| read.parent == parent && read.key == key)
            .map(|read| read.memo_address)
            .collect::<HashSet<_>>();
        assert_eq!(addresses.len(), 1);
        *addresses.iter().next().unwrap()
    }
}

fn run_object<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    protocol: ProtocolInstanceType<'db>,
    allowance: usize,
) -> ObjectAttemptRecord {
    run_object_request(
        db,
        prepared,
        protocol,
        allowance,
        ObjectRequest::Query,
        true,
    )
}

fn run_object_request<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    protocol: ProtocolInstanceType<'db>,
    allowance: usize,
    request: ObjectRequest,
    include_definitions: bool,
) -> ObjectAttemptRecord {
    run_object_request_with_cleanup(
        db,
        prepared,
        protocol,
        allowance,
        request,
        include_definitions,
        None,
    )
}

fn run_object_request_with_cleanup<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    protocol: ProtocolInstanceType<'db>,
    allowance: usize,
    request: ObjectRequest,
    include_definitions: bool,
    cleanup: Option<&ObjectCleanup>,
) -> ObjectAttemptRecord {
    let pt = place_table_ingredient(db);
    let ud = use_def_map_ingredient(db);
    let di = definition_inference_ingredient(db);
    let gc = static_class_generic_context_ingredient(db);
    let eb = explicit_bases_ingredient(db);
    let pc = pep695_generic_context_ingredient(db);
    let kc = known_class_to_class_literal_ingredient(db);
    let cm = try_mro_unspecialized_ingredient(db);
    let ci = protocol_interface_ingredient(db);
    let co = protocol_object_equivalence_ingredient(db);
    let places = sorted(
        prepared
            .classes
            .iter()
            .map(|class| {
                FinalSourceMemo::certify(
                    db as &dyn ty_python_core::Db,
                    pt,
                    class.body_scope(db).as_id(),
                )
                .unwrap()
            })
            .collect(),
    );
    let uses = sorted(
        prepared
            .classes
            .iter()
            .map(|class| {
                FinalSourceMemo::certify(
                    db as &dyn ty_python_core::Db,
                    ud,
                    class.body_scope(db).as_id(),
                )
                .unwrap()
            })
            .collect(),
    );
    let definitions = sorted(
        prepared
            .definitions
            .iter()
            .map(|definition| {
                FinalSourceMemo::certify(db as &dyn Db, di, definition.as_id()).unwrap()
            })
            .collect(),
    );
    let contexts = sorted(
        prepared
            .classes
            .iter()
            .map(|class| FinalSourceMemo::certify(db as &dyn Db, gc, class.as_id()).unwrap())
            .collect(),
    );
    let bases = sorted(
        prepared
            .classes
            .iter()
            .map(|class| FinalSourceMemo::certify(db as &dyn Db, eb, class.as_id()).unwrap())
            .collect(),
    );
    let object = FinalSourceMemo::certify(db as &dyn Db, kc, prepared.object_id).unwrap();
    let observations = ProtocolRuntimeObservations::default();
    let work = RefCell::new(Vec::new());
    let capacity = CallResourceCapacity {
        calls: if matches!(request, ObjectRequest::Pair) {
            NonZeroUsize::new(2).unwrap()
        } else {
            NonZeroUsize::MIN
        },
    };
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let raw_error = Cell::new(None);
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, allowance, || {
            let sources;
            let object_keys;
            // The reset guard clears both slots before the borrowed admission and pools retire.
            // Suppressing the endpoint slot's automatic destructor avoids a drop-check cycle.
            let admission;
            let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
            let pending = RefCell::new(None);
            admission = ObjectAdmission {
                work: &work,
                observations: &observations,
                cleanup,
                endpoint: &endpoint_slot,
                pending: &pending,
            };
            // Each marker is declared before its real pool, so it records that pool's destruction
            // only after the pool has dropped. The markers do not own or borrow the pools.
            let _environments_drop = PoolDrop {
                cleanup,
                index: 0,
                name: "environments",
            };
            let environments = CallEnvironments::with_capacity(capacity);
            let _builders_drop = PoolDrop {
                cleanup,
                index: 1,
                name: "builders",
            };
            let builders = CallBuilders::with_capacity(capacity);
            let _owners_drop = PoolDrop {
                cleanup,
                index: 2,
                name: "owners",
            };
            let owners = CallRelationOwners::with_capacity(capacity);
            let reset = ResetSlots {
                endpoint: &endpoint_slot,
                pending: &pending,
            };
            let mut registry = RegistryBuilder::new(db, &admission)?;
            sources = ProtocolSources {
                places: registry.register_final_source(
                    db as &dyn ty_python_core::Db,
                    pt,
                    &places,
                )?,
                uses: registry.register_final_source(db as &dyn ty_python_core::Db, ud, &uses)?,
                definitions: if definitions.is_empty() || !include_definitions {
                    None
                } else {
                    Some(registry.register_final_source(db as &dyn Db, di, &definitions)?)
                },
                contexts: registry.register_final_source(db as &dyn Db, gc, &contexts)?,
                bases: registry.register_final_source(db as &dyn Db, eb, &bases)?,
                pep695: absent_source(pc),
                object: registry.register_final_source(db as &dyn Db, kc, &[object])?,
                object_program: prepared.program,
                object_id: prepared.object_id,
            };
            let mro_route = registry.reserve_callable(db as &dyn Db, cm)?;
            let interface_route = registry.reserve_callable(db as &dyn Db, ci)?;
            let object_route = registry.reserve_callable(db as &dyn Db, co)?;
            object_keys = registry.fixed_callable_query_keys(&object_route)?;
            registry.bind_callable(
                &mro_route,
                ProtocolMroProvider {
                    route: mro_route.clone(),
                    sources: &sources,
                },
            )?;
            let values = registry.finite_interned_values(
                db as &dyn Db,
                ProtocolInterface::ingredient(db.zalsa()),
                protocol_interface_memo_ingredient(db),
            )?;
            registry.bind_callable(
                &interface_route,
                ProtocolInterfaceProvider {
                    sources: &sources,
                    mro_route,
                    values,
                },
            )?;
            let queries = ProtocolQueries {
                db,
                object: object_route.clone(),
                interface: interface_route,
                object_keys: &object_keys,
                members: crate::types::member_lookup::runtime::NoMemberQueries,
            };
            registry.bind_callable(
                &object_route,
                ProtocolObjectProvider {
                    queries: queries.clone(),
                    environments: &environments,
                    builders: &builders,
                    owners: &owners,
                    observations: Some(&observations),
                },
            )?;
            let environment_pool = &environments;
            let builder_pool = &builders;
            let owner_pool = &owners;
            let observations = &observations;
            let endpoint_slot = &endpoint_slot;
            let result = registry.seal()?.run(move |endpoint| {
                if cleanup.is_some() {
                    **endpoint_slot.borrow_mut() = Some(endpoint.clone());
                }
                async move {
                    let _root = cleanup.map(RootDrop);
                    let value = match request {
                        ObjectRequest::Query => {
                            queries.object_equivalence(&endpoint, protocol).await
                        }
                        ObjectRequest::Repeated(requests) => {
                            let mut result = false;
                            for _ in 0..requests {
                                result = queries.object_equivalence(&endpoint, protocol).await?;
                            }
                            Ok(result)
                        }
                        ObjectRequest::Pair => {
                            let env = environment_pool.allocate(&endpoint, prepared.program).await;
                            let builder = builder_pool.allocate(&endpoint).await;
                            let relation = owner_pool.allocate(&endpoint, env, builder).await;
                            let checker = relation.assignability(TypeVarSet::None);
                            let expected = ConstraintSet::from_bool(builder, true);
                            for _ in 0..2 {
                                let result = check_protocol_pair_for_test(
                                    db,
                                    endpoint.clone(),
                                    checker.clone(),
                                    Type::object(),
                                    Type::ProtocolInstance(protocol),
                                    queries.clone(),
                                    Some(observations),
                                )
                                .await?;
                                assert!(result.ownership_probe_same_set(expected));
                            }
                            Ok(true)
                        }
                    }?;
                    if let Some(cleanup) = cleanup {
                        cleanup.after_request.set(true);
                    }
                    Ok(value)
                }
            });
            drop(reset);
            raw_error.set(result.as_ref().err().copied());
            result
        })
    })
    .unwrap();
    assert_eq!(observations.comparison_scopes.get(), 0);
    assert_eq!(
        observations.comparison_drops.get(),
        observations
            .records
            .borrow()
            .iter()
            .filter(|record| record.site == ProtocolRuntimeSite::Object)
            .count(),
    );
    let root_reads = captured.check_root_reads();
    ObjectAttemptRecord {
        outcome: captured.value.0,
        raw_error: raw_error.get(),
        reads: captured.reads,
        stamp: captured.stamp,
        preparation,
        events: reader.take_salsa_events(),
        work: work.into_inner(),
        observations,
        root_reads,
    }
}

#[test]
fn cold_empty_object_query_uses_actual_interface_and_comparison_owners() -> anyhow::Result<()> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    assert!(prepared.definitions.is_empty());
    let protocol = protocol(&db, &prepared);
    let ci = protocol_interface_ingredient(&db);
    let co = protocol_object_equivalence_ingredient(&db);
    // Absence of the real argument tuple also excludes a memo for this operand. The first
    // canonical key is created by the consuming request, under the installed allowance.
    assert_eq!(existing_object_id(&db, co, protocol), None);
    assert_interface_missing(&db, &prepared);
    let warm_mro = mro_final(&db, prepared.root);
    let captured = run_object(&db, &prepared, protocol, 100_000);
    assert!(
        matches!(captured.outcome, Ok(Ok(true))),
        "{:?}",
        captured.outcome
    );
    captured.root_reads.unwrap();
    let events = &captured.events;
    let id = existing_object_id(&db, co, protocol)
        .expect("the actual request created its argument tuple");
    let co_key = co.database_key_index(id);
    let ci_key = interface_key(&db, &prepared);
    let class = class_keys(&db, prepared.root);
    let expected_executions = if warm_mro {
        vec![co_key, ci_key]
    } else {
        vec![co_key, ci_key, class.mro]
    };
    assert_eq!(executed(events), expected_executions);
    assert_query_reads(&captured.reads, None, &[co_key]);
    let interface_reads = captured
        .reads
        .iter()
        .filter(|read| read.parent == Some(co_key))
        .collect::<Vec<_>>();
    assert_eq!(
        interface_reads
            .iter()
            .map(|read| read.key)
            .collect::<Vec<_>>(),
        [ci_key, ci_key, ci_key]
    );
    assert!(
        interface_reads
            .iter()
            .all(|read| read.memo_address == interface_reads[0].memo_address)
    );
    assert_query_reads(
        &captured.reads,
        Some(ci_key),
        &[
            class.mro,
            class.context,
            class.bases,
            class.uses,
            class.places,
        ],
    );
    let mro_sources = [class.context, class.bases, object_key(&db, &prepared)];
    assert_query_reads(
        &captured.reads,
        Some(class.mro),
        if warm_mro { &[] } else { &mro_sources },
    );
    assert_parents(&captured.reads, &[co_key, ci_key, class.mro]);
    assert!(
        captured
            .reads
            .iter()
            .all(|read| read.status == Status::Final && read.stamp == captured.stamp)
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, co, id)
            .unwrap()
            .database_key(),
        co_key
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            ci,
            ClassType::NonGeneric(prepared.root.into()).as_id()
        )
        .unwrap()
        .database_key(),
        ci_key
    );

    let observations = &captured.observations;
    let records = observations.records.borrow();
    assert_eq!(
        records.iter().map(|record| record.site).collect::<Vec<_>>(),
        [
            ProtocolRuntimeSite::Object,
            ProtocolRuntimeSite::Direct,
            ProtocolRuntimeSite::Pair
        ]
    );
    assert!(
        records
            .iter()
            .all(|record| record.owners == records[0].owners)
    );
    assert!(records[0].owners.iter().all(|owner| !owner.is_null()));
    assert_eq!(observations.object_bodies.get(), 1);
    assert_eq!(observations.preflights.get(), 1);
    assert_eq!(observations.member_advances.get(), 2);
    assert_eq!(observations.terminal_checks.get(), 2);
    assert!(
        captured
            .work
            .iter()
            .any(|work| matches!(work, ExecutionWork::Work { units } if *units > 0))
    );
    assert!(captured.stamp.belongs_to(&db));
    assert!(!expansion_probe::active());

    let ordinary_db = database(EMPTY)?;
    let ordinary_prepared = prepare(&ordinary_db, &[("Empty", &[])], false)?;
    let ordinary = Type::instance(
        &ordinary_db,
        &ordinary_db.program_environment(),
        ordinary_prepared.root.identity_specialization(&ordinary_db),
    )
    .as_protocol_instance()
    .expect("the independent fixture declares a protocol");
    assert!(ordinary.is_equivalent_to_object(&ordinary_db));
    eprintln!(
        "EMPTY_OBJECT cold_key=true cold_co=true cold_ci=true warm_mro={warm_mro} executions={expected_executions:?} owners={records:?} work={:?}",
        captured.work
    );
    Ok(())
}

fn object_query_key<'db>(db: &'db TestDb, protocol: ProtocolInstanceType<'db>) -> DatabaseKeyIndex {
    let ingredient = protocol_object_equivalence_ingredient(db);
    ingredient.database_key_index(existing_object_id(db, ingredient, protocol).unwrap())
}

fn has_final_memo<C: Configuration>(
    result: Result<FinalSourceMemo<'_, C>, FinalSourceError>,
) -> bool {
    match result {
        Ok(_) => true,
        Err(
            FinalSourceError::MissingMemo
            | FinalSourceError::MissingValue
            | FinalSourceError::ProvisionalMemo
            | FinalSourceError::IncompleteMemo,
        ) => false,
        other => panic!("same-revision memo must be final or unavailable: {other:?}"),
    }
}

fn object_final<'db>(db: &'db TestDb, protocol: ProtocolInstanceType<'db>) -> bool {
    let ingredient = protocol_object_equivalence_ingredient(db);
    existing_object_id(db, ingredient, protocol)
        .is_some_and(|id| has_final_memo(FinalSourceMemo::certify(db as &dyn Db, ingredient, id)))
}

fn interface_final<'db>(db: &'db TestDb, prepared: &Prepared<'db>) -> bool {
    has_final_memo(FinalSourceMemo::certify(
        db as &dyn Db,
        protocol_interface_ingredient(db),
        ClassType::NonGeneric(prepared.root.into()).as_id(),
    ))
}

fn ordinary_object(source: &str, name: &str, members: &[&str]) -> anyhow::Result<bool> {
    let db = database(source)?;
    let prepared = prepare(&db, &[(name, members)], false)?;
    Ok(protocol(&db, &prepared).is_equivalent_to_object(&db))
}

fn assert_warm_object(
    record: &ObjectAttemptRecord,
    key: DatabaseKeyIndex,
    expected: bool,
    stamp: Stamp,
) {
    assert_eq!(record.complete(), expected);
    record.root_reads.unwrap();
    assert_eq!(record.stamp, stamp);
    assert!(executed(&record.preparation).is_empty());
    assert!(executed(&record.events).is_empty());
    assert_query_reads(&record.reads, None, &[key]);
    assert!(record.reads.iter().all(|read| read.parent.is_none()));
    assert_eq!(record.observations.object_bodies.get(), 0);
    assert!(record.observations.records.borrow().is_empty());
}

#[test]
fn completed_object_query_reuses_the_actual_selected_memo() -> anyhow::Result<()> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    let protocol = protocol(&db, &prepared);
    let first = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(first.complete());
    let key = object_query_key(&db, protocol);
    let address = first.address(None, key);
    for _ in 0..2 {
        let warm = run_object(&db, &prepared, protocol, usize::MAX);
        assert_warm_object(&warm, key, true, first.stamp);
        assert_eq!(warm.address(None, key), address);
    }
    Ok(())
}

#[test]
fn refused_pair_admission_retains_the_actual_object_comparison_until_child_cleanup()
-> anyhow::Result<()> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    let protocol = protocol(&db, &prepared);
    assert_eq!(
        existing_object_id(&db, protocol_object_equivalence_ingredient(&db), protocol),
        None,
    );
    assert_interface_missing(&db, &prepared);
    let warm_mro = mro_final(&db, prepared.root);
    let cleanup = ObjectCleanup::default();
    let failed = run_object_request_with_cleanup(
        &db,
        &prepared,
        protocol,
        usize::MAX,
        ObjectRequest::Query,
        true,
        Some(&cleanup),
    );
    failed.refused(Incomplete::Allowance);
    assert_eq!(cleanup.fired.get(), 1);
    assert!(!cleanup.child_factory_ran.get());
    assert!(!cleanup.after_request.get());
    assert_eq!(
        failed
            .observations
            .records
            .borrow()
            .iter()
            .map(|record| record.site)
            .collect::<Vec<_>>(),
        [ProtocolRuntimeSite::Object, ProtocolRuntimeSite::Direct],
    );
    let snapshot = cleanup.snapshot.borrow();
    let snapshot = snapshot
        .as_ref()
        .expect("the demanded child's factory was destroyed");
    assert_eq!(snapshot.comparison_scopes, 1);
    assert_eq!(snapshot.comparison_drops, 0);
    assert_eq!(snapshot.pools_dropped, [false; 3]);
    assert_eq!(snapshot.record_count, 2);
    let owners = snapshot
        .owners
        .expect("Object and Direct both recorded their actual owners");
    assert_eq!(owners[0], owners[1]);
    assert!(owners[0].iter().all(|owner| !owner.is_null()));
    assert_eq!(failed.observations.comparison_scopes.get(), 0);
    assert_eq!(failed.observations.comparison_drops.get(), 1);
    assert_eq!(
        *cleanup.journal.borrow(),
        ["child", "root", "owners", "builders", "environments"]
    );
    assert!(cleanup.pools_dropped.iter().all(Cell::get));

    let co = object_query_key(&db, protocol);
    let ci = interface_key(&db, &prepared);
    let cm = class_keys(&db, prepared.root).mro;
    let executions = if warm_mro {
        vec![co, ci]
    } else {
        vec![co, ci, cm]
    };
    assert_eq!(executed(&failed.events), executions);
    assert!(
        !failed
            .reads
            .iter()
            .any(|read| read.parent.is_none() && read.key == co)
    );
    assert!(!object_final(&db, protocol));
    assert!(interface_final(&db, &prepared));
    let ci_address = failed.address(Some(co), ci);

    let retry = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(retry.complete());
    retry.root_reads.unwrap();
    assert_eq!(retry.stamp, failed.stamp);
    assert!(executed(&retry.preparation).is_empty());
    assert_eq!(executed(&retry.events), [co]);
    assert_eq!(retry.address(Some(co), ci), ci_address);
    assert!(object_final(&db, protocol));
    let warm = run_object(&db, &prepared, protocol, usize::MAX);
    assert_warm_object(&warm, co, true, failed.stamp);
    assert_eq!(warm.address(None, co), retry.address(None, co));
    Ok(())
}

#[test]
fn hash_member_completes_false_without_comparison_owners() -> anyhow::Result<()> {
    let db = database(HASH)?;
    let prepared = prepare(&db, &[("P", &["__hash__"])], false)?;
    let protocol = protocol(&db, &prepared);
    assert_eq!(
        existing_object_id(&db, protocol_object_equivalence_ingredient(&db), protocol),
        None
    );
    assert_interface_missing(&db, &prepared);
    let result = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(!result.complete());
    assert!(object_final(&db, protocol));
    assert!(interface_final(&db, &prepared));
    let co = object_query_key(&db, protocol);
    let ci = interface_key(&db, &prepared);
    assert_query_reads(&result.reads, None, &[co]);
    assert_query_reads(&result.reads, Some(co), &[ci]);
    let interface = cached_protocol_interface(&db, ClassType::NonGeneric(prepared.root.into()));
    assert!(interface.inner(&db).contains_key("__hash__"));
    assert_eq!(result.observations.object_bodies.get(), 1);
    assert!(result.observations.records.borrow().is_empty());
    assert_eq!(result.observations.preflights.get(), 0);
    assert_eq!(result.observations.member_advances.get(), 0);
    assert_eq!(result.observations.terminal_checks.get(), 0);
    assert!(!ordinary_object(HASH, "P", &["__hash__"])?);
    Ok(())
}

#[test]
fn nonempty_member_refuses_without_publishing_false() -> anyhow::Result<()> {
    let db = database(DATA)?;
    let prepared = prepare(&db, &[("P", &["value"])], false)?;
    let protocol = protocol(&db, &prepared);
    let stamp = Stamp::current(&db);
    let ci = interface_key(&db, &prepared);
    let mut retained_ci = None;
    let mut co = None;
    for iteration in 0..3 {
        let result = run_object(&db, &prepared, protocol, usize::MAX);
        result.refused(Incomplete::UnsupportedPairOperation(
            UnsupportedPairOperation::Protocol,
        ));
        assert_eq!(result.stamp, stamp);
        let key = object_query_key(&db, protocol);
        assert_eq!(*co.get_or_insert(key), key);
        assert!(!object_final(&db, protocol));
        assert!(interface_final(&db, &prepared));
        assert!(
            !result
                .reads
                .iter()
                .any(|read| read.parent.is_none() && read.key == key)
        );
        let address = result.address(Some(key), ci);
        assert_eq!(*retained_ci.get_or_insert(address), address);
        assert_eq!(result.observations.preflights.get(), 1);
        assert_eq!(result.observations.member_advances.get(), 1);
        assert_eq!(result.observations.terminal_checks.get(), 1);
        assert_eq!(
            result
                .observations
                .records
                .borrow()
                .iter()
                .map(|record| record.site)
                .collect::<Vec<_>>(),
            [
                ProtocolRuntimeSite::Object,
                ProtocolRuntimeSite::Direct,
                ProtocolRuntimeSite::Pair
            ]
        );
        if iteration > 0 {
            assert!(executed(&result.preparation).is_empty());
            assert_eq!(executed(&result.events), [key]);
        }
    }
    let interface = cached_protocol_interface(&db, ClassType::NonGeneric(prepared.root.into()));
    assert_eq!(
        interface
            .inner(&db)
            .keys()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        ["value"]
    );
    assert!(!ordinary_object(DATA, "P", &["value"])?);
    assert!(!protocol.is_equivalent_to_object(&db));
    assert!(object_final(&db, protocol));
    Ok(())
}

#[test]
fn missing_definition_capability_refuses_and_retries_with_real_sources() -> anyhow::Result<()> {
    let db = database(HASH)?;
    let prepared = prepare(&db, &[("P", &["__hash__"])], false)?;
    let protocol = protocol(&db, &prepared);
    let failed = run_object_request(
        &db,
        &prepared,
        protocol,
        usize::MAX,
        ObjectRequest::Query,
        false,
    );
    failed.refused(Incomplete::UnsupportedProtocolInterfaceOperation(
        UnsupportedProtocolInterfaceOperation::DefinitionInference,
    ));
    assert!(!object_final(&db, protocol));
    assert!(!interface_final(&db, &prepared));
    assert!(failed.observations.records.borrow().is_empty());
    let co = object_query_key(&db, protocol);
    let ci = interface_key(&db, &prepared);
    assert!(
        !failed
            .reads
            .iter()
            .any(|read| read.key == definition_key(&db, prepared.definitions[0]))
    );
    let retry = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(!retry.complete());
    assert_eq!(retry.stamp, failed.stamp);
    assert!(executed(&retry.preparation).is_empty());
    assert!(executed(&retry.events).contains(&co));
    assert!(executed(&retry.events).contains(&ci));
    assert!(
        retry
            .reads
            .iter()
            .any(|read| read.key == definition_key(&db, prepared.definitions[0]))
    );
    let warm = run_object(&db, &prepared, protocol, usize::MAX);
    assert_warm_object(&warm, co, false, failed.stamp);
    assert_eq!(warm.address(None, co), retry.address(None, co));
    assert!(!ordinary_object(HASH, "P", &["__hash__"])?);
    Ok(())
}

#[test]
fn enclosing_pair_uses_object_query_and_keeps_its_original_owners() -> anyhow::Result<()> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    let protocol = protocol(&db, &prepared);
    assert_eq!(
        existing_object_id(&db, protocol_object_equivalence_ingredient(&db), protocol),
        None
    );
    assert_interface_missing(&db, &prepared);
    let result = run_object_request(
        &db,
        &prepared,
        protocol,
        usize::MAX,
        ObjectRequest::Pair,
        true,
    );
    assert!(result.complete());
    let co = object_query_key(&db, protocol);
    let ci = interface_key(&db, &prepared);
    assert_eq!(
        executed(&result.events)
            .iter()
            .filter(|key| **key == co)
            .count(),
        1
    );
    assert_eq!(
        executed(&result.events)
            .iter()
            .filter(|key| **key == ci)
            .count(),
        1
    );
    assert_query_reads(&result.reads, None, &[co, co]);
    assert_eq!(
        result
            .reads
            .iter()
            .filter(|read| read.parent.is_none())
            .map(|read| read.key)
            .collect::<Vec<_>>(),
        [co, co]
    );
    let records = result.observations.records.borrow();
    assert_eq!(
        records.iter().map(|record| record.site).collect::<Vec<_>>(),
        [
            ProtocolRuntimeSite::Pair,
            ProtocolRuntimeSite::Object,
            ProtocolRuntimeSite::Direct,
            ProtocolRuntimeSite::Pair,
            ProtocolRuntimeSite::Pair,
        ]
    );
    assert_eq!(records[0].owners, records[4].owners);
    assert_eq!(records[1].owners, records[2].owners);
    assert_eq!(records[1].owners, records[3].owners);
    assert!(
        records[0]
            .owners
            .iter()
            .zip(records[1].owners)
            .all(|(caller, child)| *caller != child)
    );
    assert_eq!(result.observations.object_bodies.get(), 1);
    Ok(())
}

#[test]
fn edited_member_invalidates_the_same_object_and_interface_keys() -> anyhow::Result<()> {
    const BEFORE: &str = "from typing import Protocol\nclass P(Protocol): ...\n";
    let mut db = database(BEFORE)?;
    let (old_co, old_ci, old_stamp) = {
        let prepared = prepare(&db, &[("P", &[])], false)?;
        let protocol = protocol(&db, &prepared);
        let result = run_object(&db, &prepared, protocol, usize::MAX);
        assert!(result.complete());
        (
            object_query_key(&db, protocol),
            interface_key(&db, &prepared),
            result.stamp,
        )
    };
    db.write_file(PATH, HASH)?;
    let prepared = prepare(&db, &[("P", &["__hash__"])], false)?;
    let protocol = protocol(&db, &prepared);
    let co = protocol_object_equivalence_ingredient(&db);
    let id =
        existing_object_id(&db, co, protocol).expect("the edited class retains its argument key");
    assert_eq!(co.database_key_index(id), old_co);
    assert_eq!(interface_key(&db, &prepared), old_ci);
    assert_ne!(Stamp::current(&db), old_stamp);
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, co, id),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    assert!(matches!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            protocol_interface_ingredient(&db),
            ClassType::NonGeneric(prepared.root.into()).as_id(),
        ),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    let edited = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(!edited.complete());
    assert!(
        !executed(&edited.preparation)
            .iter()
            .any(|key| [old_co, old_ci].contains(key))
    );
    assert!(executed(&edited.events).contains(&old_co));
    assert!(executed(&edited.events).contains(&old_ci));
    assert!(!ordinary_object(HASH, "P", &["__hash__"])?);
    let warm = run_object(&db, &prepared, protocol, usize::MAX);
    assert_warm_object(&warm, old_co, false, edited.stamp);
    assert_eq!(warm.address(None, old_co), edited.address(None, old_co));
    Ok(())
}

#[derive(Clone, Copy)]
enum ObjectMilestone {
    Interface,
    Object,
}

struct ColdObjectProbe {
    warm_mro: bool,
    interface: bool,
    object: bool,
    work: usize,
}

fn observed_work(record: &ObjectAttemptRecord) -> usize {
    record
        .work
        .iter()
        .try_fold(0usize, |sum, work| {
            sum.checked_add(match work {
                ExecutionWork::Work { units } => *units,
                _ => 0,
            })
        })
        .expect("the finite fixture's reported work fits usize")
}

fn cold_object_probe(allowance: usize) -> anyhow::Result<ColdObjectProbe> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    let protocol = protocol(&db, &prepared);
    assert_eq!(
        existing_object_id(&db, protocol_object_equivalence_ingredient(&db), protocol),
        None
    );
    assert_interface_missing(&db, &prepared);
    let warm_mro = mro_final(&db, prepared.root);
    let result = run_object(&db, &prepared, protocol, allowance);
    if matches!(result.outcome, Ok(Ok(_))) {
        assert!(result.complete());
    } else {
        result.refused(Incomplete::Allowance);
    }
    Ok(ColdObjectProbe {
        warm_mro,
        interface: interface_final(&db, &prepared),
        object: object_final(&db, protocol),
        work: observed_work(&result),
    })
}

fn first_object_allowance(
    upper: usize,
    warm_mro: bool,
    milestone: ObjectMilestone,
) -> anyhow::Result<usize> {
    let mut low = 0;
    let mut high = upper;
    let mut probes = 0;
    while high - low > 1 {
        probes += 1;
        assert!(probes <= usize::BITS);
        let middle = low + (high - low) / 2;
        // Fresh databases keep earlier successful children from changing a later probe's frontier.
        let result = cold_object_probe(middle)?;
        assert_eq!(result.warm_mro, warm_mro);
        if match milestone {
            ObjectMilestone::Interface => result.interface,
            ObjectMilestone::Object => result.object,
        } {
            high = middle;
        } else {
            low = middle;
        }
    }
    Ok(high)
}

#[test]
fn actual_allowance_preserves_completed_children_and_same_revision_retry() -> anyhow::Result<()> {
    let complete = cold_object_probe(usize::MAX)?;
    assert!(complete.interface && complete.object);
    let upper = complete.work;
    let upper_probe = cold_object_probe(upper)?;
    assert!(
        upper_probe.interface && upper_probe.object,
        "reported work must cover the actual debit"
    );
    assert_eq!(upper_probe.warm_mro, complete.warm_mro);
    let zero = cold_object_probe(0)?;
    assert!(!zero.interface && !zero.object);
    let interface_allowance =
        first_object_allowance(upper, complete.warm_mro, ObjectMilestone::Interface)?;
    let object_allowance =
        first_object_allowance(upper, complete.warm_mro, ObjectMilestone::Object)?;
    assert!(
        0 < interface_allowance
            && interface_allowance < object_allowance
            && object_allowance <= upper
    );

    for (allowance, retained_ci) in [
        (interface_allowance - 1, false),
        (object_allowance - 1, true),
    ] {
        let db = database(EMPTY)?;
        let prepared = prepare(&db, &[("Empty", &[])], false)?;
        let protocol = protocol(&db, &prepared);
        assert_eq!(mro_final(&db, prepared.root), complete.warm_mro);
        assert_eq!(
            existing_object_id(&db, protocol_object_equivalence_ingredient(&db), protocol),
            None
        );
        assert_interface_missing(&db, &prepared);
        let failed = run_object(&db, &prepared, protocol, allowance);
        failed.refused(Incomplete::Allowance);
        assert!(!object_final(&db, protocol));
        assert_eq!(interface_final(&db, &prepared), retained_ci);
        let co = object_query_key(&db, protocol);
        let ci = interface_key(&db, &prepared);
        let cm = class_keys(&db, prepared.root).mro;
        assert!(
            !failed
                .reads
                .iter()
                .any(|read| read.parent.is_none() && read.key == co)
        );
        let ci_address = retained_ci.then(|| failed.address(Some(co), ci));
        let retained_mro = mro_final(&db, prepared.root);
        let mro_address = failed
            .reads
            .iter()
            .find(|read| read.key == cm)
            .map(|read| read.memo_address);
        let retry = run_object(&db, &prepared, protocol, usize::MAX);
        assert!(retry.complete());
        assert_eq!(retry.stamp, failed.stamp);
        assert!(executed(&retry.preparation).is_empty());
        assert_eq!(object_query_key(&db, protocol), co);
        assert_eq!(executed(&retry.events).contains(&ci), !retained_ci);
        assert_eq!(executed(&retry.events).contains(&cm), !retained_mro);
        if let Some(address) = ci_address {
            assert_eq!(retry.address(Some(co), ci), address);
        }
        if let Some(address) = mro_address {
            assert!(retained_mro);
            if !retained_ci {
                assert_eq!(retry.address(Some(ci), cm), address);
            }
        }
        assert!(object_final(&db, protocol));
        assert!(interface_final(&db, &prepared));
        assert!(mro_final(&db, prepared.root));
        let warm = run_object(&db, &prepared, protocol, usize::MAX);
        assert_warm_object(&warm, co, true, failed.stamp);
        assert_eq!(warm.address(None, co), retry.address(None, co));
    }
    Ok(())
}

#[test]
fn warm_object_hits_exhaust_work_without_discarding_the_completed_memo() -> anyhow::Result<()> {
    let db = database(EMPTY)?;
    let prepared = prepare(&db, &[("Empty", &[])], false)?;
    let protocol = protocol(&db, &prepared);
    let initial = run_object(&db, &prepared, protocol, usize::MAX);
    assert!(initial.complete());
    let co = object_query_key(&db, protocol);
    let address = initial.address(None, co);
    let prefix = run_object_request(
        &db,
        &prepared,
        protocol,
        usize::MAX,
        ObjectRequest::Repeated(2),
        true,
    );
    assert!(prefix.complete());
    assert!(executed(&prefix.events).is_empty());
    assert_eq!(prefix.observations.object_bodies.get(), 0);
    assert_query_reads(&prefix.reads, None, &[co, co]);
    assert_eq!(
        prefix.reads.iter().map(|read| read.key).collect::<Vec<_>>(),
        [co, co]
    );
    let allowance = observed_work(&prefix);
    assert!(allowance > 0);
    // Each actual hit admits positive work, so this finite request count exceeds the allowance.
    let requests = allowance.checked_add(1).unwrap();
    let failed = run_object_request(
        &db,
        &prepared,
        protocol,
        allowance,
        ObjectRequest::Repeated(requests),
        true,
    );
    failed.refused(Incomplete::Allowance);
    assert_eq!(failed.stamp, initial.stamp);
    assert!(executed(&failed.events).is_empty());
    assert_eq!(failed.observations.object_bodies.get(), 0);
    assert!(failed.observations.records.borrow().is_empty());
    assert!(!failed.reads.is_empty());
    assert!(failed.reads.len() < requests);
    assert!(
        failed
            .reads
            .iter()
            .all(|read| read.key == co && read.parent.is_none() && read.memo_address == address)
    );
    assert!(object_final(&db, protocol));
    for _ in 0..2 {
        let retry = run_object(&db, &prepared, protocol, usize::MAX);
        assert_warm_object(&retry, co, true, initial.stamp);
        assert_eq!(retry.address(None, co), address);
    }
    Ok(())
}
