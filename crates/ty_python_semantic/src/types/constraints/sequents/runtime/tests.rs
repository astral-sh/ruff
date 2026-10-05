use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, FinalSourceError, FinalSourceMemo, QueryKeyProfile,
    RegistryBuilder,
};
use salsa::plumbing::ZalsaDatabase;
use salsa::prepared_source_probe::{self, Read, Status};
use salsa::{DatabaseKeyIndex, Event, EventKind};

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::possible_assignability_ingredient;
use crate::types::constraints::runtime::satisfaction::ConcreteSolverQueries;
use crate::types::constraints::sequents::{pair_sequent_ingredient, single_sequent_ingredient};
use crate::types::constraints::variables::{
    ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, ConstraintProvenance,
    TypeVarRangeBound,
};
use crate::types::mapping::runtime::{
    MaterializationKeyProfile, MaterializationObservations, MaterializationProvider,
    MaterializationQueries,
};
use crate::types::relation::runtime::constraint_set::{
    OwnedRelationKind, OwnedRelationProvider, OwnedRelationQueries,
};
use crate::types::relation::runtime::protocol::NoProtocolQueries;
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallMappingVisitors, CallRelationOwners, CallResourceCapacity,
};
use crate::types::relation::{
    owned_assignability_ingredient, owned_equivalence_ingredient, redundancy_ingredient,
};
use crate::types::set_theoretic::{
    intersection_from_two_elements_ingredient, union_from_two_elements_ingredient,
};
use crate::types::{
    Type, TypeFormType, cached_materialization_ingredient, register_type_pair_values,
};

struct Admission;
impl ExecutionAdmission for Admission {
    fn admit(&self, _: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Request<'db> {
    Single(Constraint<'db>),
    Pair(Constraint<'db>, Constraint<'db>),
}
struct Record<'db> {
    result: Result<RunResult<Vec<&'db SequentMap<'db>>>, Incomplete>,
    events: Vec<Event>,
    reads: Vec<Read>,
}
fn execute<'db>(db: &'db TestDb, program: Program<'db>, requests: &[Request<'db>]) -> Record<'db> {
    let admission = Admission;
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, usize::MAX, || {
            // The borrowed key tokens and query bundle outlive the registry on every error path.
            let single_keys;
            let pair_keys;
            let queries;
            let mut registry = RegistryBuilder::new(db, &admission)?;
            let single_route =
                registry.reserve_callable(db as &dyn Db, single_sequent_ingredient(db))?;
            let pair_route =
                registry.reserve_callable(db as &dyn Db, pair_sequent_ingredient(db))?;
            single_keys = registry.callable_query_keys::<_, SingleSequentProfile>(&single_route)?;
            pair_keys = registry.callable_query_keys::<_, PairSequentProfile>(&pair_route)?;
            queries = SequentQueries {
                single_route,
                pair_route,
                single_keys: &single_keys,
                pair_keys: &pair_keys,
            };
            registry.bind_callable(
                &queries.single_route,
                SingleSequentProvider {
                    queries: queries.clone(),
                },
            )?;
            registry.bind_callable(
                &queries.pair_route,
                PairSequentProvider {
                    queries: queries.clone(),
                },
            )?;
            let queries = &queries;
            registry.seal()?.run(move |endpoint| async move {
                let mut values = Vec::new();
                for request in requests {
                    values.push(match *request {
                        Request::Single(constraint) => {
                            queries.single(&endpoint, program, constraint).await?
                        }
                        Request::Pair(left, right) => {
                            queries.pair(&endpoint, program, left, right).await?
                        }
                    });
                }
                Ok(values)
            })
        })
    })
    .unwrap();
    if matches!(captured.value.0, Ok(Ok(_))) {
        captured.check_root_reads().unwrap();
    }
    Record {
        result: captured.value.0,
        events: reader.take_salsa_events(),
        reads: captured.reads,
    }
}
fn execute_concrete_pairs<'db>(db: &'db TestDb, requests: &[Request<'db>]) -> Record<'db> {
    let program = db.program_environment().program(db);
    let admission = Admission;
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, usize::MAX, || {
            let capacity = CallResourceCapacity {
                calls: NonZeroUsize::new(32).unwrap(),
            };
            let environments = CallEnvironments::with_capacity(capacity);
            let builders = CallBuilders::with_capacity(capacity);
            let owners = CallRelationOwners::with_capacity(capacity);
            let visitors = CallMappingVisitors::with_capacity(capacity);
            let materialization_observations = MaterializationObservations::default();
            let single_keys;
            let pair_keys;
            let materialization_keys;
            let forms;
            let type_pairs;
            let materialization;
            let owned_relations;
            let queries;
            let mut registry = RegistryBuilder::new(db, &admission)?;
            let single_route =
                registry.reserve_callable(db as &dyn Db, single_sequent_ingredient(db))?;
            let pair_route =
                registry.reserve_callable(db as &dyn Db, pair_sequent_ingredient(db))?;
            let materialization_route =
                registry.reserve_callable(db as &dyn Db, cached_materialization_ingredient(db))?;
            let assignability = owned_assignability_ingredient(db);
            let equivalence = owned_equivalence_ingredient(db);
            let assignability_route = registry.reserve_callable(db as &dyn Db, assignability)?;
            let equivalence_route = registry.reserve_callable(db as &dyn Db, equivalence)?;
            single_keys = registry.callable_query_keys::<_, SingleSequentProfile>(&single_route)?;
            pair_keys = registry.callable_query_keys::<_, PairSequentProfile>(&pair_route)?;
            materialization_keys = registry
                .callable_query_keys::<_, MaterializationKeyProfile>(&materialization_route)?;
            forms = registry
                .finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
            type_pairs = register_type_pair_values(
                db,
                &mut registry,
                assignability,
                equivalence,
                redundancy_ingredient(db),
                possible_assignability_ingredient(db),
                union_from_two_elements_ingredient(db),
                intersection_from_two_elements_ingredient(db),
            )?;
            materialization = MaterializationQueries {
                route: materialization_route,
                keys: &materialization_keys,
            };
            owned_relations = OwnedRelationQueries {
                assignability: assignability_route,
                equivalence: equivalence_route,
                keys: &type_pairs,
            };
            queries = ConcreteSolverQueries {
                sequents: SequentQueries {
                    single_route,
                    pair_route,
                    single_keys: &single_keys,
                    pair_keys: &pair_keys,
                },
                materialization: &materialization,
                environments: &environments,
                builders: &builders,
                owners: &owners,
                protocols: NoProtocolQueries,
                owned_relations: owned_relations.clone(),
                relation_observer: None,
            };
            registry.bind_callable(
                &materialization.route,
                MaterializationProvider {
                    environments: &environments,
                    visitors: &visitors,
                    forms: &forms,
                    observations: &materialization_observations,
                },
            )?;
            registry.bind_callable(
                &owned_relations.assignability,
                OwnedRelationProvider {
                    kind: OwnedRelationKind::Assignability,
                    queries: queries.clone(),
                    environments: &environments,
                    builders: &builders,
                    owners: &owners,
                    mappings: &visitors,
                    observer: None,
                },
            )?;
            registry.bind_callable(
                &owned_relations.equivalence,
                OwnedRelationProvider {
                    kind: OwnedRelationKind::Equivalence,
                    queries: queries.clone(),
                    environments: &environments,
                    builders: &builders,
                    owners: &owners,
                    mappings: &visitors,
                    observer: None,
                },
            )?;
            registry.bind_callable(
                &queries.sequents.single_route,
                SingleSequentProvider {
                    queries: queries.clone(),
                },
            )?;
            registry.bind_callable(
                &queries.sequents.pair_route,
                PairSequentProvider {
                    queries: queries.clone(),
                },
            )?;
            let queries = &queries;
            registry.seal()?.run(move |endpoint| async move {
                let mut values = Vec::new();
                for request in requests {
                    values.push(match *request {
                        Request::Single(constraint) => {
                            queries.single(&endpoint, program, constraint).await?
                        }
                        Request::Pair(left, right) => {
                            queries.pair(&endpoint, program, left, right).await?
                        }
                    });
                }
                Ok(values)
            })
        })
    })
    .unwrap();
    if matches!(captured.value.0, Ok(Ok(_))) {
        captured.check_root_reads().unwrap();
    }
    Record {
        result: captured.value.0,
        events: reader.take_salsa_events(),
        reads: captured.reads,
    }
}

fn variables(db: &TestDb) -> [BoundTypeVarInstance<'_>; 3] {
    ["T", "U", "V"].map(|name| {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    })
}
fn rows<'db>(db: &'db TestDb, [t, u, v]: [BoundTypeVarInstance<'db>; 3]) -> [Request<'db>; 4] {
    let range = |provenance, left, right| {
        Constraint::from(TypeVarRangeBound::new(db, provenance, left, right))
    };
    let tt = range(ConstraintProvenance::Evidence, t, t);
    let tu = range(ConstraintProvenance::Validity, t, u);
    let uv = range(ConstraintProvenance::Evidence, u, v);
    let ut = range(ConstraintProvenance::Evidence, u, t);
    [
        Request::Single(tt),
        Request::Pair(tu, uv),
        Request::Pair(tu, ut),
        Request::Pair(uv, tu),
    ]
}
fn executed(events: &[Event]) -> Vec<DatabaseKeyIndex> {
    events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NormalConstraint {
    equivalence: bool,
    provenance: ConstraintProvenance,
    left: usize,
    right: usize,
}
#[derive(Debug, Eq, PartialEq)]
enum NormalGroup {
    Ungrouped(Vec<Sequent<NormalConstraint>>),
    Grouped {
        equivalence: NormalConstraint,
        leftwards: Vec<Sequent<NormalConstraint>>,
        rightwards: Vec<Sequent<NormalConstraint>>,
    },
}
fn normalize<'db>(
    db: &'db TestDb,
    variables: [BoundTypeVarInstance<'db>; 3],
    map: &SequentMap<'db>,
) -> (Vec<NormalGroup>, Vec<Sequent<NormalConstraint>>) {
    let variable = |value: BoundTypeVarInstance<'db>| {
        variables
            .iter()
            .position(|candidate| candidate.identity(db) == value.identity(db))
            .unwrap()
    };
    let constraint = |value: Constraint<'db>| {
        let (equivalence, provenance, left, right) = match value {
            Constraint::TypeVarRange(bound) => (false, bound.provenance, bound.left, bound.right),
            Constraint::TypeVarEquivalence(bound) => {
                (true, bound.provenance, bound.left, bound.right)
            }
            other => panic!("typevar rule produced a concrete bound: {other:?}"),
        };
        NormalConstraint {
            equivalence,
            provenance,
            left: variable(left),
            right: variable(right),
        }
    };
    let sequent = |value: &Sequent<Constraint<'db>>| match *value {
        Sequent::SingleTautology { ante } => Sequent::SingleTautology {
            ante: constraint(ante),
        },
        Sequent::PairImpossibility { ante1, ante2 } => Sequent::PairImpossibility {
            ante1: constraint(ante1),
            ante2: constraint(ante2),
        },
        Sequent::TripleImpossibility {
            ante1,
            ante2,
            ante3,
        } => Sequent::TripleImpossibility {
            ante1: constraint(ante1),
            ante2: constraint(ante2),
            ante3: constraint(ante3),
        },
        Sequent::SingleImplication {
            ante,
            post,
            fuel_cost,
        } => Sequent::SingleImplication {
            ante: constraint(ante),
            post: constraint(post),
            fuel_cost,
        },
        Sequent::PairImplication {
            ante1,
            ante2,
            post,
            fuel_cost,
        } => Sequent::PairImplication {
            ante1: constraint(ante1),
            ante2: constraint(ante2),
            post: constraint(post),
            fuel_cost,
        },
    };
    let groups = map
        .sequents
        .iter()
        .map(|group| match group {
            SequentGroup::Ungrouped(values) => {
                NormalGroup::Ungrouped(values.iter().map(&sequent).collect())
            }
            SequentGroup::Grouped {
                equivalence,
                leftwards,
                rightwards,
            } => NormalGroup::Grouped {
                equivalence: constraint((*equivalence).into()),
                leftwards: leftwards.iter().map(&sequent).collect(),
                rightwards: rightwards.iter().map(&sequent).collect(),
            },
        })
        .collect();
    (groups, map.pending.iter().map(sequent).collect())
}

#[test]
fn cold_typevar_rules_match_ordinary_order_and_reuse_canonical_memos() {
    let db = setup_db();
    let ordinary = setup_db();
    let vars = variables(&db);
    let ordinary_vars = variables(&ordinary);
    let env = db.program_environment();
    let program = env.program(&db);
    let requests = rows(&db, vars);
    let ordinary_requests = rows(&ordinary, ordinary_vars);
    let record = execute(&db, program, &requests.repeat(2));
    let values = record.result.unwrap().unwrap();
    assert_eq!(values.len(), 8);
    let keys = executed(&record.events);
    assert_eq!(
        keys.len(),
        4,
        "each cold body executes once, with no semantic query escape"
    );
    assert_ne!(
        keys[1], keys[3],
        "reversing the pair preserves ordered canonical identity"
    );
    assert_eq!(
        record
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::DidInternValue { .. }))
            .count(),
        4,
        "all four real argument tuples were cold"
    );
    let actual_reads = record
        .reads
        .iter()
        .map(|read| {
            assert_eq!(read.parent, None);
            assert_eq!(read.status, Status::Final);
            read.key
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_reads, keys.repeat(2));
    for (index, request) in ordinary_requests.into_iter().enumerate() {
        let expected = match request {
            Request::Single(value) => {
                SequentMap::for_constraint(&ordinary, &ordinary.program_environment(), value)
            }
            Request::Pair(left, right) => SequentMap::for_constraint_pair(
                &ordinary,
                &ordinary.program_environment(),
                left,
                right,
            ),
        };
        assert!(!values[index].sequents.is_empty());
        assert!(values[index].pending.is_empty());
        assert_eq!(
            normalize(&db, vars, values[index]),
            normalize(&ordinary, ordinary_vars, expected)
        );
        assert!(std::ptr::eq(values[index], values[index + 4]));
        assert_eq!(
            record.reads[index].memo_address,
            record.reads[index + 4].memo_address
        );
    }
}

fn unequal_pair<'db>(db: &'db TestDb, equivalent: bool, reversed: bool) -> [Constraint<'db>; 2] {
    let [t, ..] = variables(db);
    let first = TypeFormType::from_type_expression(db, Type::int_literal(1));
    let second = TypeFormType::from_type_expression(db, Type::int_literal(2));
    let bounds = if equivalent {
        [
            Constraint::from(ConcreteEquivalenceBound::new(
                ConstraintProvenance::Evidence,
                t,
                first,
            )),
            Constraint::from(ConcreteEquivalenceBound::new(
                ConstraintProvenance::Evidence,
                t,
                second,
            )),
        ]
    } else {
        [
            Constraint::from(ConcreteLowerBound::new(
                ConstraintProvenance::Evidence,
                t,
                first,
            )),
            Constraint::from(ConcreteUpperBound::new(
                ConstraintProvenance::Evidence,
                t,
                second,
            )),
        ]
    };
    if reversed {
        [bounds[1], bounds[0]]
    } else {
        bounds
    }
}

fn assert_impossible_pair<'db>(
    map: &SequentMap<'db>,
    first: Constraint<'db>,
    second: Constraint<'db>,
) {
    assert!(map.pending.is_empty());
    let [SequentGroup::Ungrouped(sequents)] = map.sequents.as_slice() else {
        panic!("the original pair rule emits one ungrouped impossibility");
    };
    let (ante1, ante2) = if matches!(
        (first, second),
        (Constraint::ConcreteUpper(_), Constraint::ConcreteLower(_))
    ) {
        (second, first)
    } else {
        (first, second)
    };
    assert_eq!(
        sequents.as_ref(),
        &[Sequent::PairImpossibility { ante1, ante2 }]
    );
}

#[test]
fn concrete_pairs_keep_original_impossibility_and_reuse_owned_memos() {
    for equivalent in [false, true] {
        for reversed in [false, true] {
            let db = setup_db();
            let [first, second] = unequal_pair(&db, equivalent, reversed);
            let requests = [
                Request::Pair(first, second),
                Request::Pair(first, second),
                Request::Single(first),
                Request::Single(second),
            ];
            let record = execute_concrete_pairs(&db, &requests);
            let values = record.result.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(values.len(), 4);
            assert_impossible_pair(values[0], first, second);
            assert!(std::ptr::eq(values[0], values[1]));
            assert!(
                values[2..]
                    .iter()
                    .all(|map| map.sequents.is_empty() && map.pending.is_empty())
            );
            let pair_ingredient = pair_sequent_ingredient(&db);
            let pair_keys = executed(&record.events)
                .into_iter()
                .filter(|key| pair_ingredient.database_key_index(key.key_index()) == *key)
                .collect::<Vec<_>>();
            assert_eq!(pair_keys.len(), 1);
            let pair_reads = record
                .reads
                .iter()
                .filter(|read| read.key == pair_keys[0])
                .collect::<Vec<_>>();
            assert_eq!(pair_reads.len(), 2);
            assert_eq!(pair_reads[0].memo_address, pair_reads[1].memo_address);
            assert_eq!(pair_reads[0].stamp, pair_reads[1].stamp);
            let assignability = owned_assignability_ingredient(&db);
            let equivalence = owned_equivalence_ingredient(&db);
            let owned_reads = record
                .reads
                .iter()
                .filter(|read| {
                    let assignable =
                        assignability.database_key_index(read.key.key_index()) == read.key;
                    let equivalent_read =
                        equivalence.database_key_index(read.key.key_index()) == read.key;
                    if assignable || equivalent_read {
                        assert_eq!(equivalent_read, equivalent);
                        true
                    } else {
                        false
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(owned_reads.len(), if equivalent { 2 } else { 1 });
            assert_eq!(
                executed(&record.events)
                    .iter()
                    .filter(|key| **key == owned_reads[0].key)
                    .count(),
                1
            );
            for read in &owned_reads {
                assert_eq!(read.parent, Some(pair_keys[0]));
                assert_eq!(read.status, Status::Final);
                assert_eq!(read.key, owned_reads[0].key);
                assert_eq!(read.memo_address, owned_reads[0].memo_address);
                assert_eq!(read.stamp, owned_reads[0].stamp);
            }
            let ordinary = setup_db();
            let env = ordinary.program_environment();
            let [first, second] = unequal_pair(&ordinary, equivalent, reversed);
            let mut reader = ordinary.clone();
            reader.clear_salsa_events();
            let expected = prepared_source_probe::capture(&ordinary, || {
                [
                    SequentMap::for_constraint_pair(&ordinary, &env, first, second),
                    SequentMap::for_constraint_pair(&ordinary, &env, first, second),
                    SequentMap::for_constraint(&ordinary, &env, first),
                    SequentMap::for_constraint(&ordinary, &env, second),
                ]
            })
            .unwrap();
            expected.check_root_reads().unwrap();
            assert_impossible_pair(expected.value[0], first, second);
            assert!(std::ptr::eq(expected.value[0], expected.value[1]));
            assert!(
                expected.value[2..]
                    .iter()
                    .all(|map| map.sequents.is_empty() && map.pending.is_empty())
            );
            assert_eq!(record.reads.len(), expected.reads.len());
            assert_eq!(
                executed(&record.events).len(),
                executed(&reader.take_salsa_events()).len()
            );
        }
    }
}

#[test]
fn unavailable_variance_refuses_without_publishing_and_allows_same_revision_progress() {
    let db = setup_db();
    let vars = variables(&db);
    let program = db.program_environment().program(&db);
    let stamp = prepared_source_probe::Stamp::current(&db);
    let lower = |variable| {
        Constraint::from(ConcreteLowerBound::new(
            ConstraintProvenance::Evidence,
            variable,
            Type::unknown(),
        ))
    };
    let record = execute(
        &db,
        program,
        &[Request::Pair(lower(vars[0]), lower(vars[1]))],
    );
    assert_eq!(
        record.result,
        Err(Incomplete::UnsupportedSequentOperation(
            UnsupportedSequentOperation::Variance
        ))
    );
    let keys = executed(&record.events);
    assert_eq!(keys.len(), 1);
    assert!(record.reads.is_empty());
    assert!(matches!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            pair_sequent_ingredient(&db),
            keys[0].key_index()
        ),
        Err(FinalSourceError::MissingMemo)
    ));
    assert!(stamp.belongs_to(&db));
    let retry = execute(&db, program, &rows(&db, vars)[..1]);
    let values = retry.result.unwrap().unwrap();
    assert_eq!(executed(&retry.events).len(), 1);
    assert!(!values[0].sequents.is_empty());
    assert!(stamp.belongs_to(&db));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleBoundary {
    Initial,
    Recovery,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LifecycleSnapshot {
    key: salsa::Id,
    input_matches: bool,
    cycle_head: bool,
    last_shape: Option<(usize, usize)>,
}

#[derive(Default)]
struct LifecycleJournal {
    stage: Cell<Option<LifecycleBoundary>>,
    key: Cell<Option<salsa::Id>>,
    initial: Cell<Option<(usize, usize)>>,
    computed: Cell<Option<(usize, usize)>>,
    recovery: Cell<bool>,
    work_count: Cell<usize>,
    body_complete_work: Cell<Option<usize>>,
    owner_live: Cell<bool>,
    child_started: Cell<bool>,
    child_saw_owner: Cell<Option<bool>>,
    owner_snapshot: Cell<Option<LifecycleSnapshot>>,
    drops: RefCell<Vec<&'static str>>,
    fired: Cell<bool>,
    panic_identity: Arc<()>,
}

struct LifecycleOwner<'a> {
    journal: &'a LifecycleJournal,
    snapshot: &'a dyn Fn() -> LifecycleSnapshot,
}

impl Drop for LifecycleOwner<'_> {
    fn drop(&mut self) {
        self.journal.owner_snapshot.set(Some((self.snapshot)()));
        self.journal.owner_live.set(false);
        self.journal.drops.borrow_mut().push("owner");
    }
}

struct LifecycleChild<'a>(&'a LifecycleJournal);

impl Drop for LifecycleChild<'_> {
    fn drop(&mut self) {
        self.0.child_saw_owner.set(Some(self.0.owner_live.get()));
        self.0.drops.borrow_mut().push("child");
    }
}

#[derive(Debug)]
struct SequentNativePanic(Arc<()>);

struct LifecycleAdmission<'run, 'db: 'run> {
    journal: &'run LifecycleJournal,
    boundary: LifecycleBoundary,
    completion_work: Option<usize>,
    native_panic: bool,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
}

impl ExecutionAdmission for LifecycleAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !matches!(work, ExecutionWork::Work { .. }) {
            return Ok(());
        }
        let index = self.journal.work_count.get();
        self.journal.work_count.set(index + 1);
        let selected = match self.boundary {
            LifecycleBoundary::Complete => self.completion_work == Some(index),
            boundary => self.journal.stage.get() == Some(boundary),
        };
        if !selected || self.journal.fired.replace(true) {
            return Ok(());
        }
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract("sequent fault has no active endpoint"))?;
        let child = LifecycleChild(self.journal);
        *self.pending.borrow_mut() = Some(endpoint.demand(move || {
            child.0.child_started.set(true);
            async move {
                let _child = child;
                Ok(())
            }
        })?);
        if self.native_panic {
            std::panic::panic_any(SequentNativePanic(Arc::clone(&self.journal.panic_identity)));
        }
        Err(RunError::Refused(
            salsa::attempt_probe::Incomplete::Allowance,
        ))
    }
}

struct LifecycleSlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}

impl Drop for LifecycleSlots<'_, '_, '_> {
    fn drop(&mut self) {
        drop(self.pending.borrow_mut().take());
        drop(self.endpoint.borrow_mut().take());
    }
}

struct LifecycleProvider<'run, 'db: 'run, C: InternedQueryConfiguration, K, I> {
    inner: I,
    route: CallableRoute<'run, 'db, C>,
    keys: &'run QueryKeys<'db, C, K>,
    journal: &'run LifecycleJournal,
    boundary: LifecycleBoundary,
    expected_input: Option<C::Input<'db>>,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
}

impl<'run, 'db: 'run, C, K, I> CallableRouteProvider<'run, 'db, C>
    for LifecycleProvider<'run, 'db, C, K, I>
where
    C: InternedQueryConfiguration
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
    for<'a> <C as salsa::plumbing::interned::Configuration>::Fields<'a>: Copy + PartialEq,
    K: QueryKeyProfile<C> + 'run,
    I: CallableRouteProvider<'run, 'db, C>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        self.inner.native_value(endpoint, db, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        let id = endpoint.intern_query_key(self.keys, input).await;
        self.journal.key.set(Some(id));
        if self.boundary != LifecycleBoundary::Complete {
            // The self-edge invokes the registered cycle callbacks, using their actual seed.
            endpoint
                .child_call(|| async {
                    let _seed = endpoint.fetch_ref(&self.route, id)?.await?;
                    Ok(())
                })
                .await;
        }
        self.journal.stage.set(Some(LifecycleBoundary::Complete));
        **self.endpoint.borrow_mut() = Some(endpoint.clone());
        self.journal.owner_live.set(true);
        let snapshot = || LifecycleSnapshot {
            key: id,
            input_matches: self.expected_input == Some(input),
            cycle_head: false,
            last_shape: None,
        };
        let _owner = LifecycleOwner {
            journal: self.journal,
            snapshot: &snapshot,
        };
        let value = self.inner.body(endpoint, db, input).await?;
        // Complete is the shared reducer's last admitted work before returning its map.
        self.journal
            .body_complete_work
            .set(self.journal.work_count.get().checked_sub(1));
        self.journal
            .computed
            .set(Some((value.sequents.len(), value.pending.len())));
        Ok(value)
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        self.journal.stage.set(Some(LifecycleBoundary::Initial));
        **self.endpoint.borrow_mut() = Some(endpoint.clone());
        self.journal.owner_live.set(true);
        let snapshot = || LifecycleSnapshot {
            key: id,
            input_matches: self.expected_input == Some(input),
            cycle_head: false,
            last_shape: None,
        };
        let _owner = LifecycleOwner {
            journal: self.journal,
            snapshot: &snapshot,
        };
        let value = self.inner.initial(endpoint, db, id, input).await?;
        self.journal
            .initial
            .set(Some((value.sequents.len(), value.pending.len())));
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call SequentMap<'db>,
        value: SequentMap<'db>,
        input: C::Input<'db>,
    ) -> RunResult<SequentMap<'db>>
    where
        'run: 'call,
    {
        self.journal.stage.set(Some(LifecycleBoundary::Recovery));
        self.journal.recovery.set(true);
        **self.endpoint.borrow_mut() = Some(endpoint.clone());
        self.journal.owner_live.set(true);
        let snapshot = || LifecycleSnapshot {
            key: cycle.id(),
            input_matches: self.expected_input == Some(input),
            cycle_head: cycle.head_ids().any(|id| id == cycle.id()),
            last_shape: Some((last.sequents.len(), last.pending.len())),
        };
        let _owner = LifecycleOwner {
            journal: self.journal,
            snapshot: &snapshot,
        };
        self.inner
            .recover(endpoint, db, cycle, last, value, input)
            .await
    }
}

fn run_lifecycle<'db>(
    db: &'db TestDb,
    request: Request<'db>,
    boundary: LifecycleBoundary,
    completion_work: Option<usize>,
    native_panic: bool,
    journal: &LifecycleJournal,
) -> Result<RunResult<&'db SequentMap<'db>>, Incomplete> {
    let program = db.program_environment().program(db);
    expansion_probe::run(db, usize::MAX, || {
        let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
        let pending = RefCell::new(None);
        let control = LifecycleAdmission {
            journal,
            boundary,
            completion_work,
            native_panic,
            endpoint: &endpoint_slot,
            pending: &pending,
        };
        let single_keys;
        let pair_keys;
        let queries;
        let _slots = LifecycleSlots {
            endpoint: &endpoint_slot,
            pending: &pending,
        };
        let mut registry = RegistryBuilder::new(db, &control)?;
        let single_route =
            registry.reserve_callable(db as &dyn Db, single_sequent_ingredient(db))?;
        let pair_route = registry.reserve_callable(db as &dyn Db, pair_sequent_ingredient(db))?;
        single_keys = registry.callable_query_keys::<_, SingleSequentProfile>(&single_route)?;
        pair_keys = registry.callable_query_keys::<_, PairSequentProfile>(&pair_route)?;
        queries = SequentQueries {
            single_route,
            pair_route,
            single_keys: &single_keys,
            pair_keys: &pair_keys,
        };
        registry.bind_callable(
            &queries.single_route,
            LifecycleProvider {
                inner: SingleSequentProvider {
                    queries: queries.clone(),
                },
                route: queries.single_route.clone(),
                keys: &single_keys,
                journal,
                boundary,
                endpoint: &endpoint_slot,
                expected_input: match request {
                    Request::Single(constraint) => Some((program, constraint)),
                    Request::Pair(_, _) => None,
                },
            },
        )?;
        registry.bind_callable(
            &queries.pair_route,
            LifecycleProvider {
                inner: PairSequentProvider {
                    queries: queries.clone(),
                },
                route: queries.pair_route.clone(),
                keys: &pair_keys,
                journal,
                boundary,
                endpoint: &endpoint_slot,
                expected_input: match request {
                    Request::Single(_) => None,
                    Request::Pair(left, right) => Some((program, left, right)),
                },
            },
        )?;
        let queries = &queries;
        registry.seal()?.run(move |endpoint| async move {
            match request {
                Request::Single(constraint) => queries.single(&endpoint, program, constraint).await,
                Request::Pair(left, right) => queries.pair(&endpoint, program, left, right).await,
            }
        })
    })
    .0
}

fn lifecycle_case(pair: bool, boundary: LifecycleBoundary, native_panic: bool) {
    let baseline_db = setup_db();
    let baseline_vars = variables(&baseline_db);
    let index = usize::from(pair);
    let baseline_request = rows(&baseline_db, baseline_vars)[index];
    let baseline = LifecycleJournal::default();
    let expected = run_lifecycle(
        &baseline_db,
        baseline_request,
        LifecycleBoundary::Complete,
        None,
        false,
        &baseline,
    )
    .unwrap()
    .unwrap();
    assert!(!expected.sequents.is_empty());
    assert!(expected.pending.is_empty());
    let completion_work = baseline
        .body_complete_work
        .get()
        .expect("the genuine body completed");

    let db = setup_db();
    let vars = variables(&db);
    let request = rows(&db, vars)[index];
    let program = db.program_environment().program(&db);
    let stamp = prepared_source_probe::Stamp::current(&db);
    let journal = LifecycleJournal::default();
    let captured = prepared_source_probe::capture(&db, || {
        catch_unwind(AssertUnwindSafe(|| {
            run_lifecycle(
                &db,
                request,
                boundary,
                Some(completion_work),
                native_panic,
                &journal,
            )
        }))
    })
    .unwrap();
    if native_panic {
        let payload = captured
            .value
            .expect_err("native admission panic")
            .downcast::<SequentNativePanic>()
            .expect("the original payload survives");
        assert!(Arc::ptr_eq(&payload.0, &journal.panic_identity));
    } else {
        assert_eq!(captured.value.unwrap(), Err(Incomplete::Allowance),);
    }
    assert!(journal.fired.get());
    assert!(!journal.child_started.get());
    assert_eq!(journal.child_saw_owner.get(), Some(true));
    assert!(!journal.owner_live.get());
    let drops = journal.drops.borrow();
    assert_eq!(
        drops.as_slice(),
        if boundary == LifecycleBoundary::Recovery {
            &["owner", "owner", "child", "owner"][..]
        } else {
            &["child", "owner"][..]
        }
    );
    let snapshot = journal
        .owner_snapshot
        .get()
        .expect("the callback owner was observed");
    assert_eq!(Some(snapshot.key), journal.key.get());
    assert!(snapshot.input_matches);
    if boundary == LifecycleBoundary::Recovery {
        assert_eq!(journal.initial.get(), Some((0, 0)));
        assert_eq!(journal.computed.get(), Some((expected.sequents.len(), 0)));
        assert!(journal.recovery.get());
        assert!(snapshot.cycle_head);
        assert_eq!(snapshot.last_shape, Some((0, 0)));
    } else {
        assert_eq!(journal.initial.get(), None);
        assert_eq!(journal.computed.get(), None);
        assert!(!journal.recovery.get());
    }
    let id = journal.key.get().unwrap();
    let (key, certification) = match request {
        Request::Single(_) => {
            let ingredient = single_sequent_ingredient(&db);
            (
                ingredient.database_key_index(id),
                FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            )
        }
        Request::Pair(_, _) => {
            let ingredient = pair_sequent_ingredient(&db);
            (
                ingredient.database_key_index(id),
                FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            )
        }
    };
    assert!(
        !captured
            .reads
            .iter()
            .any(|read| read.key == key && read.parent.is_none())
    );
    if native_panic {
        assert!(
            matches!(
                certification,
                Err(FinalSourceError::MissingMemo | FinalSourceError::ProvisionalMemo)
            ),
            "unfinished sequent map must not be published"
        );
    } else {
        assert_eq!(
            certification,
            Err(if boundary == LifecycleBoundary::Recovery {
                FinalSourceError::ProvisionalMemo
            } else {
                FinalSourceError::MissingMemo
            })
        );
    }
    if !native_panic {
        let retry = execute(&db, program, &[request]);
        let values = retry.result.unwrap().unwrap();
        assert_eq!(executed(&retry.events).len(), 1);
        assert_eq!(
            normalize(&db, vars, values[0]),
            normalize(&baseline_db, baseline_vars, expected)
        );
    } else {
        let retry = salsa::Cancelled::catch(AssertUnwindSafe(|| execute(&db, program, &[request])));
        assert!(matches!(retry, Err(salsa::Cancelled::PropagatedPanic)));
    }
    assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
}

#[test]
fn registered_sequent_initial_and_recovery_drain_children_before_callback_owners() {
    for pair in [false, true] {
        for boundary in [LifecycleBoundary::Initial, LifecycleBoundary::Recovery] {
            lifecycle_case(pair, boundary, false);
        }
    }
}

#[test]
fn sequent_completion_refusal_drains_children_and_leaves_the_memo_unpublished() {
    for pair in [false, true] {
        lifecycle_case(pair, LifecycleBoundary::Complete, false);
    }
}

#[test]
fn registered_sequent_native_panics_preserve_payload_and_callback_ownership() {
    for pair in [false, true] {
        for boundary in [
            LifecycleBoundary::Initial,
            LifecycleBoundary::Recovery,
            LifecycleBoundary::Complete,
        ] {
            lifecycle_case(pair, boundary, true);
        }
    }
}
