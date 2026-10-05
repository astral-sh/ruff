use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::sync::Arc;

use ruff_db::files::system_path_to_file;
use ruff_index::Idx;
use ruff_python_ast::name::Name;
use salsa::attempt_probe::remaining_allowance_for_diagnostics;
use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, FinalSourceMemo, RegistryBuilder,
};
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe::{self, Read, Stamp, Status};
use salsa::{Database, DatabaseKeyIndex, Event, EventKind};
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::constraints::control::{GrowthPlan, map_growth};
use crate::types::constraints::control::{Unrestricted as UnrestrictedCollections, unrestricted};
use crate::types::constraints::sequents::profile::{PairSequentProfile, SingleSequentProfile};
use crate::types::constraints::sequents::runtime::{PairSequentProvider, SingleSequentProvider};
use crate::types::constraints::sequents::{
    Sequent, SequentGroup, pair_sequent_ingredient, single_sequent_ingredient,
};
use crate::types::constraints::type_analysis::{
    OrdinaryConstraintTypes, SyncConstraintTypeEffects,
};
use crate::types::constraints::typevar_equivalence::tests::assert_consistent;
use crate::types::constraints::variables::{ConstraintProvenance, TypeVarRangeBound};
use crate::types::constraints::{
    ALWAYS_FALSE, Node, NodeId, SourceOrder, max_constructor_and_typevar_depth,
    possible_assignability_ingredient,
};
use crate::types::mapping::runtime::{
    MaterializationKeyProfile, MaterializationObservations, MaterializationProvider,
    MaterializationQueries,
};
use crate::types::newtype::{NewType, NewTypeBase};
use crate::types::relation::runtime::constraint_set::{
    ConstraintSetObservation, OwnedRelationKind, OwnedRelationObservation, OwnedRelationProvider,
    OwnedRelationQueries,
};
use crate::types::relation::runtime::protocol::NoProtocolQueries;
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallMappingVisitors, CallRelationOwners, CallResourceCapacity,
};
use crate::types::relation::{
    RelationOwners, TypeRelation, TypeVarEvaluation, owned_assignability_ingredient,
    owned_equivalence_ingredient, redundancy_ingredient,
};
use crate::types::set_theoretic::{
    intersection_from_two_elements_ingredient, union_from_two_elements_ingredient,
};
use crate::types::tuple::TupleType;
use crate::types::visitor::SearchOperation;
use crate::types::visitor::{
    SearchWork, TypeCollector, TypeDepthEffects, TypeSearchEffects, TypeSearchMode, TypeWalkCursor,
    TypeWalkEffects, TypeWalkEvent, TypeWalkPolicy, TypeWalkWork, WalkAction,
};
use crate::types::{CallableType, Parameter, Parameters, Signature};
use crate::types::{
    DynamicType, KnownInstanceType, TypeFormType, TypePair, TypeVarVariance,
    cached_materialization_ingredient, register_type_pair_values,
};
use rustc_hash::FxHashSet;

#[cfg(feature = "experimental-analysis")]
mod walk_receiver;

struct ConcreteInventory<'db, T> {
    captured: prepared_source_probe::Captured<'db, T>,
    preparation: Vec<Event>,
    events: Vec<Event>,
}

fn capture_concrete_inventory<'db, T>(
    db: &'db TestDb,
    run: impl FnOnce() -> T,
) -> ConcreteInventory<'db, T> {
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(db, run)
        .expect("ordinary concrete inventory starts outside a query");
    let events = reader.take_salsa_events();
    assert!(captured.belongs_to(db));
    if !captured.reads.is_empty() {
        assert_eq!(captured.check_root_reads(), Ok(()));
    }
    ConcreteInventory {
        captured,
        preparation,
        events,
    }
}

fn print_concrete_inventory<T: Debug>(
    db: &TestDb,
    label: &str,
    inventory: &ConcreteInventory<'_, T>,
    cold_reads: &[Read],
) {
    salsa::attach(db, || {
        eprintln!("CONCRETE_RESULT {label}: {:?}", inventory.captured.value);
        for (index, event) in inventory.preparation.iter().enumerate() {
            eprintln!("CONCRETE_PREPARATION_EVENT {label} {index}: {event:?}");
        }
        for (index, read) in inventory.captured.reads.iter().enumerate() {
            let cold_matches = cold_reads
                .iter()
                .enumerate()
                .filter(|(_, cold)| cold.key == read.key)
                .map(|(index, cold)| {
                    (
                        index,
                        cold.stamp == read.stamp,
                        cold.memo_address == read.memo_address,
                    )
                })
                .collect::<Vec<_>>();
            eprintln!(
                "CONCRETE_READ {label} {index}: {read:?}; cold_matches=(index, same_stamp, same_memo) {cold_matches:?}"
            );
        }
        for (index, event) in inventory.events.iter().enumerate() {
            eprintln!("CONCRETE_EVENT {label} {index}: {event:?}");
        }
    });
}

#[derive(Clone, Copy, Debug)]
enum ConcreteFormula {
    EqualRange,
    UnequalRange,
    UnequalEquivalences,
    GradualEquivalence,
}

fn concrete_formula<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    typevar: BoundTypeVarInstance<'db>,
    selected: ConcreteFormula,
    reversed: bool,
) -> ConstraintSet<'db, 'c> {
    let env = db.program_environment();
    if matches!(selected, ConcreteFormula::GradualEquivalence) {
        let bound = TypeFormType::from_type_expression(db, Type::any());
        return ConstraintSet::constrain_typevar_equivalence_bound(
            db, &env, builder, typevar, bound,
        );
    }
    let first = TypeFormType::from_type_expression(db, Type::int_literal(1));
    let second = TypeFormType::from_type_expression(
        db,
        Type::int_literal(if matches!(selected, ConcreteFormula::EqualRange) {
            1
        } else {
            2
        }),
    );
    let first_constraint = || {
        if matches!(selected, ConcreteFormula::UnequalEquivalences) {
            ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, typevar, first)
        } else {
            ConstraintSet::constrain_typevar_lower_bound(db, &env, builder, typevar, first)
        }
    };
    let second_constraint = || {
        if matches!(selected, ConcreteFormula::UnequalEquivalences) {
            ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, typevar, second)
        } else {
            ConstraintSet::constrain_typevar_upper_bound(db, &env, builder, typevar, second)
        }
    };
    if reversed {
        second_constraint().and(db, builder, first_constraint)
    } else {
        first_constraint().and(db, builder, second_constraint)
    }
}

fn concrete_inventory_constraints<'db>(
    builder: &ConstraintSetBuilder<'db>,
) -> Vec<(ConstraintId, Constraint<'db>, Support, Option<(u16, u16)>)> {
    let storage = builder.storage.borrow();
    storage
        .constraints
        .iter_enumerated()
        .map(|(id, constraint)| {
            (
                id,
                *constraint,
                storage.constraint_support(id).clone(),
                storage.constraint_bound_depth_cache.get(&id).copied(),
            )
        })
        .collect()
}

#[test]
#[ignore = "temporary ordinary concrete solver dependency inventory"]
fn ordinary_concrete_solver_read_inventory() {
    for selected in [
        ConcreteFormula::EqualRange,
        ConcreteFormula::UnequalRange,
        ConcreteFormula::UnequalEquivalences,
        ConcreteFormula::GradualEquivalence,
    ] {
        for reversed in [false, true] {
            if reversed && matches!(selected, ConcreteFormula::GradualEquivalence) {
                continue;
            }
            let label = format!("{selected:?}/reversed={reversed}");
            let db = setup_db();
            let env = db.program_environment();
            let [typevar, ..] = variables(&db);
            let builder = ConstraintSetBuilder::new();
            let construction = capture_concrete_inventory(&db, || {
                concrete_formula(&db, &builder, typevar, selected, reversed)
            });
            let set = construction.captured.value;
            let before = concrete_inventory_constraints(&builder);
            let before_never = builder
                .storage
                .borrow()
                .never_satisfied_cache
                .get(&set.node)
                .copied();
            let cold = capture_concrete_inventory(&db, || set.is_never_satisfied(&db, &env));
            let after = concrete_inventory_constraints(&builder);
            let retry = capture_concrete_inventory(&db, || set.is_never_satisfied(&db, &env));

            // Formatting and producer inspection happen after both solves, since they can read queries.
            print_concrete_inventory(&db, &format!("{label}/construction"), &construction, &[]);
            print_concrete_inventory(&db, &format!("{label}/cold"), &cold, &[]);
            print_concrete_inventory(
                &db,
                &format!("{label}/same_builder"),
                &retry,
                &cold.captured.reads,
            );
            salsa::attach(&db, || {
                let storage = builder.storage.borrow();
                eprintln!(
                    "CONCRETE_BUILDER {label}: root={:?}; source_order={:?}; nodes={:?}; source_orders={:?}; typevars={:?}; before_never={before_never:?}; after_never={:?}",
                    set.node,
                    set.source_order,
                    storage.nodes,
                    storage.source_orders,
                    storage.typevars,
                    storage.never_satisfied_cache.get(&set.node),
                );
                eprintln!(
                    "CONCRETE_BEFORE_CONSTRAINTS {label}: (id, value, support, cached_depth) {before:?}"
                );
                eprintln!(
                    "CONCRETE_AFTER_CONSTRAINTS {label}: (id, value, support, cached_depth) {after:?}"
                );
                for row in &after {
                    if !before.iter().any(|before| before.0 == row.0) {
                        eprintln!("CONCRETE_NEWLY_RETAINED_CONSTRAINT {label}: {row:?}");
                    }
                }
            });
            assert_consistent(&db, &builder.storage.borrow());

            // Re-fetch only sequent keys that the cold solve actually read, in their observed order.
            for (index, read) in cold.captured.reads.iter().enumerate() {
                if single_sequent_ingredient(&db).database_key_index(read.key.key_index())
                    != read.key
                    && pair_sequent_ingredient(&db).database_key_index(read.key.key_index())
                        != read.key
                {
                    continue;
                }
                let request = read_request(&db, read.key);
                let inspected = capture_concrete_inventory(&db, || match request {
                    Request::Single(value) => SequentMap::for_constraint(&db, &env, value),
                    Request::Pair(left, right) => {
                        SequentMap::for_constraint_pair(&db, &env, left, right)
                    }
                });
                salsa::attach(&db, || {
                    eprintln!("CONCRETE_SEQUENT_REQUEST {label}/cold_read={index}: {request:?}");
                });
                print_concrete_inventory(
                    &db,
                    &format!("{label}/inspect_sequent/cold_read={index}"),
                    &inspected,
                    &cold.captured.reads,
                );
            }

            let mut bounds = Vec::new();
            for (_, constraint, _, _) in &after {
                let bound = match constraint {
                    Constraint::ConcreteLower(value) => value.bound,
                    Constraint::ConcreteUpper(value) => value.bound,
                    Constraint::ConcreteEquivalence(value) => value.bound,
                    Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => continue,
                };
                if !bounds.contains(&bound) {
                    bounds.push(bound);
                }
            }
            for (index, bound) in bounds.iter().copied().enumerate() {
                for kind in [MaterializationKind::Bottom, MaterializationKind::Top] {
                    let inspected = capture_concrete_inventory(&db, || {
                        (bound, kind, bound.materialization(&db, &env, kind))
                    });
                    print_concrete_inventory(
                        &db,
                        &format!("{label}/inspect_materialization/bound={index}/{kind:?}"),
                        &inspected,
                        &cold.captured.reads,
                    );
                }
            }
            // These original producers are inspection calls; matching keys identify cold dependencies.
            for (left_index, left) in bounds.iter().copied().enumerate() {
                for (right_index, right) in bounds.iter().copied().enumerate() {
                    let pair = format!("{label}/inspect_relation/{left_index}->{right_index}");
                    let assignable = capture_concrete_inventory(&db, || {
                        let value = left.when_constraint_set_assignable_to_owned(&db, &env, right);
                        (left, right, matches!(&value, Cow::Borrowed(_)), value)
                    });
                    print_concrete_inventory(
                        &db,
                        &format!("{pair}/when_constraint_set_assignable_to_owned"),
                        &assignable,
                        &cold.captured.reads,
                    );
                    let equivalent = capture_concrete_inventory(&db, || {
                        let value = left.when_constraint_set_equivalent_to_owned(&db, &env, right);
                        (left, right, matches!(&value, Cow::Borrowed(_)), value)
                    });
                    print_concrete_inventory(
                        &db,
                        &format!("{pair}/when_constraint_set_equivalent_to_owned"),
                        &equivalent,
                        &cold.captured.reads,
                    );
                    let scalar_assignable = capture_concrete_inventory(&db, || {
                        (
                            left,
                            right,
                            left.is_constraint_set_assignable_to(&db, &env, right),
                        )
                    });
                    print_concrete_inventory(
                        &db,
                        &format!("{pair}/is_constraint_set_assignable_to"),
                        &scalar_assignable,
                        &cold.captured.reads,
                    );
                    let scalar_equivalent = capture_concrete_inventory(&db, || {
                        (
                            left,
                            right,
                            left.is_constraint_set_equivalent_to(&db, &env, right),
                        )
                    });
                    print_concrete_inventory(
                        &db,
                        &format!("{pair}/is_constraint_set_equivalent_to"),
                        &scalar_equivalent,
                        &cold.captured.reads,
                    );
                }
            }
        }
    }
}

#[derive(Default)]
struct Admission(RefCell<Vec<ExecutionWork>>);
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.0.borrow_mut().push(work);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Request<C> {
    Single(C),
    Pair(C, C),
}
struct PathSnapshot {
    event: PathTrace,
    sequents: Vec<Sequent<ConstraintId, u16>>,
    discovered: Vec<(ConstraintId, bool)>,
}
#[derive(Default)]
struct Observations<'db> {
    fetched: Vec<(Request<Constraint<'db>>, &'db SequentMap<'db>)>,
    imported: Vec<(Constraint<'db>, ConstraintId)>,
    paths: Vec<PathSnapshot>,
    or_nodes: Vec<(NodeId, NodeId, NodeId)>,
    before_never_cache: Vec<(NodeId, Option<usize>)>,
    bound_searches: Vec<(Type<'db>, BoundSearch, bool)>,
    before_import: Vec<Constraint<'db>>,
    before_depth: Vec<Type<'db>>,
    after_depth: Vec<(Type<'db>, (u16, u16))>,
    before_depth_cache: Vec<(ConstraintId, (u16, u16))>,
}
impl<'db> Observations<'db> {
    fn record(&mut self, db: &TestDb, observation: Observation<'_, 'db>) {
        match observation {
            Observation::Single(value, map) => self.fetched.push((Request::Single(value), map)),
            Observation::Pair(left, right, map) => {
                self.fetched.push((Request::Pair(left, right), map))
            }
            Observation::Imported(value, id) => self.imported.push((value, id)),
            Observation::BeforeConstraintImport(value) => self.before_import.push(value),
            Observation::BeforeDepth(bound) => self.before_depth.push(bound),
            Observation::AfterDepth(bound, depth) => self.after_depth.push((bound, depth)),
            Observation::BeforeDepthCacheInsert(id, depth) => {
                self.before_depth_cache.push((id, depth));
            }
            Observation::Or(left, right, result) => self.or_nodes.push((left, right, result)),
            Observation::BoundSearch(bound, search, found) => {
                self.bound_searches.push((bound, search, found));
            }
            Observation::BeforeNeverCacheInsert(node) => self
                .before_never_cache
                .push((node, remaining_allowance_for_diagnostics(db))),
            Observation::Path(event, path) => self.paths.push(PathSnapshot {
                event,
                sequents: path.observed_sequents().to_vec(),
                discovered: path.observed_discovered().collect(),
            }),
        }
    }
}
struct Record<'db, T = bool> {
    result: T,
    observations: Observations<'db>,
    events: Vec<Event>,
    reads: Vec<Read>,
    work: Vec<ExecutionWork>,
    delivered: bool,
    entry_remaining: Option<usize>,
    exit_remaining: Option<usize>,
}
fn execute<'db>(
    db: &'db TestDb,
    program: Program<'db>,
    constraints: ConstraintSet<'db, '_>,
    kind: SatisfactionKind,
    observe: bool,
) -> Record<'db> {
    let record = execute_with_allowance(db, program, constraints, kind, observe, usize::MAX);
    assert!(record.delivered);
    assert!(!record.reads.is_empty());
    Record {
        result: record.result.unwrap().unwrap(),
        observations: record.observations,
        events: record.events,
        reads: record.reads,
        work: record.work,
        delivered: record.delivered,
        entry_remaining: record.entry_remaining,
        exit_remaining: record.exit_remaining,
    }
}

fn execute_with_allowance<'db>(
    db: &'db TestDb,
    program: Program<'db>,
    constraints: ConstraintSet<'db, '_>,
    kind: SatisfactionKind,
    observe: bool,
    allowance: usize,
) -> Record<'db, Result<RunResult<bool>, Incomplete>> {
    let admission = Admission::default();
    let observations = RefCell::new(Observations::default());
    let delivered = Cell::new(false);
    let entry_remaining = Cell::new(None);
    let exit_remaining = Cell::new(None);
    let observer = |observation: Observation<'_, 'db>| {
        if let Observation::BeforeNeverCacheInsert(node) = &observation {
            assert_eq!(*node, constraints.node);
            assert!(
                !constraints
                    .builder
                    .storage
                    .borrow()
                    .never_satisfied_cache
                    .contains_key(node)
            );
        }
        observations.borrow_mut().record(db, observation);
    };
    let observer: Option<&dyn Fn(Observation<'_, 'db>)> = observe.then_some(&observer);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, allowance, || {
            // Tokens and the borrowed bundle outlive registry teardown, including a failed bind.
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
            let delivered = &delivered;
            let entry_remaining = &entry_remaining;
            let exit_remaining = &exit_remaining;
            registry.seal()?.run(move |endpoint| async move {
                entry_remaining.set(remaining_allowance_for_diagnostics(db));
                let result = if observer.is_some() {
                    satisfy_observed(db, &endpoint, program, constraints, kind, queries, observer)
                        .await
                } else {
                    satisfy(db, &endpoint, program, constraints, kind, queries).await
                };
                if result.is_ok() {
                    delivered.set(true);
                    exit_remaining.set(remaining_allowance_for_diagnostics(db));
                }
                result
            })
        })
    })
    .unwrap();
    if !captured.reads.is_empty() {
        captured.check_root_reads().unwrap();
    }
    Record {
        result: captured.value.0,
        observations: observations.into_inner(),
        events: reader.take_salsa_events(),
        reads: captured.reads,
        work: admission.0.into_inner(),
        delivered: delivered.get(),
        entry_remaining: entry_remaining.get(),
        exit_remaining: exit_remaining.get(),
    }
}
fn variables(db: &TestDb) -> [BoundTypeVarInstance<'_>; 4] {
    ["T", "U", "V", "W"].map(|name| {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConcreteBoundary {
    Root,
    Import,
    Depth,
    DepthComplete,
    DepthPublish,
    ScalarChecker,
    ScalarPair,
    ScalarAlways,
    OwnedResult,
    OwnedConstructed,
    OwnedDirection,
    OwnedPair,
    OwnedPackaged,
    Single,
    Pair,
    NeverPublish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConcreteFault {
    Refuse,
    Panic,
}

#[derive(Debug)]
struct ConcreteNativePanic(Arc<()>);

struct ConcreteAdmission<'a, 'run, 'db: 'run> {
    events: &'a RefCell<Vec<ExecutionWork>>,
    fault: Option<(usize, ConcreteFault)>,
    fired: &'a Cell<bool>,
    panic_identity: &'a Arc<()>,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    cleanup: &'run dyn Fn(),
    child_started: &'run Cell<bool>,
}

impl ExecutionAdmission for ConcreteAdmission<'_, '_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let index = self.events.borrow().len();
        self.events.borrow_mut().push(work);
        if let Some((target, fault)) = self.fault
            && target == index
            && !self.fired.replace(true)
        {
            let endpoint = self
                .endpoint
                .borrow()
                .as_ref()
                .cloned()
                .ok_or(RunError::Contract(
                    "concrete solver fault requires an active endpoint",
                ))?;
            let cleanup = CacheChildCleanup(self.cleanup);
            let started = self.child_started;
            *self.pending.borrow_mut() = Some(endpoint.demand(move || {
                started.set(true);
                async move {
                    let _cleanup = cleanup;
                    Ok(())
                }
            })?);
            match fault {
                ConcreteFault::Refuse => {
                    return Err(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance,
                    ));
                }
                ConcreteFault::Panic => {
                    std::panic::panic_any(ConcreteNativePanic(Arc::clone(self.panic_identity)))
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ConcreteOutcome {
    Returned(Result<RunResult<bool>, Incomplete>),
    NativePanic,
}

#[derive(Debug)]
enum ConcreteRelationSnapshot {
    Checker {
        resources: [*const (); 6],
        relation: TypeRelation,
        evaluation: TypeVarEvaluation,
        inferable_none: bool,
        given_never: bool,
    },
    Pair {
        builder: *const (),
        node: NodeId,
    },
    Always {
        builder: *const (),
        node: NodeId,
        result: bool,
    },
    Owned {
        borrowed: bool,
        always: bool,
        node: NodeId,
        address: *const (),
        source_order_absent: bool,
        inner_absent: bool,
    },
}

#[derive(Debug)]
enum ConcreteOwnedSnapshot<'db> {
    Constructed {
        kind: OwnedRelationKind,
        resources: [*const (); 6],
    },
    Direction {
        source: Type<'db>,
        target: Type<'db>,
        resources: [*const (); 6],
        relation: TypeRelation,
        evaluation: TypeVarEvaluation,
    },
    Pair {
        builder: *const (),
        node: NodeId,
    },
    Packaged {
        node: NodeId,
        source_order_absent: bool,
        inner_absent: bool,
    },
    Initial,
    Recovery,
}

struct ConcreteRun<'db> {
    record: Record<'db, ConcreteOutcome>,
    boundaries: Vec<(ConcreteBoundary, usize)>,
    relations: Vec<ConcreteRelationSnapshot>,
    owned_relations: Vec<ConcreteOwnedSnapshot<'db>>,
    materialization_roots: usize,
}

#[derive(Clone, Copy)]
enum ConcreteEntry {
    Never,
    RelationSatisfaction { always: bool },
}

fn execute_concrete<'db>(
    db: &'db TestDb,
    constraints: ConstraintSet<'db, '_>,
    allowance: usize,
    fault: Option<(usize, ConcreteFault)>,
) -> ConcreteRun<'db> {
    execute_concrete_entry(db, constraints, allowance, fault, ConcreteEntry::Never)
}

fn execute_concrete_entry<'db>(
    db: &'db TestDb,
    constraints: ConstraintSet<'db, '_>,
    allowance: usize,
    fault: Option<(usize, ConcreteFault)>,
    entry: ConcreteEntry,
) -> ConcreteRun<'db> {
    let program = db.program_environment().program(db);
    let env = db.program_environment();
    let input_owners = RelationOwners::new(&env, constraints.builder);
    let checker = input_owners.constraint_set_assignability();
    let observations = RefCell::new(Observations::default());
    let relations = RefCell::new(Vec::new());
    let owned_relations_observed = RefCell::new(Vec::new());
    let boundaries = RefCell::new(Vec::new());
    let work = RefCell::new(Vec::new());
    let fired = Cell::new(false);
    let panic_identity = Arc::new(());
    let delivered = Cell::new(false);
    let entry_remaining = Cell::new(None);
    let exit_remaining = Cell::new(None);
    let materialization_observations = MaterializationObservations::default();
    let live = Cell::new(false);
    let journal = RefCell::new(Vec::new());
    let child_started = Cell::new(false);
    let cleanup_snapshot = RefCell::new(None);
    let cleanup = || {
        let storage = constraints.builder.storage.try_borrow();
        *cleanup_snapshot.borrow_mut() = Some((
            live.get(),
            storage.is_ok(),
            storage.as_ref().ok().and_then(|storage| {
                storage
                    .never_satisfied_cache
                    .get(&constraints.node)
                    .copied()
            }),
        ));
        journal.borrow_mut().push("child");
    };
    let observe = |observation: Observation<'_, 'db>| {
        let boundary = match &observation {
            Observation::BeforeConstraintImport(_) => Some(ConcreteBoundary::Import),
            Observation::BeforeDepth(_) => Some(ConcreteBoundary::Depth),
            Observation::AfterDepth(..) => Some(ConcreteBoundary::DepthComplete),
            Observation::BeforeDepthCacheInsert(..) => Some(ConcreteBoundary::DepthPublish),
            Observation::BeforeNeverCacheInsert(_) => Some(ConcreteBoundary::NeverPublish),
            Observation::Single(..) => Some(ConcreteBoundary::Single),
            Observation::Pair(..) => Some(ConcreteBoundary::Pair),
            _ => None,
        };
        if let Some(boundary) = boundary {
            boundaries
                .borrow_mut()
                .push((boundary, work.borrow().len()));
        }
        observations.borrow_mut().record(db, observation);
    };
    let observe_relation = |observation: ConstraintSetObservation<'_, 'db>| {
        let (boundary, snapshot) = match observation {
            ConstraintSetObservation::Checker {
                resources,
                relation,
                evaluation,
                inferable_none,
                given_never,
            } => (
                ConcreteBoundary::ScalarChecker,
                ConcreteRelationSnapshot::Checker {
                    resources,
                    relation,
                    evaluation,
                    inferable_none,
                    given_never,
                },
            ),
            ConstraintSetObservation::PairResult(set) => (
                ConcreteBoundary::ScalarPair,
                ConcreteRelationSnapshot::Pair {
                    builder: std::ptr::from_ref(set.builder).cast(),
                    node: set.node,
                },
            ),
            ConstraintSetObservation::AlwaysResult {
                constraints: set,
                result,
            } => (
                ConcreteBoundary::ScalarAlways,
                ConcreteRelationSnapshot::Always {
                    builder: std::ptr::from_ref(set.builder).cast(),
                    node: set.node,
                    result,
                },
            ),
            ConstraintSetObservation::OwnedResult(value) => (
                ConcreteBoundary::OwnedResult,
                ConcreteRelationSnapshot::Owned {
                    borrowed: matches!(value, Cow::Borrowed(_)),
                    always: value.is_trivially_always_satisfied(),
                    node: value.node,
                    address: std::ptr::from_ref(value.as_ref()).cast(),
                    source_order_absent: value.source_order.is_none(),
                    inner_absent: value.inner.is_none(),
                },
            ),
        };
        boundaries
            .borrow_mut()
            .push((boundary, work.borrow().len()));
        relations.borrow_mut().push(snapshot);
    };
    let observe_owned = |observation: OwnedRelationObservation<'_, '_, 'db>| {
        let (boundary, snapshot) = match observation {
            OwnedRelationObservation::Constructed {
                kind, resources, ..
            } => (
                Some(ConcreteBoundary::OwnedConstructed),
                ConcreteOwnedSnapshot::Constructed { kind, resources },
            ),
            OwnedRelationObservation::Direction {
                source,
                target,
                resources,
                relation,
                evaluation,
                materialization_guard: _,
            } => (
                Some(ConcreteBoundary::OwnedDirection),
                ConcreteOwnedSnapshot::Direction {
                    source,
                    target,
                    resources,
                    relation,
                    evaluation,
                },
            ),
            OwnedRelationObservation::PairResult(set) => (
                Some(ConcreteBoundary::OwnedPair),
                ConcreteOwnedSnapshot::Pair {
                    builder: std::ptr::from_ref(set.builder).cast(),
                    node: set.node,
                },
            ),
            OwnedRelationObservation::Packaged(value) => (
                Some(ConcreteBoundary::OwnedPackaged),
                ConcreteOwnedSnapshot::Packaged {
                    node: value.node,
                    source_order_absent: value.source_order.is_none(),
                    inner_absent: value.inner.is_none(),
                },
            ),
            OwnedRelationObservation::Initial => (None, ConcreteOwnedSnapshot::Initial),
            OwnedRelationObservation::Recovery => (None, ConcreteOwnedSnapshot::Recovery),
        };
        if let Some(boundary) = boundary {
            boundaries
                .borrow_mut()
                .push((boundary, work.borrow().len()));
        }
        owned_relations_observed.borrow_mut().push(snapshot);
    };

    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            expansion_probe::run(db, allowance, || {
                // Routes borrow tokens and stable pools. Registry teardown drains their tasks
                // before any of these retained resources leave scope.
                let capacity = CallResourceCapacity { calls: NonZeroUsize::new(32).unwrap() };
                let environments = CallEnvironments::with_capacity(capacity);
                let builders = CallBuilders::with_capacity(capacity);
                let owners = CallRelationOwners::with_capacity(capacity);
                let visitors = CallMappingVisitors::with_capacity(capacity);
                let single_keys;
                let pair_keys;
                let materialization_keys;
                let forms;
                let type_pairs;
                let owned_relations;
                let materialization;
                let queries;
                let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
                let pending = RefCell::new(None);
                let admission = ConcreteAdmission {
                    events: &work, fault, fired: &fired, panic_identity: &panic_identity, endpoint: &endpoint_slot,
                    pending: &pending, cleanup: &cleanup, child_started: &child_started,
                };
                let _reset = CacheReset { endpoint: &endpoint_slot, pending: &pending };
                let mut registry = RegistryBuilder::new(db, &admission)?;
                let single_route = registry.reserve_callable(db as &dyn Db, single_sequent_ingredient(db))?;
                let pair_route = registry.reserve_callable(db as &dyn Db, pair_sequent_ingredient(db))?;
                let materialization_route = registry.reserve_callable(db as &dyn Db, cached_materialization_ingredient(db))?;
                single_keys = registry.callable_query_keys::<_, SingleSequentProfile>(&single_route)?;
                pair_keys = registry.callable_query_keys::<_, PairSequentProfile>(&pair_route)?;
                materialization_keys = registry.callable_query_keys::<_, MaterializationKeyProfile>(&materialization_route)?;
                forms = registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
                let assignability = owned_assignability_ingredient(db);
                let equivalence = owned_equivalence_ingredient(db);
                let assignability_route = registry.reserve_callable(db as &dyn Db, assignability)?;
                let equivalence_route = registry.reserve_callable(db as &dyn Db, equivalence)?;
                type_pairs = register_type_pair_values(
                    db, &mut registry, assignability, equivalence, redundancy_ingredient(db),
                    possible_assignability_ingredient(db), union_from_two_elements_ingredient(db),
                    intersection_from_two_elements_ingredient(db),
                )?;
                owned_relations = OwnedRelationQueries {
                    assignability: assignability_route, equivalence: equivalence_route, keys: &type_pairs,
                };
                materialization = MaterializationQueries { route: materialization_route, keys: &materialization_keys };
                queries = ConcreteSolverQueries {
                    sequents: SequentQueries {
                        single_route, pair_route, single_keys: &single_keys, pair_keys: &pair_keys,
                    },
                    materialization: &materialization,
                    environments: &environments,
                    builders: &builders,
                    owners: &owners,
                    protocols: NoProtocolQueries,
                    owned_relations: owned_relations.clone(),
                    relation_observer: Some(&observe_relation),
                };
                registry.bind_callable(&materialization.route, MaterializationProvider {
                    environments: &environments, visitors: &visitors, forms: &forms,
                    observations: &materialization_observations,
                })?;
                registry.bind_callable(
                    &owned_relations.assignability,
                    OwnedRelationProvider {
                        kind: OwnedRelationKind::Assignability, queries: queries.clone(),
                        environments: &environments, builders: &builders, owners: &owners,
                        mappings: &visitors, observer: Some(&observe_owned),
                    },
                )?;
                registry.bind_callable(
                    &owned_relations.equivalence,
                    OwnedRelationProvider {
                        kind: OwnedRelationKind::Equivalence, queries: queries.clone(),
                        environments: &environments, builders: &builders, owners: &owners,
                        mappings: &visitors, observer: Some(&observe_owned),
                    },
                )?;
                registry.bind_callable(&queries.sequents.single_route, SingleSequentProvider { queries: queries.clone() })?;
                registry.bind_callable(&queries.sequents.pair_route, PairSequentProvider { queries: queries.clone() })?;
                let queries = &queries;
                let admission = &admission;
                let live = &live;
                let journal = &journal;
                let delivered = &delivered;
                let entry_remaining = &entry_remaining;
                let exit_remaining = &exit_remaining;
                let boundaries = &boundaries;
                let observe = &observe;
                let checker = &checker;
                registry.seal()?.run(move |endpoint| {
                    **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        live.set(true);
                        let _owner = CacheRootOwner { live, journal };
                        boundaries.borrow_mut().push((ConcreteBoundary::Root, admission.events.borrow().len()));
                        entry_remaining.set(remaining_allowance_for_diagnostics(db));
                        let result = match entry {
                            ConcreteEntry::Never => satisfy_observed(db, &endpoint, program, constraints, SatisfactionKind::Never, queries, Some(observe)).await,
                            ConcreteEntry::RelationSatisfaction { always } => {
                                crate::types::relation::runtime::constraint_set::satisfy_constraints(
                                    db, &endpoint, checker, constraints, always, (*queries).clone(),
                                ).await
                            }
                        };
                        if result.is_ok() {
                            delivered.set(true);
                            exit_remaining.set(remaining_allowance_for_diagnostics(db));
                        }
                        result
                    }
                })
            })
        }))
    }).expect("composed solver starts outside a query");
    if captured
        .value
        .as_ref()
        .is_ok_and(|value| matches!(&value.0, Ok(Ok(_))))
        && !captured.reads.is_empty()
    {
        captured.check_root_reads().unwrap();
    }
    let result = match captured.value {
        Ok(result) => ConcreteOutcome::Returned(result.0),
        Err(payload) if payload.is::<ConcreteNativePanic>() => {
            let payload = payload.downcast::<ConcreteNativePanic>().unwrap();
            assert!(Arc::ptr_eq(&payload.0, &panic_identity));
            ConcreteOutcome::NativePanic
        }
        Err(payload) => std::panic::resume_unwind(payload),
    };
    assert!(!live.get());
    assert!(!child_started.get());
    assert_eq!(fired.get(), fault.is_some());
    if fault.is_some() {
        assert_eq!(*cleanup_snapshot.borrow(), Some((true, true, None)));
        assert_eq!(&*journal.borrow(), &["child", "root"]);
    } else {
        assert!(cleanup_snapshot.borrow().is_none());
    }
    ConcreteRun {
        record: Record {
            result,
            observations: observations.into_inner(),
            events: reader.take_salsa_events(),
            reads: captured.reads,
            work: work.into_inner(),
            delivered: delivered.get(),
            entry_remaining: entry_remaining.get(),
            exit_remaining: exit_remaining.get(),
        },
        boundaries: boundaries.into_inner(),
        relations: relations.into_inner(),
        owned_relations: owned_relations_observed.into_inner(),
        materialization_roots: materialization_observations.roots.get(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConcreteRequest {
    Sequent(Request<usize>),
    Owned {
        equivalent: bool,
        first: ConcreteBound,
        second: ConcreteBound,
    },
    Materialization {
        bound: ConcreteBound,
        kind: MaterializationKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConcreteBound {
    One,
    Two,
    Any,
}

fn concrete_bound<'db>(db: &'db TestDb, bound: Type<'db>) -> ConcreteBound {
    let Type::TypeForm(form) = bound else {
        panic!("the concrete fixture retains its original TypeForm bound");
    };
    match form.type_argument(db) {
        argument if argument == Type::int_literal(1) => ConcreteBound::One,
        argument if argument == Type::int_literal(2) => ConcreteBound::Two,
        argument if argument == Type::any() => ConcreteBound::Any,
        argument => panic!("unexpected bound in the concrete fixture: {argument:?}"),
    }
}

fn owned_relation_input<'db, C>(
    db: &'db TestDb,
    _: &IngredientImpl<C>,
    key: DatabaseKeyIndex,
) -> TypePair<'db>
where
    C: for<'a> salsa::plumbing::function::Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
{
    C::id_to_input(db.zalsa(), key.key_index())
}

fn materialization_input<'db, C>(
    db: &'db TestDb,
    _: &IngredientImpl<C>,
    key: DatabaseKeyIndex,
) -> (Type<'db>, Program<'db>, MaterializationKind)
where
    C: crate::types::mapping::runtime::MaterializationConfiguration,
{
    C::id_to_input(db.zalsa(), key.key_index())
}

fn concrete_request<'db>(
    db: &'db TestDb,
    builder: &ConstraintSetBuilder<'db>,
    key: DatabaseKeyIndex,
) -> ConcreteRequest {
    let materialization = cached_materialization_ingredient(db);
    if materialization.database_key_index(key.key_index()) == key {
        let (bound, program, kind) = materialization_input(db, materialization, key);
        assert_eq!(program, db.program_environment().program(db));
        return ConcreteRequest::Materialization {
            bound: concrete_bound(db, bound),
            kind,
        };
    }
    let assignability = owned_assignability_ingredient(db);
    let equivalence = owned_equivalence_ingredient(db);
    let owned = if assignability.database_key_index(key.key_index()) == key {
        Some((false, owned_relation_input(db, assignability, key)))
    } else if equivalence.database_key_index(key.key_index()) == key {
        Some((true, owned_relation_input(db, equivalence, key)))
    } else {
        None
    };
    if let Some((equivalent, pair)) = owned {
        assert_eq!(pair.program(db), db.program_environment().program(db));
        return ConcreteRequest::Owned {
            equivalent,
            first: concrete_bound(db, pair.first(db)),
            second: concrete_bound(db, pair.second(db)),
        };
    }
    let id = |constraint| builder.storage.borrow().constraint_cache[&constraint].index();
    ConcreteRequest::Sequent(match read_request(db, key) {
        Request::Single(value) => Request::Single(id(value)),
        Request::Pair(left, right) => Request::Pair(id(left), id(right)),
    })
}

fn concrete_reads<'db>(
    db: &'db TestDb,
    builder: &ConstraintSetBuilder<'db>,
    reads: &[Read],
) -> Vec<(Option<ConcreteRequest>, ConcreteRequest)> {
    reads
        .iter()
        .map(|read| {
            assert_eq!(read.status, Status::Final);
            (
                read.parent
                    .map(|parent| concrete_request(db, builder, parent)),
                concrete_request(db, builder, read.key),
            )
        })
        .collect()
}

fn certify_concrete_reads(db: &TestDb, reads: &[Read]) {
    let single = single_sequent_ingredient(db);
    let pair = pair_sequent_ingredient(db);
    let materialization = cached_materialization_ingredient(db);
    let assignability = owned_assignability_ingredient(db);
    let equivalence = owned_equivalence_ingredient(db);
    for read in reads {
        assert_eq!(read.status, Status::Final);
        let certified = if single.database_key_index(read.key.key_index()) == read.key {
            FinalSourceMemo::certify(db as &dyn Db, single, read.key.key_index())
                .expect("completed single sequent remains final")
                .database_key()
        } else if pair.database_key_index(read.key.key_index()) == read.key {
            FinalSourceMemo::certify(db as &dyn Db, pair, read.key.key_index())
                .expect("completed pair sequent remains final")
                .database_key()
        } else if assignability.database_key_index(read.key.key_index()) == read.key {
            FinalSourceMemo::certify(db as &dyn Db, assignability, read.key.key_index())
                .expect("completed owned assignability remains final")
                .database_key()
        } else if equivalence.database_key_index(read.key.key_index()) == read.key {
            FinalSourceMemo::certify(db as &dyn Db, equivalence, read.key.key_index())
                .expect("completed owned equivalence remains final")
                .database_key()
        } else {
            assert_eq!(
                materialization.database_key_index(read.key.key_index()),
                read.key
            );
            FinalSourceMemo::certify(db as &dyn Db, materialization, read.key.key_index())
                .expect("completed materialization remains final")
                .database_key()
        };
        assert_eq!(certified, read.key);
    }
}

fn assert_concrete_builder<'db>(db: &'db TestDb, builder: &ConstraintSetBuilder<'db>) {
    let storage = builder.storage.borrow();
    assert_consistent(db, &storage);
    assert!(storage.compacted.is_none());
    for (id, constraint) in storage.constraints.iter_enumerated() {
        let subject = match constraint {
            Constraint::ConcreteLower(bound) => bound.typevar,
            Constraint::ConcreteUpper(bound) => bound.typevar,
            Constraint::ConcreteEquivalence(bound) => bound.typevar,
            other => panic!("unexpected constraint in the concrete fixture: {other:?}"),
        };
        let support = storage.constraint_support(id);
        assert!(support.is_complete());
        assert_eq!(
            support.iter().collect::<Vec<_>>(),
            [storage.typevar_cache[&subject.identity(db)]]
        );
    }
    for source in &storage.source_orders {
        match source {
            SourceOrder::Constraint(id) => assert!(id.index() < storage.constraints.len()),
            SourceOrder::Ordered(left, right) => {
                assert!(left.index() < storage.source_orders.len());
                assert!(right.index() < storage.source_orders.len());
            }
        }
    }
    for node in &storage.nodes {
        assert!(node.constraint.index() < storage.constraints.len());
        for child in [node.if_true, node.if_uncertain, node.if_false] {
            assert!(child.is_terminal() || child.index() < storage.nodes.len());
        }
    }
    for (id, depth) in &storage.constraint_bound_depth_cache {
        assert!(id.index() < storage.constraints.len());
        assert_eq!(*depth, (1, 0));
    }
}

fn assert_scalar_owners(
    builder: &ConstraintSetBuilder<'_>,
    snapshots: &[ConcreteRelationSnapshot],
) {
    let outer = std::ptr::from_ref(builder).cast::<()>();
    let mut scalar_builders = Vec::new();
    let mut current = None;
    let mut paired = None;
    let mut completed = 0;
    let mut owned = 0;
    for snapshot in snapshots {
        match *snapshot {
            ConcreteRelationSnapshot::Checker {
                resources,
                relation,
                evaluation,
                inferable_none,
                given_never,
            } => {
                assert_eq!(relation, TypeRelation::Assignability);
                assert_eq!(evaluation, TypeVarEvaluation::Lazy);
                assert!(inferable_none && given_never);
                assert!(resources.iter().all(|pointer| !pointer.is_null()));
                assert_ne!(resources[1], outer);
                assert!(!scalar_builders.contains(&resources[1]));
                scalar_builders.push(resources[1]);
                current = Some(resources[1]);
                paired = None;
            }
            ConcreteRelationSnapshot::Pair { builder, node } => {
                assert_eq!(Some(builder), current);
                paired = Some((builder, node));
            }
            ConcreteRelationSnapshot::Always {
                builder,
                node,
                result,
            } => {
                assert_eq!(Some((builder, node)), paired);
                assert!(result);
                completed += 1;
            }
            ConcreteRelationSnapshot::Owned {
                borrowed, always, ..
            } => {
                assert!(!borrowed);
                assert!(always);
                owned += 1;
            }
        }
    }
    assert_eq!(scalar_builders.len(), 2);
    assert_eq!(completed, 2);
    assert!(owned > 0);
}

fn assert_unequal_sequents<'db>(
    builder: &ConstraintSetBuilder<'db>,
    observations: &Observations<'db>,
    reverse_antecedents: bool,
) {
    let id = |constraint| builder.storage.borrow().constraint_cache[&constraint].index();
    let requests = observations
        .fetched
        .iter()
        .map(|(request, map)| match *request {
            Request::Single(constraint) => {
                assert!(map.sequents.is_empty());
                Request::Single(id(constraint))
            }
            Request::Pair(first, second) => {
                let [SequentGroup::Ungrouped(sequents)] = map.sequents.as_slice() else {
                    panic!("an impossible pair emits its original ungrouped sequent");
                };
                let (ante1, ante2) = if reverse_antecedents {
                    (second, first)
                } else {
                    (first, second)
                };
                assert_eq!(
                    sequents.as_ref(),
                    &[Sequent::PairImpossibility { ante1, ante2 }]
                );
                Request::Pair(id(first), id(second))
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        requests,
        [Request::Single(1), Request::Pair(0, 1), Request::Single(0)]
    );
    let antecedents = if reverse_antecedents { (1, 0) } else { (0, 1) };
    assert!(
        observations
            .paths
            .iter()
            .any(|path| path.sequents.iter().any(|sequent| {
                matches!(sequent, Sequent::PairImpossibility { ante1, ante2 }
            if (ante1.index(), ante2.index()) == antecedents)
            }))
    );
}

fn assert_unequal_owned<'db>(
    db: &'db TestDb,
    builder: &ConstraintSetBuilder<'db>,
    selected: ConcreteFormula,
    reversed: bool,
    run: &ConcreteRun<'db>,
    reads: &[(Option<ConcreteRequest>, ConcreteRequest)],
) {
    let equivalent = matches!(selected, ConcreteFormula::UnequalEquivalences);
    let (first, second) = if equivalent && reversed {
        (ConcreteBound::Two, ConcreteBound::One)
    } else {
        (ConcreteBound::One, ConcreteBound::Two)
    };
    let expected = ConcreteRequest::Owned {
        equivalent,
        first,
        second,
    };
    let owned_reads = run
        .record
        .reads
        .iter()
        .zip(reads)
        .filter_map(|(read, (parent, request))| {
            if matches!(request, ConcreteRequest::Owned { .. }) {
                assert_eq!(*request, expected);
                assert_eq!(*parent, Some(ConcreteRequest::Sequent(Request::Pair(0, 1))));
                Some(read)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(owned_reads.len(), if equivalent { 2 } else { 1 });
    for reused in &owned_reads[1..] {
        assert_eq!(reused.key, owned_reads[0].key);
        assert_eq!(reused.stamp, owned_reads[0].stamp);
        assert_eq!(reused.memo_address, owned_reads[0].memo_address);
    }
    assert_eq!(
        executed(&run.record.events)
            .iter()
            .filter(|key| **key == owned_reads[0].key)
            .count(),
        1
    );

    let mut private_builder = None;
    let mut directions = 0;
    let mut packaged = 0;
    let mut produced = 0;
    for observation in &run.owned_relations {
        match *observation {
            ConcreteOwnedSnapshot::Constructed { kind, resources } => {
                assert_eq!(
                    kind,
                    if equivalent {
                        OwnedRelationKind::Equivalence
                    } else {
                        OwnedRelationKind::Assignability
                    }
                );
                assert!(resources.iter().all(|pointer| !pointer.is_null()));
                assert_ne!(resources[1], std::ptr::from_ref(builder).cast());
                assert!(private_builder.replace(resources[1]).is_none());
            }
            ConcreteOwnedSnapshot::Direction {
                source,
                target,
                resources,
                relation,
                evaluation,
            } => {
                assert_eq!(concrete_bound(db, source), first);
                assert_eq!(concrete_bound(db, target), second);
                assert_eq!(Some(resources[1]), private_builder);
                assert_eq!(
                    relation,
                    if equivalent {
                        TypeRelation::Redundancy { pure: true }
                    } else {
                        TypeRelation::Assignability
                    }
                );
                assert_eq!(evaluation, TypeVarEvaluation::Lazy);
                directions += 1;
            }
            ConcreteOwnedSnapshot::Pair { builder, node } => {
                assert_eq!(Some(builder), private_builder);
                assert_eq!(node, ALWAYS_FALSE);
                produced += 1;
            }
            ConcreteOwnedSnapshot::Packaged {
                node,
                source_order_absent,
                inner_absent,
            } => {
                assert_eq!(node, ALWAYS_FALSE);
                assert!(source_order_absent && inner_absent);
                packaged += 1;
            }
            ConcreteOwnedSnapshot::Initial | ConcreteOwnedSnapshot::Recovery => {
                panic!("acyclic unequal relations do not invoke cycle callbacks");
            }
        }
    }
    assert!(private_builder.is_some());
    assert_eq!(directions, 1);
    assert_eq!(packaged, 1);
    assert_eq!(produced, 1);
    let env = db.program_environment();
    let first = TypeFormType::from_type_expression(
        db,
        Type::int_literal(if equivalent && reversed { 2 } else { 1 }),
    );
    let second = TypeFormType::from_type_expression(
        db,
        Type::int_literal(if equivalent && reversed { 1 } else { 2 }),
    );
    let native = if equivalent {
        first.when_constraint_set_equivalent_to_owned(db, &env, second)
    } else {
        first.when_constraint_set_assignable_to_owned(db, &env, second)
    };
    assert!(matches!(native, Cow::Borrowed(_)));
    let mut borrowed = 0;
    let mut scalar_view = None;
    let mut scalar_pair = None;
    let mut scalar_results = 0;
    for observation in &run.relations {
        match *observation {
            ConcreteRelationSnapshot::Owned {
                borrowed: is_borrowed,
                always,
                node,
                address,
                source_order_absent,
                inner_absent,
            } => {
                assert!(is_borrowed && !always && source_order_absent && inner_absent);
                assert_eq!(node, ALWAYS_FALSE);
                assert_eq!(address, std::ptr::from_ref(native.as_ref()).cast());
                borrowed += 1;
            }
            ConcreteRelationSnapshot::Checker {
                resources,
                relation,
                evaluation,
                inferable_none,
                given_never,
            } => {
                assert!(equivalent && inferable_none && given_never);
                assert_eq!(relation, TypeRelation::Assignability);
                assert_eq!(evaluation, TypeVarEvaluation::Lazy);
                assert!(resources.iter().all(|pointer| !pointer.is_null()));
                assert_ne!(resources[1], std::ptr::from_ref(builder).cast());
                assert_ne!(Some(resources[1]), private_builder);
                assert!(scalar_view.replace(resources[1]).is_none());
            }
            ConcreteRelationSnapshot::Pair {
                builder: view,
                node,
            } => {
                assert_eq!(Some(view), scalar_view);
                assert_eq!(node, ALWAYS_FALSE);
                assert!(scalar_pair.replace((view, node)).is_none());
            }
            ConcreteRelationSnapshot::Always {
                builder: view,
                node,
                result,
            } => {
                assert!(equivalent);
                assert_eq!(Some((view, node)), scalar_pair);
                scalar_results += 1;
                assert_ne!(view, std::ptr::from_ref(builder).cast());
                assert_ne!(Some(view), private_builder);
                assert_eq!(node, ALWAYS_FALSE);
                assert!(!result);
            }
        }
    }
    assert_eq!(borrowed, 1);
    assert_eq!(scalar_results, usize::from(equivalent));
}

#[test]
fn original_concrete_solves_keep_materialization_keys_imports_and_scalar_owners() {
    for (selected, reversed, expected_reads, expected_executions) in [
        (ConcreteFormula::EqualRange, false, 12, 8),
        (ConcreteFormula::EqualRange, true, 12, 8),
        (ConcreteFormula::UnequalRange, false, 10, 8),
        (ConcreteFormula::UnequalRange, true, 10, 8),
        (ConcreteFormula::UnequalEquivalences, false, 9, 8),
        (ConcreteFormula::UnequalEquivalences, true, 9, 8),
        (ConcreteFormula::GradualEquivalence, false, 3, 3),
    ] {
        let db = setup_db();
        let [t, ..] = variables(&db);
        let builder = ConstraintSetBuilder::new();
        let construction = capture_concrete_inventory(&db, || {
            concrete_formula(&db, &builder, t, selected, reversed)
        });
        let construction_shape = (
            construction.captured.reads.len(),
            executed(&construction.events).len(),
        );
        assert_eq!(construction_shape, (1, 0));
        let set = construction.captured.value;
        let before = concrete_inventory_constraints(&builder);
        let impossible = matches!(
            selected,
            ConcreteFormula::UnequalRange | ConcreteFormula::UnequalEquivalences
        );
        assert_interior(set);
        let original_root = builder.storage.borrow().interior_node_data(set.node);
        let original_source = set
            .source_order
            .map(|id| builder.storage.borrow().source_order_data(id));
        if matches!(
            selected,
            ConcreteFormula::EqualRange | ConcreteFormula::UnequalRange
        ) {
            let storage = builder.storage.borrow();
            assert_eq!(storage.constraints.len(), 2);
            assert_eq!(
                matches!(
                    storage.constraints[ConstraintId::from_usize(0)],
                    Constraint::ConcreteUpper(_)
                ),
                reversed
            );
            assert_eq!(
                matches!(
                    storage.constraints[ConstraintId::from_usize(1)],
                    Constraint::ConcreteLower(_)
                ),
                reversed
            );
        }
        assert!(builder.storage.borrow().never_satisfied_cache.is_empty());
        assert!(
            builder
                .storage
                .borrow()
                .constraint_bound_depth_cache
                .is_empty()
        );
        let first = execute_concrete(&db, set, usize::MAX, None);
        assert_eq!(
            first.record.result,
            ConcreteOutcome::Returned(Ok(Ok(impossible)))
        );
        assert!(first.record.delivered);
        assert_eq!(first.record.reads.len(), expected_reads);
        assert!(first.record.reads.iter().all(|read| {
            construction
                .captured
                .reads
                .iter()
                .all(|prepared| prepared.key != read.key)
        }));
        assert_eq!(executed(&first.record.events).len(), expected_executions);
        assert_eq!(first.materialization_roots, if impossible { 4 } else { 2 });
        let after = concrete_inventory_constraints(&builder);
        assert_concrete_builder(&db, &builder);
        assert_eq!(
            builder.storage.borrow().interior_node_data(set.node),
            original_root
        );
        assert_eq!(
            set.source_order
                .map(|id| builder.storage.borrow().source_order_data(id)),
            original_source
        );
        assert_eq!(
            builder
                .storage
                .borrow()
                .never_satisfied_cache
                .get(&set.node),
            Some(&impossible)
        );
        let retry = execute_concrete(&db, set, usize::MAX, None);
        assert_eq!(
            retry.record.result,
            ConcreteOutcome::Returned(Ok(Ok(impossible)))
        );
        assert!(retry.record.delivered);
        assert!(retry.record.reads.is_empty());
        assert!(executed(&retry.record.events).is_empty());
        assert!(retry.record.observations.fetched.is_empty());
        assert!(retry.relations.is_empty());
        assert!(retry.owned_relations.is_empty());
        assert_eq!(retry.materialization_roots, 0);
        assert_eq!(concrete_inventory_constraints(&builder), after);

        // Inspection starts only after both solves, so these reads cannot prepare a solver answer.
        let inspection = capture_concrete_inventory(&db, || {
            let actual = concrete_reads(&db, &builder, &first.record.reads);
            certify_concrete_reads(&db, &first.record.reads);
            if matches!(selected, ConcreteFormula::EqualRange) {
                assert_eq!(after.len(), 3);
                let (equality_id, equality, _, _) = after[2];
                assert_eq!(equality_id.index(), 2);
                let Constraint::ConcreteEquivalence(equality_bound) = equality else {
                    panic!("the original range pair derives an equality");
                };
                assert_eq!(equality_bound.typevar, t);
                assert_eq!(equality_bound.provenance, ConstraintProvenance::Evidence);
                assert_eq!(
                    equality_bound.bound,
                    TypeFormType::from_type_expression(&db, Type::int_literal(1))
                );
                assert!(after.iter().all(|(_, _, _, depth)| *depth == Some((1, 0))));
                assert!(
                    first
                        .record
                        .observations
                        .imported
                        .contains(&(equality, equality_id))
                );
                assert!(has_pair_implication(&first.record.observations));
                assert!(first.record.observations.paths.iter().any(|path| path.sequents.iter().any(|sequent| {
                    matches!(sequent, Sequent::PairImplication { ante1, ante2, post, fuel_cost }
                        if ante1.index() < 2 && ante2.index() < 2 && *post == equality_id && *fuel_cost == 1)
                })));
                let requests = first
                    .record
                    .observations
                    .fetched
                    .iter()
                    .map(|(request, _)| {
                        let id = |constraint| {
                            builder.storage.borrow().constraint_cache[&constraint].index()
                        };
                        match *request {
                            Request::Single(value) => Request::Single(id(value)),
                            Request::Pair(left, right) => Request::Pair(id(left), id(right)),
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    requests
                        .iter()
                        .filter(|request| matches!(request, Request::Single(2)))
                        .count(),
                    1
                );
                assert!(
                    requests.iter().any(|request| matches!(
                        request,
                        Request::Pair(0, 2) | Request::Pair(2, 0)
                    ))
                );
                assert!(
                    requests.iter().any(|request| matches!(
                        request,
                        Request::Pair(1, 2) | Request::Pair(2, 1)
                    ))
                );
                assert!(actual.iter().any(|(parent, request)| parent.is_some()
                    && matches!(request, ConcreteRequest::Materialization { .. })));
                assert_scalar_owners(&builder, &first.relations);
                assert!(first.owned_relations.is_empty());
            } else if impossible {
                assert_eq!(after, before);
                assert_eq!(after.len(), 2);
                assert!(after.iter().all(|(_, _, _, depth)| depth.is_none()));
                let antecedent_order =
                    if matches!(selected, ConcreteFormula::UnequalRange) && reversed {
                        [1, 0]
                    } else {
                        [0, 1]
                    };
                let expected_imports = antecedent_order.map(|index| {
                    let (id, constraint, _, _) = before[index];
                    (constraint, id)
                });
                assert_eq!(first.record.observations.imported, expected_imports);
                assert!(first.record.observations.before_depth.is_empty());
                assert!(first.record.observations.after_depth.is_empty());
                assert!(first.record.observations.before_depth_cache.is_empty());
                // Range rules retain lower/upper antecedent order even when the query key is reversed.
                assert_unequal_sequents(
                    &builder,
                    &first.record.observations,
                    matches!(selected, ConcreteFormula::UnequalRange) && reversed,
                );
                assert_unequal_owned(&db, &builder, selected, reversed, &first, &actual);
            } else {
                assert_eq!(after.len(), 1);
                assert!(first.relations.is_empty());
                assert!(first.owned_relations.is_empty());
                let [(Request::Single(_), map)] = first.record.observations.fetched.as_slice()
                else {
                    panic!("gradual equality fetches its original single sequent");
                };
                assert!(map.sequents.is_empty());
                let env = db.program_environment();
                let bound = TypeFormType::from_type_expression(&db, Type::any());
                let bottom = bound.materialization(&db, &env, MaterializationKind::Bottom);
                let top = bound.materialization(&db, &env, MaterializationKind::Top);
                assert_eq!(bottom, TypeFormType::from_type_expression(&db, Type::Never));
                assert_eq!(top, TypeFormType::from_type_expression(&db, Type::object()));
                assert_ne!(bottom, top);
                let materialization = cached_materialization_ingredient(&db);
                let keys = first
                    .record
                    .reads
                    .iter()
                    .filter(|read| {
                        materialization.database_key_index(read.key.key_index()) == read.key
                    })
                    .map(|read| read.key)
                    .collect::<Vec<_>>();
                assert_eq!(keys.len(), 2);
                assert_ne!(keys[0], keys[1]);
                assert_eq!(
                    materialization_input(&db, materialization, keys[0]),
                    (bound, env.program(&db), MaterializationKind::Bottom)
                );
                assert_eq!(
                    materialization_input(&db, materialization, keys[1]),
                    (bound, env.program(&db), MaterializationKind::Top)
                );
            }
            actual
        });
        let ordinary = setup_db();
        let ordinary_env = ordinary.program_environment();
        let [t, ..] = variables(&ordinary);
        let ordinary_builder = ConstraintSetBuilder::new();
        let construction = capture_concrete_inventory(&ordinary, || {
            concrete_formula(&ordinary, &ordinary_builder, t, selected, reversed)
        });
        assert_eq!(
            (
                construction.captured.reads.len(),
                executed(&construction.events).len()
            ),
            construction_shape
        );
        let ordinary_set = construction.captured.value;
        let expected = capture_concrete_inventory(&ordinary, || {
            ordinary_set.is_never_satisfied(&ordinary, &ordinary_env)
        });
        assert_eq!(expected.captured.value, impossible);
        assert_eq!(expected.captured.reads.len(), expected_reads);
        assert_eq!(executed(&expected.events).len(), expected_executions);
        let ordinary_retry = capture_concrete_inventory(&ordinary, || {
            ordinary_set.is_never_satisfied(&ordinary, &ordinary_env)
        });
        assert_eq!(ordinary_retry.captured.value, impossible);
        assert!(ordinary_retry.captured.reads.is_empty());
        assert!(executed(&ordinary_retry.events).is_empty());
        assert_eq!(
            inspection.captured.value,
            concrete_reads(&ordinary, &ordinary_builder, &expected.captured.reads)
        );
        assert_eq!(
            executed(&first.record.events)
                .into_iter()
                .map(|key| concrete_request(&db, &builder, key))
                .collect::<Vec<_>>(),
            executed(&expected.events)
                .into_iter()
                .map(|key| concrete_request(&ordinary, &ordinary_builder, key))
                .collect::<Vec<_>>(),
        );
        eprintln!(
            "concrete {selected:?}, reversed={reversed}: reads={}, executions={}, debit={}, admission={:?}",
            first.record.reads.len(),
            executed(&first.record.events).len(),
            first.record.entry_remaining.unwrap() - first.record.exit_remaining.unwrap(),
            first.record.work
        );
    }
}

#[test]
fn relation_terminal_satisfaction_forwards_real_nonterminal_sets_and_kind() {
    for always in [false, true] {
        let db = setup_db();
        let vars = variables(&db);
        let builder = ConstraintSetBuilder::new();
        let set = formula(&db, &builder, vars, Formula::Implication, false);
        assert_interior(set);
        let original_root = builder.storage.borrow().interior_node_data(set.node);
        let original_source = set.source_order;
        let result = execute_concrete_entry(
            &db,
            set,
            usize::MAX,
            None,
            ConcreteEntry::RelationSatisfaction { always },
        );
        assert_eq!(
            result.record.result,
            ConcreteOutcome::Returned(Ok(Ok(always)))
        );
        assert!(result.record.delivered);
        assert!(!result.record.reads.is_empty());
        certify_sequent_reads(&db, &result.record.reads);
        assert_eq!(
            builder.storage.borrow().interior_node_data(set.node),
            original_root
        );
        assert_eq!(set.source_order, original_source);
        assert_typevar_builder_support(&db, &builder);
        assert_eq!(
            builder
                .storage
                .borrow()
                .never_satisfied_cache
                .get(&set.node)
                .copied(),
            (!always).then_some(false)
        );
    }
}

#[test]
fn concrete_solver_refusals_preserve_completed_prefixes_at_every_effect() {
    let calibration_db = setup_db();
    let [t, ..] = variables(&calibration_db);
    let calibration_builder = ConstraintSetBuilder::new();
    let calibration_set = concrete_formula(
        &calibration_db,
        &calibration_builder,
        t,
        ConcreteFormula::EqualRange,
        false,
    );
    let calibration = execute_concrete(&calibration_db, calibration_set, usize::MAX, None);
    assert_eq!(
        calibration.record.result,
        ConcreteOutcome::Returned(Ok(Ok(false)))
    );
    for required in [
        ConcreteBoundary::Import,
        ConcreteBoundary::Depth,
        ConcreteBoundary::DepthComplete,
        ConcreteBoundary::DepthPublish,
        ConcreteBoundary::ScalarChecker,
        ConcreteBoundary::ScalarPair,
        ConcreteBoundary::ScalarAlways,
        ConcreteBoundary::OwnedResult,
        ConcreteBoundary::Single,
        ConcreteBoundary::Pair,
        ConcreteBoundary::NeverPublish,
    ] {
        assert!(
            calibration
                .boundaries
                .iter()
                .any(|(boundary, _)| *boundary == required),
            "missing real {required:?} boundary"
        );
    }
    let first = calibration.boundaries[0].1;
    let publication = calibration
        .boundaries
        .iter()
        .find_map(|(boundary, index)| {
            (*boundary == ConcreteBoundary::NeverPublish).then_some(*index)
        })
        .unwrap();
    // A cold root cache performs lookup, relocation, reservation, reserve commit, and insertion.
    // Stop at insertion: failures after a completed publication have a different invariant.
    let last = publication + 4;
    assert_eq!(
        calibration.record.work[last],
        ExecutionWork::Work { units: 1 }
    );
    for index in (first..=last).filter(|index| {
        matches!(
            calibration.record.work[*index],
            ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
        )
    }) {
        let db = setup_db();
        let [t, ..] = variables(&db);
        let builder = ConstraintSetBuilder::new();
        let set = concrete_formula(&db, &builder, t, ConcreteFormula::EqualRange, false);
        let original_root = builder.storage.borrow().interior_node_data(set.node);
        let original_source = set
            .source_order
            .map(|id| builder.storage.borrow().source_order_data(id));
        let failed = execute_concrete(&db, set, usize::MAX, Some((index, ConcreteFault::Refuse)));
        assert_eq!(
            failed.record.result,
            ConcreteOutcome::Returned(Err(Incomplete::Allowance)),
            "admission {index}"
        );
        assert!(!failed.record.delivered);
        assert_eq!(
            &failed.record.work[..=index],
            &calibration.record.work[..=index]
        );
        assert!(
            !builder
                .storage
                .borrow()
                .never_satisfied_cache
                .contains_key(&set.node)
        );
        assert_eq!(
            builder.storage.borrow().interior_node_data(set.node),
            original_root
        );
        assert_eq!(
            set.source_order
                .map(|id| builder.storage.borrow().source_order_data(id)),
            original_source
        );
        assert_concrete_builder(&db, &builder);
        certify_concrete_reads(&db, &failed.record.reads);
        let retained = concrete_inventory_constraints(&builder);
        for (_, _, _, cached_depth) in &retained {
            if let Some(depth) = cached_depth {
                assert!(
                    failed
                        .record
                        .observations
                        .after_depth
                        .iter()
                        .any(|(_, completed)| completed == depth)
                );
            }
        }
        let retry = execute_concrete(&db, set, usize::MAX, None);
        assert_eq!(
            retry.record.result,
            ConcreteOutcome::Returned(Ok(Ok(false)))
        );
        assert_concrete_builder(&db, &builder);
        let complete = concrete_inventory_constraints(&builder);
        assert_eq!(complete.len(), 3);
        for ((id, constraint, support, depth), (actual_id, actual, actual_support, actual_depth)) in
            retained.iter().zip(&complete)
        {
            assert_eq!(
                (id, constraint, support),
                (actual_id, actual, actual_support)
            );
            if depth.is_some() {
                assert_eq!(depth, actual_depth);
            }
        }
        certify_concrete_reads(&db, &failed.record.reads);
        for previous in &failed.record.reads {
            for reused in retry
                .record
                .reads
                .iter()
                .filter(|read| read.key == previous.key)
            {
                assert_eq!(reused.memo_address, previous.memo_address);
                assert_eq!(reused.stamp, previous.stamp);
            }
        }
        let warm = execute_concrete(&db, set, usize::MAX, None);
        assert_eq!(warm.record.result, ConcreteOutcome::Returned(Ok(Ok(false))));
        assert!(warm.record.reads.is_empty());
        assert!(executed(&warm.record.events).is_empty());
    }
}

#[test]
fn concrete_solver_native_panics_drain_children_before_root_cleanup() {
    let calibration_db = setup_db();
    let [t, ..] = variables(&calibration_db);
    let builder = ConstraintSetBuilder::new();
    let set = concrete_formula(
        &calibration_db,
        &builder,
        t,
        ConcreteFormula::EqualRange,
        false,
    );
    let calibration = execute_concrete(&calibration_db, set, usize::MAX, None);
    let mut indices = Vec::new();
    for (boundary, start) in &calibration.boundaries {
        if matches!(
            boundary,
            ConcreteBoundary::Import
                | ConcreteBoundary::Depth
                | ConcreteBoundary::DepthPublish
                | ConcreteBoundary::ScalarChecker
                | ConcreteBoundary::ScalarPair
                | ConcreteBoundary::Single
                | ConcreteBoundary::Pair
                | ConcreteBoundary::NeverPublish
        ) {
            let index = (*start..calibration.record.work.len())
                .find(|index| {
                    matches!(
                        calibration.record.work[*index],
                        ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
                    )
                })
                .unwrap();
            let poisons_query = matches!(
                boundary,
                ConcreteBoundary::ScalarChecker | ConcreteBoundary::ScalarPair
            );
            if !indices.iter().any(|(previous, _)| *previous == index) {
                indices.push((index, poisons_query));
            }
        }
    }
    assert!(!indices.is_empty());
    for (index, poisons_query) in indices {
        let db = setup_db();
        let [t, ..] = variables(&db);
        let builder = ConstraintSetBuilder::new();
        let set = concrete_formula(&db, &builder, t, ConcreteFormula::EqualRange, false);
        let failed = execute_concrete(&db, set, usize::MAX, Some((index, ConcreteFault::Panic)));
        assert_eq!(failed.record.result, ConcreteOutcome::NativePanic);
        assert!(!failed.record.delivered);
        assert!(
            !builder
                .storage
                .borrow()
                .never_satisfied_cache
                .contains_key(&set.node)
        );
        assert_concrete_builder(&db, &builder);
        certify_concrete_reads(&db, &failed.record.reads);
        let retry = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            execute_concrete(&db, set, usize::MAX, None)
        }));
        if poisons_query {
            assert!(
                matches!(retry, Err(salsa::Cancelled::PropagatedPanic)),
                "a native panic inside the pair query poisons the current revision"
            );
        } else {
            assert_eq!(
                retry.unwrap().record.result,
                ConcreteOutcome::Returned(Ok(Ok(false)))
            );
        }
        assert_concrete_builder(&db, &builder);
    }
}

#[test]
fn unequal_solver_refusal_and_native_panic_preserve_semantic_impossibility_distinctions() {
    for selected in [
        ConcreteFormula::UnequalRange,
        ConcreteFormula::UnequalEquivalences,
    ] {
        let calibration_db = setup_db();
        let [t, ..] = variables(&calibration_db);
        let calibration_builder = ConstraintSetBuilder::new();
        let set = concrete_formula(&calibration_db, &calibration_builder, t, selected, false);
        let calibration = execute_concrete(&calibration_db, set, usize::MAX, None);
        assert_eq!(
            calibration.record.result,
            ConcreteOutcome::Returned(Ok(Ok(true)))
        );
        let required = [
            ConcreteBoundary::OwnedConstructed,
            ConcreteBoundary::OwnedDirection,
            ConcreteBoundary::OwnedPair,
            ConcreteBoundary::OwnedPackaged,
            ConcreteBoundary::OwnedResult,
            ConcreteBoundary::NeverPublish,
        ];
        for required in required {
            assert!(
                calibration
                    .boundaries
                    .iter()
                    .any(|(boundary, _)| *boundary == required)
            );
        }
        if matches!(selected, ConcreteFormula::UnequalEquivalences) {
            assert!(
                calibration
                    .boundaries
                    .iter()
                    .any(|(boundary, _)| *boundary == ConcreteBoundary::ScalarAlways)
            );
        }
        let mut indices = Vec::new();
        for (boundary, start) in &calibration.boundaries {
            if required.contains(boundary) || *boundary == ConcreteBoundary::ScalarAlways {
                let index = (*start..calibration.record.work.len())
                    .find(|index| {
                        matches!(
                            calibration.record.work[*index],
                            ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
                        )
                    })
                    .expect("each observed completion has a subsequent admitted boundary");
                let poisons_query = *boundary != ConcreteBoundary::NeverPublish;
                if !indices.iter().any(|(previous, _)| *previous == index) {
                    indices.push((index, poisons_query));
                }
            }
        }
        for (index, poisons_query) in indices {
            for fault in [ConcreteFault::Refuse, ConcreteFault::Panic] {
                let db = setup_db();
                let [t, ..] = variables(&db);
                let builder = ConstraintSetBuilder::new();
                let set = concrete_formula(&db, &builder, t, selected, false);
                let before = concrete_inventory_constraints(&builder);
                let original_root = builder.storage.borrow().interior_node_data(set.node);
                let source_order = set.source_order;
                let stamp = Stamp::current(&db);
                let failed = execute_concrete(&db, set, usize::MAX, Some((index, fault)));
                assert_eq!(
                    failed.record.result,
                    match fault {
                        ConcreteFault::Refuse =>
                            ConcreteOutcome::Returned(Err(Incomplete::Allowance)),
                        ConcreteFault::Panic => ConcreteOutcome::NativePanic,
                    }
                );
                assert!(!failed.record.delivered);
                assert_eq!(
                    &failed.record.work[..=index],
                    &calibration.record.work[..=index]
                );
                assert!(
                    !builder
                        .storage
                        .borrow()
                        .never_satisfied_cache
                        .contains_key(&set.node)
                );
                assert_eq!(
                    builder.storage.borrow().interior_node_data(set.node),
                    original_root
                );
                assert_eq!(set.source_order, source_order);
                assert_eq!(concrete_inventory_constraints(&builder), before);
                assert_concrete_builder(&db, &builder);
                certify_concrete_reads(&db, &failed.record.reads);
                let retry = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                    execute_concrete(&db, set, usize::MAX, None)
                }));
                if fault == ConcreteFault::Panic && poisons_query {
                    assert!(matches!(retry, Err(salsa::Cancelled::PropagatedPanic)));
                } else {
                    let retry = retry.unwrap();
                    assert_eq!(retry.record.result, ConcreteOutcome::Returned(Ok(Ok(true))));
                    for previous in &failed.record.reads {
                        for reused in retry
                            .record
                            .reads
                            .iter()
                            .filter(|read| read.key == previous.key)
                        {
                            assert_eq!(reused.memo_address, previous.memo_address);
                            assert_eq!(reused.stamp, previous.stamp);
                        }
                    }
                    let warm = execute_concrete(&db, set, usize::MAX, None);
                    assert_eq!(warm.record.result, ConcreteOutcome::Returned(Ok(Ok(true))));
                    assert!(warm.record.reads.is_empty());
                    assert!(executed(&warm.record.events).is_empty());
                }
                assert_eq!(Stamp::current(&db), stamp);
                assert_eq!(concrete_inventory_constraints(&builder), before);
            }
        }
    }
}

#[test]
fn concrete_root_allowance_cutoff_retains_all_children_and_depths_for_retry() {
    let calibration_db = setup_db();
    let [t, ..] = variables(&calibration_db);
    let builder = ConstraintSetBuilder::new();
    let set = concrete_formula(
        &calibration_db,
        &builder,
        t,
        ConcreteFormula::EqualRange,
        false,
    );
    let calibration = execute_concrete(&calibration_db, set, usize::MAX, None);
    let [(node, Some(remaining))] = calibration
        .record
        .observations
        .before_never_cache
        .as_slice()
    else {
        panic!("the successful solve reaches its root publication exactly once");
    };
    assert_eq!(*node, set.node);
    let cutoff = usize::MAX - *remaining;
    let db = setup_db();
    let [t, ..] = variables(&db);
    let builder = ConstraintSetBuilder::new();
    let set = concrete_formula(&db, &builder, t, ConcreteFormula::EqualRange, false);
    let failed = execute_concrete(&db, set, cutoff, None);
    assert_eq!(
        failed.record.result,
        ConcreteOutcome::Returned(Err(Incomplete::Allowance))
    );
    assert!(!failed.record.delivered);
    assert_eq!(failed.record.reads.len(), 12);
    assert_eq!(executed(&failed.record.events).len(), 8);
    assert!(builder.storage.borrow().never_satisfied_cache.is_empty());
    assert_eq!(
        builder.storage.borrow().constraint_bound_depth_cache.len(),
        3
    );
    assert_concrete_builder(&db, &builder);
    certify_concrete_reads(&db, &failed.record.reads);
    let retained = concrete_inventory_constraints(&builder);
    let retry = execute_concrete(&db, set, usize::MAX, None);
    assert_eq!(
        retry.record.result,
        ConcreteOutcome::Returned(Ok(Ok(false)))
    );
    assert!(executed(&retry.record.events).is_empty());
    assert_eq!(concrete_inventory_constraints(&builder), retained);
    let warm = execute_concrete(&db, set, usize::MAX, None);
    assert_eq!(warm.record.result, ConcreteOutcome::Returned(Ok(Ok(false))));
    assert!(warm.record.reads.is_empty());
    assert!(executed(&warm.record.events).is_empty());
}

#[derive(Clone, Copy, Debug)]
enum Formula {
    ContradictoryChain,
    PositiveChain,
    Implication,
    Disjunction,
}
fn formula<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    [t, u, v, w]: [BoundTypeVarInstance<'db>; 4],
    formula: Formula,
    reversed: bool,
) -> ConstraintSet<'db, 'c> {
    let env = db.program_environment();
    let range = |left, right| {
        ConstraintSet::constrain_typevar_upper_bound(db, &env, builder, left, Type::TypeVar(right))
    };
    let (first, second) = if reversed {
        (range(u, v), range(t, u))
    } else {
        (range(t, u), range(u, v))
    };
    match formula {
        Formula::ContradictoryChain => first
            .and(db, builder, || second)
            .and(db, builder, || range(v, w))
            .and(db, builder, || range(t, w).negate(db, builder)),
        Formula::PositiveChain => first
            .and(db, builder, || second)
            .and(db, builder, || range(v, w)),
        Formula::Implication => first
            .and(db, builder, || second)
            .implies(db, builder, || range(t, v)),
        Formula::Disjunction => first.or(db, builder, || second),
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NormalConstraint {
    equivalence: bool,
    provenance: ConstraintProvenance,
    left: usize,
    right: usize,
}
fn normalize<'db>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    value: Constraint<'db>,
) -> NormalConstraint {
    let variable = |value: BoundTypeVarInstance<'db>| {
        vars.iter()
            .position(|candidate| candidate.identity(db) == value.identity(db))
            .unwrap()
    };
    let (equivalence, provenance, left, right) = match value {
        Constraint::TypeVarRange(bound) => (false, bound.provenance, bound.left, bound.right),
        Constraint::TypeVarEquivalence(bound) => (true, bound.provenance, bound.left, bound.right),
        other => panic!("typevar-only solve reached a concrete bound: {other:?}"),
    };
    NormalConstraint {
        equivalence,
        provenance,
        left: variable(left),
        right: variable(right),
    }
}
fn normalize_request<'db>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    request: Request<Constraint<'db>>,
) -> Request<NormalConstraint> {
    match request {
        Request::Single(value) => Request::Single(normalize(db, vars, value)),
        Request::Pair(left, right) => {
            Request::Pair(normalize(db, vars, left), normalize(db, vars, right))
        }
    }
}
fn single_input<'db, C>(
    db: &'db TestDb,
    _: &IngredientImpl<C>,
    key: DatabaseKeyIndex,
) -> Constraint<'db>
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    C::id_to_input(db.zalsa(), key.key_index()).1
}
fn pair_input<'db, C>(
    db: &'db TestDb,
    _: &IngredientImpl<C>,
    key: DatabaseKeyIndex,
) -> (Constraint<'db>, Constraint<'db>)
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
{
    let (_, left, right) = C::id_to_input(db.zalsa(), key.key_index());
    (left, right)
}
fn read_request<'db>(db: &'db TestDb, key: DatabaseKeyIndex) -> Request<Constraint<'db>> {
    let single = single_sequent_ingredient(db);
    if single.database_key_index(key.key_index()) == key {
        Request::Single(single_input(db, single, key))
    } else {
        let pair = pair_sequent_ingredient(db);
        assert_eq!(
            pair.database_key_index(key.key_index()),
            key,
            "unexpected non-sequent query"
        );
        let (left, right) = pair_input(db, pair, key);
        Request::Pair(left, right)
    }
}
fn normalized_reads<'db>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    reads: &[Read],
) -> Vec<Request<NormalConstraint>> {
    reads
        .iter()
        .map(|read| {
            assert_eq!(
                read.parent, None,
                "a direct solver has no fabricated consuming query"
            );
            assert_eq!(read.status, Status::Final);
            normalize_request(db, vars, read_request(db, read.key))
        })
        .collect()
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
fn normalized_executions<'db>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    events: &[Event],
) -> Vec<Request<NormalConstraint>> {
    executed(events)
        .into_iter()
        .map(|key| normalize_request(db, vars, read_request(db, key)))
        .collect()
}
fn compare_ordinary<'db>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    record: &Record<'db>,
    selected: Formula,
    reversed: bool,
    kind: SatisfactionKind,
) {
    let ordinary = setup_db();
    let ordinary_vars = variables(&ordinary);
    let builder = ConstraintSetBuilder::new();
    let set = formula(&ordinary, &builder, ordinary_vars, selected, reversed);
    let env = ordinary.program_environment();
    let mut reader = ordinary.clone();
    reader.clear_salsa_events();
    let expected = prepared_source_probe::capture(&ordinary, || match kind {
        SatisfactionKind::Never => set.is_never_satisfied(&ordinary, &env),
        SatisfactionKind::Always => set.is_always_satisfied(&ordinary, &env),
    })
    .unwrap();
    expected.check_root_reads().unwrap();
    assert_eq!(record.result, expected.value);
    let actual = normalized_reads(db, vars, &record.reads);
    let expected_reads = normalized_reads(&ordinary, ordinary_vars, &expected.reads);
    assert_eq!(
        actual, expected_reads,
        "preserve every request, orientation and repetition"
    );
    assert_eq!(
        normalized_executions(db, vars, &record.events),
        normalized_executions(&ordinary, ordinary_vars, &reader.take_salsa_events())
    );
    assert!(!actual.is_empty());
    if !record.observations.fetched.is_empty() {
        let observed = record
            .observations
            .fetched
            .iter()
            .map(|(request, _)| normalize_request(db, vars, *request))
            .collect::<Vec<_>>();
        assert_eq!(actual, observed);
    }
    let executed = executed(&record.events);
    assert!(!executed.is_empty(), "the sequent bodies were cold");
    assert_eq!(
        record
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::DidInternValue { .. }))
            .count(),
        executed.len()
    );
    assert!(
        record
            .work
            .iter()
            .any(|work| matches!(work, ExecutionWork::Work { units } if *units > 0))
    );
    let mut counts = [0usize; 4];
    let mut payloads_and_units = [0usize; 3];
    for work in &record.work {
        match *work {
            ExecutionWork::Task { requested_bytes } => {
                counts[0] += 1;
                payloads_and_units[0] += requested_bytes;
            }
            ExecutionWork::Resource { requested_bytes } => {
                counts[1] += 1;
                payloads_and_units[1] += requested_bytes;
            }
            ExecutionWork::Work { units } => {
                counts[2] += 1;
                payloads_and_units[2] += units;
            }
            ExecutionWork::Poll => counts[3] += 1,
        }
    }
    eprintln!(
        "{selected:?}, reversed={reversed}: Task/Resource/Work/Poll counts={counts:?}, requested Task/Resource bytes and Work units={payloads_and_units:?}; admission sequence={:?}",
        record.work
    );
}
fn assert_interior(set: ConstraintSet<'_, '_>) {
    assert!(matches!(set.node.node(), Node::Interior(_)));
    assert!(!set.is_trivially_never_satisfied());
    assert!(!set.is_trivially_always_satisfied());
}
fn assert_source_seed<'db, T>(
    db: &'db TestDb,
    vars: [BoundTypeVarInstance<'db>; 4],
    builder: &ConstraintSetBuilder<'db>,
    set: ConstraintSet<'db, '_>,
    record: &Record<'db, T>,
) {
    let storage = builder.storage.borrow();
    let mut scan = SourceOrderScan::new(set.source_order);
    while scan
        .advance_with(
            &storage,
            &mut crate::types::constraints::control::Unrestricted,
        )
        .unwrap()
        .is_continue()
    {}
    let expected = scan
        .result
        .iter()
        .map(|id| normalize(db, vars, storage.constraint_data(*id)))
        .collect::<Vec<_>>();
    let first = record.observations.paths.first().unwrap();
    assert!(matches!(first.event, PathTrace::EnterEdge { .. }));
    let actual = first
        .discovered
        .iter()
        .map(|(id, _)| normalize(db, vars, storage.constraint_data(*id)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(first.discovered.iter().all(|(_, processed)| !processed));
}

#[test]
fn cold_transitive_nonterminal_solve_imports_real_sequents() {
    for selected in [Formula::ContradictoryChain, Formula::PositiveChain] {
        let db = setup_db();
        let vars = variables(&db);
        let program = db.program_environment().program(&db);
        let builder = ConstraintSetBuilder::new();
        let set = formula(&db, &builder, vars, selected, false);
        assert_interior(set);
        let intermediates = [(vars[0], vars[2]), (vars[1], vars[3])].map(|(left, right)| {
            Constraint::from(TypeVarRangeBound::new(
                &db,
                ConstraintProvenance::Evidence,
                left,
                right,
            ))
        });
        assert!(intermediates.iter().all(|value| {
            !builder
                .storage
                .borrow()
                .constraint_cache
                .contains_key(value)
        }));
        let record = execute(&db, program, set, SatisfactionKind::Never, true);
        assert_eq!(
            record.result,
            matches!(selected, Formula::ContradictoryChain)
        );
        assert!(record.observations.fetched.iter().any(|(request, map)| matches!(request, Request::Pair(..))
            && map.sequents.iter().any(|group| matches!(group, SequentGroup::Ungrouped(sequents)
                if sequents.iter().any(|sequent| matches!(sequent, Sequent::PairImplication { .. }))))));
        assert!(intermediates.iter().any(|value| {
            builder
                .storage
                .borrow()
                .constraint_cache
                .get(value)
                .is_some_and(|id| record.observations.imported.contains(&(*value, *id)))
        }));
        assert!(record.observations.paths.iter().any(|path| {
            path.sequents
                .iter()
                .any(|sequent| matches!(sequent, Sequent::PairImplication { .. }))
        }));
        assert_source_seed(&db, vars, &builder, set, &record);
        compare_ordinary(&db, vars, &record, selected, false, SatisfactionKind::Never);
    }
    // Exercise the same entry without instrumentation on a fresh, cold builder/database.
    let db = setup_db();
    let vars = variables(&db);
    let builder = ConstraintSetBuilder::new();
    let set = formula(&db, &builder, vars, Formula::PositiveChain, false);
    let record = execute(
        &db,
        db.program_environment().program(&db),
        set,
        SatisfactionKind::Never,
        false,
    );
    assert!(!record.result);
    compare_ordinary(
        &db,
        vars,
        &record,
        Formula::PositiveChain,
        false,
        SatisfactionKind::Never,
    );
}

#[test]
fn always_traversal_preserves_uncertain_or_and_each_source_order() {
    for (selected, reversed) in [
        (Formula::Implication, false),
        (Formula::Disjunction, false),
        (Formula::Implication, true),
    ] {
        let db = setup_db();
        let vars = variables(&db);
        let builder = ConstraintSetBuilder::new();
        let set = formula(&db, &builder, vars, selected, reversed);
        assert_interior(set);
        if matches!(selected, Formula::Disjunction) {
            assert!(
                !builder
                    .storage
                    .borrow()
                    .interior_node_data(set.node)
                    .if_uncertain
                    .is_terminal()
            );
        }
        let record = execute(
            &db,
            db.program_environment().program(&db),
            set,
            SatisfactionKind::Always,
            true,
        );
        assert_eq!(record.result, matches!(selected, Formula::Implication));
        assert!(
            !record.observations.or_nodes.is_empty(),
            "negated traversal must execute the original node OR"
        );
        if matches!(selected, Formula::Disjunction) {
            assert!(
                record
                    .observations
                    .or_nodes
                    .iter()
                    .any(|(_, uncertain, _)| !uncertain.is_terminal())
            );
        }
        assert!(
            builder.storage.borrow().never_satisfied_cache.is_empty(),
            "Always does not publish Never results"
        );
        assert_source_seed(&db, vars, &builder, set, &record);
        compare_ordinary(
            &db,
            vars,
            &record,
            selected,
            reversed,
            SatisfactionKind::Always,
        );
    }
}

fn assert_typevar_builder_support<'db>(db: &'db TestDb, builder: &ConstraintSetBuilder<'db>) {
    let storage = builder.storage.borrow();
    assert!(storage.compacted.is_none());
    assert_consistent(db, &storage);
    for support in &storage.supports {
        assert!(support.iter().all(|id| id.index() < storage.typevars.len()));
    }
    for (id, constraint) in storage.constraints.iter_enumerated() {
        let (left, right) = match constraint {
            Constraint::TypeVarRange(bound) => (bound.left, bound.right),
            Constraint::TypeVarEquivalence(bound) => (bound.left, bound.right),
            other => panic!("the fixture only publishes typevar constraints: {other:?}"),
        };
        let mut expected = vec![
            storage.typevar_cache[&left.identity(db)],
            storage.typevar_cache[&right.identity(db)],
        ];
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(
            storage.constraint_support(id).iter().collect::<Vec<_>>(),
            expected
        );
    }
    for node in &storage.nodes {
        for child in [node.if_true, node.if_uncertain, node.if_false] {
            assert!(child.is_terminal() || child.index() < storage.nodes.len());
        }
    }
    for source in &storage.source_orders {
        match source {
            SourceOrder::Constraint(id) => assert!(id.index() < storage.constraints.len()),
            SourceOrder::Ordered(left, right) => {
                assert!(left.index() < storage.source_orders.len());
                assert!(right.index() < storage.source_orders.len());
            }
        }
    }
    for (id, depth) in &storage.constraint_bound_depth_cache {
        assert!(id.index() < storage.constraints.len());
        assert_eq!(*depth, (0, 0));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProducedCache {
    Depth,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheValue {
    Depth((u16, u16)),
    Never(bool),
}

#[derive(Debug, Eq, PartialEq)]
struct ProducedCacheState {
    entries: Vec<(usize, CacheValue)>,
    capacity: usize,
}

impl ProducedCache {
    fn state(self, builder: &ConstraintSetBuilder<'_>) -> ProducedCacheState {
        let storage = builder.storage.borrow();
        let (mut entries, capacity): (Vec<_>, _) = match self {
            Self::Depth => (
                storage
                    .constraint_bound_depth_cache
                    .iter()
                    .map(|(id, depth)| (id.index(), CacheValue::Depth(*depth)))
                    .collect(),
                storage.constraint_bound_depth_cache.capacity(),
            ),
            Self::Never => (
                storage
                    .never_satisfied_cache
                    .iter()
                    .map(|(id, value)| (id.index(), CacheValue::Never(*value)))
                    .collect(),
                storage.never_satisfied_cache.capacity(),
            ),
        };
        entries.sort_unstable_by_key(|(id, _)| *id);
        ProducedCacheState { entries, capacity }
    }

    fn key(self, input: ConstraintSet<'_, '_>) -> usize {
        match self {
            Self::Depth => input
                .builder
                .storage
                .borrow()
                .interior_node_data(input.node)
                .constraint
                .index(),
            Self::Never => input.node.index(),
        }
    }

    fn growth(self, before: &ProducedCacheState) -> GrowthPlan {
        let len = before.entries.len();
        match self {
            Self::Depth => {
                map_growth::<ConstraintId, (u16, u16), RunError>(len, before.capacity, len + 1)
            }
            Self::Never => map_growth::<NodeId, bool, RunError>(len, before.capacity, len + 1),
        }
        .expect("the finite cache fixture has representable growth")
    }

    fn expected(self) -> CacheValue {
        match self {
            Self::Depth => CacheValue::Depth((0, 0)),
            Self::Never => CacheValue::Never(false),
        }
    }
}

fn produced_cache_inputs<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    selected: ProducedCache,
    count: usize,
) -> Vec<ConstraintSet<'db, 'c>> {
    let env = db.program_environment();
    let result = (0..count)
        .map(|index| {
            let vars = ["T", "U", "V", "W"].map(|name| {
                BoundTypeVarInstance::synthetic(
                    db,
                    &env,
                    Name::new(format!("Cache{index}{name}")),
                    TypeVarVariance::Invariant,
                )
            });
            let input = match selected {
                ProducedCache::Depth => ConstraintSet::constrain_typevar_upper_bound(
                    db,
                    &env,
                    builder,
                    vars[0],
                    Type::TypeVar(vars[1]),
                ),
                ProducedCache::Never => formula(db, builder, vars, Formula::PositiveChain, false),
            };
            assert_interior(input);
            input
        })
        .collect();
    assert!(
        builder
            .storage
            .borrow()
            .constraint_bound_depth_cache
            .is_empty()
    );
    assert!(builder.storage.borrow().never_satisfied_cache.is_empty());
    result
}

fn ordinary_produced_cache(selected: ProducedCache, count: usize) -> CacheValue {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let inputs = produced_cache_inputs(&db, &builder, selected, count);
    let env = db.program_environment();
    let mut result = None;
    for input in inputs {
        result = Some(match selected {
            ProducedCache::Depth => {
                let id = builder
                    .storage
                    .borrow()
                    .interior_node_data(input.node)
                    .constraint;
                CacheValue::Depth(
                    builder
                        .storage
                        .borrow_mut()
                        .cached_constraint_bound_depth(&db, &env, id),
                )
            }
            ProducedCache::Never => CacheValue::Never(input.is_never_satisfied(&db, &env)),
        });
    }
    result.expect("the independent ordinary fixture includes a target")
}

struct CacheChildCleanup<'a>(&'a dyn Fn());
impl Drop for CacheChildCleanup<'_> {
    fn drop(&mut self) {
        (self.0)();
    }
}

struct CacheRootOwner<'a> {
    live: &'a Cell<bool>,
    journal: &'a RefCell<Vec<&'static str>>,
}
impl Drop for CacheRootOwner<'_> {
    fn drop(&mut self) {
        self.live.set(false);
        self.journal.borrow_mut().push("root");
    }
}

struct CacheAdmission<'run, 'db: 'run> {
    events: RefCell<Vec<ExecutionWork>>,
    refuse: Option<usize>,
    fired: Cell<bool>,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    cleanup: &'run dyn Fn(),
    child_started: &'run Cell<bool>,
}
impl ExecutionAdmission for CacheAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let index = self.events.borrow().len();
        self.events.borrow_mut().push(work);
        if self.refuse == Some(index) && !self.fired.replace(true) {
            let endpoint = self
                .endpoint
                .borrow()
                .as_ref()
                .cloned()
                .ok_or(RunError::Contract("cache fault has no active endpoint"))?;
            let cleanup = CacheChildCleanup(self.cleanup);
            let started = self.child_started;
            *self.pending.borrow_mut() = Some(endpoint.demand(move || {
                started.set(true);
                async move {
                    let _cleanup = cleanup;
                    Ok(())
                }
            })?);
            return Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ));
        }
        Ok(())
    }
}

struct CacheReset<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}
impl Drop for CacheReset<'_, '_, '_> {
    fn drop(&mut self) {
        let pending = self.pending.borrow_mut().take();
        let endpoint = self.endpoint.borrow_mut().take();
        drop(pending);
        drop(endpoint);
    }
}

struct CacheRun {
    result: Result<RunResult<Vec<CacheValue>>, Incomplete>,
    inner_error: Option<RunError>,
    completed: usize,
    events: Vec<ExecutionWork>,
    before_never_cache_insert: Option<usize>,
}

fn run_cache_producer<'db>(
    db: &'db TestDb,
    input: ConstraintSet<'db, '_>,
    selected: ProducedCache,
    repeats: usize,
    allowance: usize,
    refusal: Option<(usize, bool)>,
) -> CacheRun {
    let program = db.program_environment().program(db);
    let before = selected.state(input.builder);
    let key = selected.key(input);
    let live = Cell::new(false);
    let completed = Cell::new(0);
    let inner_error = Cell::new(None);
    let journal = RefCell::new(Vec::new());
    let child_started = Cell::new(false);
    let child_drops = Cell::new(0);
    let events = RefCell::new(Vec::new());
    let before_never_cache_insert = Cell::new(None);
    let cleanup_snapshot = RefCell::new(None);
    let cleanup = || {
        let storage_available = input.builder.storage.try_borrow_mut().is_ok();
        let state = storage_available.then(|| selected.state(input.builder));
        *cleanup_snapshot.borrow_mut() =
            Some((live.get(), completed.get(), storage_available, state));
        child_drops.set(child_drops.get() + 1);
        journal.borrow_mut().push("child");
    };
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, allowance, || {
            // The tokens precede every owner that can retain an endpoint during cleanup.
            let single_keys;
            let pair_keys;
            let queries;
            let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
            let pending = RefCell::new(None);
            let admission = CacheAdmission {
                events: RefCell::new(Vec::new()),
                refuse: refusal.map(|(index, _)| index),
                fired: Cell::new(false),
                endpoint: &endpoint_slot,
                pending: &pending,
                cleanup: &cleanup,
                child_started: &child_started,
            };
            let reset = CacheReset {
                endpoint: &endpoint_slot,
                pending: &pending,
            };
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
            let admission = &admission;
            let live = &live;
            let completed = &completed;
            let journal = &journal;
            let before_never_cache_insert = &before_never_cache_insert;
            let result = registry.seal()?.run(move |endpoint| {
                **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                async move {
                    live.set(true);
                    let _owner = CacheRootOwner { live, journal };
                    let observe = |observation: Observation<'_, 'db>| {
                        if let Observation::BeforeNeverCacheInsert(node) = observation
                            && node == input.node
                        {
                            assert!(
                                before_never_cache_insert
                                    .replace(Some(admission.events.borrow().len()))
                                    .is_none()
                            );
                        }
                    };
                    let mut values = Vec::new();
                    for _ in 0..repeats {
                        let value = match selected {
                            ProducedCache::Depth => {
                                let id = input
                                    .builder
                                    .storage
                                    .borrow()
                                    .interior_node_data(input.node)
                                    .constraint;
                                let actual = input.builder.storage.borrow().constraint_data(id);
                                assert!(matches!(
                                    actual,
                                    Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_)
                                ));
                                let mut effects = RuntimeSatisfaction {
                                    db,
                                    endpoint: &endpoint,
                                    program,
                                    builder: input.builder,
                                    fields: salsa::FieldReads::new(db),
                                    queries,
                                    observer: None,
                                };
                                CacheValue::Depth(
                                    PathEffects::constraint_depth(&mut effects, id).await?,
                                )
                            }
                            ProducedCache::Never => CacheValue::Never(
                                satisfy_observed(
                                    db,
                                    &endpoint,
                                    program,
                                    input,
                                    SatisfactionKind::Never,
                                    queries,
                                    Some(&observe),
                                )
                                .await?,
                            ),
                        };
                        completed.set(completed.get() + 1);
                        values.push(value);
                    }
                    Ok(values)
                }
            });
            inner_error.set(result.as_ref().err().copied());
            *events.borrow_mut() = admission.events.borrow().clone();
            assert_eq!(admission.fired.get(), refusal.is_some());
            drop(reset);
            result
        })
    })
    .expect("cache producer source capture");
    if !captured.reads.is_empty() {
        captured.check_root_reads().unwrap();
    }
    assert!(!live.get());
    assert!(!child_started.get());
    assert_eq!(child_drops.get(), usize::from(refusal.is_some()));
    if refusal.is_some() {
        assert_eq!(&*journal.borrow(), &["child", "root"]);
        let (owner_live, completed, storage_available, state) = cleanup_snapshot
            .into_inner()
            .expect("queued child records passive cleanup state");
        assert!(
            owner_live,
            "the producer owner survives queued-child cleanup"
        );
        assert_eq!(completed, 0);
        assert!(storage_available);
        let state = state.expect("available storage records the cleanup state");
        assert_eq!(state.entries, before.entries);
        assert!(!state.entries.iter().any(|(id, _)| *id == key));
        if refusal.is_some_and(|(_, reserved)| reserved) {
            assert!(state.capacity > before.capacity);
        } else {
            assert_eq!(state.capacity, before.capacity);
        }
    }
    assert_typevar_builder_support(db, input.builder);
    CacheRun {
        result: captured.value.0,
        inner_error: inner_error.get(),
        completed: completed.get(),
        events: events.into_inner(),
        before_never_cache_insert: before_never_cache_insert.get(),
    }
}

#[test]
fn real_typevar_cache_producers_admit_growth_and_retry_every_publication_boundary() {
    for selected in [ProducedCache::Depth, ProducedCache::Never] {
        for prefix in [0, 7, 14] {
            let db = setup_db();
            let builder = ConstraintSetBuilder::new();
            let inputs = produced_cache_inputs(&db, &builder, selected, prefix + 1);
            for input in &inputs[..prefix] {
                assert_eq!(
                    run_cache_producer(&db, *input, selected, 1, usize::MAX, None).result,
                    Ok(Ok(vec![selected.expected()]))
                );
            }
            let target = inputs[prefix];
            let before = selected.state(&builder);
            assert_eq!(before.entries.len(), prefix);
            assert_eq!(before.entries.len(), before.capacity);
            assert!(
                !before
                    .entries
                    .iter()
                    .any(|(id, _)| *id == selected.key(target))
            );
            let plan = selected.growth(&before);
            let baseline = run_cache_producer(&db, target, selected, 1, usize::MAX, None);
            let ordinary = ordinary_produced_cache(selected, prefix + 1);
            assert_eq!(ordinary, selected.expected());
            assert_eq!(baseline.result, Ok(Ok(vec![ordinary])));
            assert_eq!(baseline.completed, 1);
            let resource = baseline
                .events
                .iter()
                .rposition(|event| {
                    *event
                        == ExecutionWork::Resource {
                            requested_bytes: plan.requested_payload_bytes,
                        }
                })
                .expect("actual selected cache reservation");
            if selected == ProducedCache::Never {
                assert_eq!(
                    resource,
                    baseline
                        .before_never_cache_insert
                        .expect("root cache insertion")
                        + 2
                );
            }
            assert_eq!(
                baseline.events[resource - 1],
                ExecutionWork::Work {
                    units: plan.relocation_units
                }
            );
            assert_eq!(
                baseline.events[resource + 1],
                ExecutionWork::Work { units: 1 }
            );
            assert_eq!(
                baseline.events[resource + 2],
                ExecutionWork::Work { units: 1 }
            );
            let after = selected.state(&builder);
            assert_eq!(after.entries.len(), prefix + 1);
            assert!(after.capacity >= plan.requested_capacity);
            for (refused, reserved) in [
                (resource - 1, false),
                (resource, false),
                (resource + 1, true),
                (resource + 2, true),
            ] {
                let db = setup_db();
                let builder = ConstraintSetBuilder::new();
                let inputs = produced_cache_inputs(&db, &builder, selected, prefix + 1);
                for input in &inputs[..prefix] {
                    assert_eq!(
                        run_cache_producer(&db, *input, selected, 1, usize::MAX, None).result,
                        Ok(Ok(vec![selected.expected()]))
                    );
                }
                let target = inputs[prefix];
                let before = selected.state(&builder);
                let failed = run_cache_producer(
                    &db,
                    target,
                    selected,
                    1,
                    usize::MAX,
                    Some((refused, reserved)),
                );
                assert_eq!(failed.result, Err(Incomplete::Allowance));
                assert_eq!(
                    failed.inner_error,
                    Some(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance
                    ))
                );
                assert_eq!(failed.completed, 0);
                assert_eq!(&failed.events[..=refused], &baseline.events[..=refused]);
                assert_eq!(selected.state(&builder).entries, before.entries);
                let retained_before_retry = selected.state(&builder);
                let retry = run_cache_producer(&db, target, selected, 1, usize::MAX, None);
                assert_eq!(retry.result, Ok(Ok(vec![selected.expected()])));
                assert_eq!(retry.completed, 1);
                let complete = selected.state(&builder);
                assert_eq!(complete.entries, after.entries);
                let retry_start = if selected == ProducedCache::Never {
                    retry
                        .before_never_cache_insert
                        .expect("root cache insertion")
                } else {
                    retry
                        .events
                        .iter()
                        .position(|event| *event == ExecutionWork::Poll)
                        .expect("root poll")
                        + 1
                };
                if reserved {
                    assert_eq!(complete.capacity, retained_before_retry.capacity);
                    assert!(
                        !retry.events[retry_start..]
                            .iter()
                            .any(|event| matches!(event, ExecutionWork::Resource { .. }))
                    );
                }
                let warm = run_cache_producer(&db, target, selected, 1, usize::MAX, None);
                assert_eq!(warm.result, Ok(Ok(vec![selected.expected()])));
                assert_eq!(selected.state(&builder), complete);
                assert_eq!(warm.before_never_cache_insert, None);
                let poll = warm
                    .events
                    .iter()
                    .position(|event| *event == ExecutionWork::Poll)
                    .expect("root poll");
                assert!(
                    !warm.events[poll + 1..]
                        .iter()
                        .any(|event| matches!(event, ExecutionWork::Resource { .. }))
                );
            }
        }
    }
}

#[test]
fn real_typevar_cache_hits_have_positive_width_independent_progress() {
    for selected in [ProducedCache::Depth, ProducedCache::Never] {
        let mut previous = None;
        for width in [1, 32] {
            let db = setup_db();
            let builder = ConstraintSetBuilder::new();
            let inputs = produced_cache_inputs(&db, &builder, selected, width);
            for input in &inputs {
                assert_eq!(
                    run_cache_producer(&db, *input, selected, 1, usize::MAX, None).result,
                    Ok(Ok(vec![selected.expected()]))
                );
            }
            let before = selected.state(&builder);
            let warm = run_cache_producer(&db, inputs[0], selected, 1, usize::MAX, None);
            assert_eq!(warm.result, Ok(Ok(vec![selected.expected()])));
            let poll = warm
                .events
                .iter()
                .position(|event| *event == ExecutionWork::Poll)
                .expect("root poll");
            let work: Vec<_> = warm.events[poll + 1..]
                .iter()
                .filter_map(|event| match event {
                    ExecutionWork::Work { units } => Some(*units),
                    _ => None,
                })
                .collect();
            assert!(!work.is_empty() && work.iter().all(|units| *units > 0));
            assert!(
                !warm.events[poll + 1..]
                    .iter()
                    .any(|event| matches!(event, ExecutionWork::Resource { .. }))
            );
            if let Some(previous) = &previous {
                assert_eq!(&work, previous);
            } else {
                previous = Some(work.clone());
            }
            let limited = run_cache_producer(
                &db,
                inputs[0],
                selected,
                8,
                3 * work.iter().sum::<usize>(),
                None,
            );
            assert_eq!(limited.result, Err(Incomplete::Allowance));
            assert_eq!(
                limited.inner_error,
                Some(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance
                ))
            );
            assert_eq!(limited.completed, 3);
            assert_eq!(selected.state(&builder), before);
            let retry = run_cache_producer(&db, inputs[0], selected, 1, usize::MAX, None);
            assert_eq!(retry.result, Ok(Ok(vec![selected.expected()])));
            assert_eq!(selected.state(&builder), before);
        }
    }
}

fn certify_sequent_reads(db: &TestDb, reads: &[Read]) {
    assert!(!reads.is_empty());
    let single = single_sequent_ingredient(db);
    let pair = pair_sequent_ingredient(db);
    for read in reads {
        assert_eq!(read.status, Status::Final);
        assert_eq!(read.parent, None);
        let certified = if single.database_key_index(read.key.key_index()) == read.key {
            FinalSourceMemo::certify(db as &dyn Db, single, read.key.key_index())
                .expect("completed single sequent remains a reusable final memo")
                .database_key()
        } else {
            assert_eq!(pair.database_key_index(read.key.key_index()), read.key);
            FinalSourceMemo::certify(db as &dyn Db, pair, read.key.key_index())
                .expect("completed pair sequent remains a reusable final memo")
                .database_key()
        };
        assert_eq!(certified, read.key);
    }
}

fn has_pair_implication(observations: &Observations<'_>) -> bool {
    observations.fetched.iter().any(|(request, map)| {
        matches!(request, Request::Pair(..))
            && map.sequents.iter().any(|group| {
                matches!(group, SequentGroup::Ungrouped(sequents)
                    if sequents.iter().any(|sequent| matches!(sequent, Sequent::PairImplication { .. })))
            })
    })
}

#[test]
fn allowance_refusal_preserves_completed_sequents_and_same_builder_retries() {
    let calibration_db = setup_db();
    let calibration_vars = variables(&calibration_db);
    let calibration_builder = ConstraintSetBuilder::new();
    let calibration_set = formula(
        &calibration_db,
        &calibration_builder,
        calibration_vars,
        Formula::ContradictoryChain,
        false,
    );
    assert_interior(calibration_set);
    assert!(
        calibration_builder
            .storage
            .borrow()
            .never_satisfied_cache
            .is_empty()
    );
    let calibration = execute(
        &calibration_db,
        calibration_db
            .program_environment()
            .program(&calibration_db),
        calibration_set,
        SatisfactionKind::Never,
        true,
    );
    assert!(calibration.result);
    assert!(has_pair_implication(&calibration.observations));
    let [(node, Some(remaining))] = calibration.observations.before_never_cache.as_slice() else {
        panic!("calibration must reach the root cache boundary once with an actual remainder");
    };
    assert_eq!(*node, calibration_set.node);
    let cutoff = usize::MAX.checked_sub(*remaining).unwrap();
    assert!(cutoff > 0);

    let db = setup_db();
    let vars = variables(&db);
    let program = db.program_environment().program(&db);
    let builder = ConstraintSetBuilder::new();
    let set = formula(&db, &builder, vars, Formula::ContradictoryChain, false);
    assert_interior(set);
    let original_root = builder.storage.borrow().interior_node_data(set.node);
    let original_source = set
        .source_order
        .map(|id| builder.storage.borrow().source_order_data(id));
    let assert_root_preserved = || {
        let storage = builder.storage.borrow();
        assert_eq!(storage.interior_node_data(set.node), original_root);
        assert_eq!(
            set.source_order.map(|id| storage.source_order_data(id)),
            original_source
        );
    };
    assert!(builder.storage.borrow().never_satisfied_cache.is_empty());
    let intermediates = [(vars[0], vars[2]), (vars[1], vars[3])].map(|(left, right)| {
        Constraint::from(TypeVarRangeBound::new(
            &db,
            ConstraintProvenance::Evidence,
            left,
            right,
        ))
    });
    assert!(intermediates.iter().all(|value| {
        !builder
            .storage
            .borrow()
            .constraint_cache
            .contains_key(value)
    }));
    let stamp = Stamp::current(&db);
    let limited = execute_with_allowance(&db, program, set, SatisfactionKind::Never, true, cutoff);
    assert_eq!(limited.result, Err(Incomplete::Allowance));
    assert!(!limited.delivered);
    assert_eq!(limited.exit_remaining, None);
    assert_eq!(
        limited.observations.before_never_cache,
        [(set.node, Some(0))],
        "the real cutoff reaches the original debit immediately before cache publication",
    );
    assert!(
        !builder
            .storage
            .borrow()
            .never_satisfied_cache
            .contains_key(&set.node)
    );
    assert!(stamp.belongs_to(&db));
    assert!(has_pair_implication(&limited.observations));
    assert!(intermediates.iter().any(|value| {
        builder
            .storage
            .borrow()
            .constraint_cache
            .get(value)
            .is_some_and(|id| limited.observations.imported.contains(&(*value, *id)))
    }));
    assert_eq!(
        normalized_reads(&db, vars, &limited.reads),
        normalized_reads(&calibration_db, calibration_vars, &calibration.reads),
    );
    assert_eq!(
        normalized_executions(&db, vars, &limited.events),
        normalized_executions(&calibration_db, calibration_vars, &calibration.events),
    );
    assert_eq!(
        limited
            .observations
            .imported
            .iter()
            .map(|(value, _)| normalize(&db, vars, *value))
            .collect::<Vec<_>>(),
        calibration
            .observations
            .imported
            .iter()
            .map(|(value, _)| normalize(&calibration_db, calibration_vars, *value))
            .collect::<Vec<_>>(),
    );
    certify_sequent_reads(&db, &limited.reads);
    assert_typevar_builder_support(&db, &builder);
    assert_root_preserved();
    assert_source_seed(&db, vars, &builder, set, &limited);

    let retry =
        execute_with_allowance(&db, program, set, SatisfactionKind::Never, true, usize::MAX);
    assert_eq!(retry.result, Ok(Ok(calibration.result)));
    assert!(retry.delivered);
    assert!(executed(&retry.events).is_empty());
    assert!(has_pair_implication(&retry.observations));
    assert_eq!(retry.observations.before_never_cache.len(), 1);
    assert_eq!(retry.observations.before_never_cache[0].0, set.node);
    assert_eq!(retry.observations.imported, limited.observations.imported);
    assert_eq!(
        retry
            .reads
            .iter()
            .map(|read| (read.key, read.memo_address))
            .collect::<Vec<_>>(),
        limited
            .reads
            .iter()
            .map(|read| (read.key, read.memo_address))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        retry.observations.fetched.len(),
        limited.observations.fetched.len()
    );
    for ((request, map), (previous_request, previous_map)) in retry
        .observations
        .fetched
        .iter()
        .zip(&limited.observations.fetched)
    {
        assert_eq!(request, previous_request);
        assert!(std::ptr::eq(*map, *previous_map));
        assert_eq!(*map, *previous_map);
    }
    certify_sequent_reads(&db, &retry.reads);
    assert_typevar_builder_support(&db, &builder);
    assert_root_preserved();
    assert_source_seed(&db, vars, &builder, set, &retry);
    assert_eq!(
        builder
            .storage
            .borrow()
            .never_satisfied_cache
            .get(&set.node),
        Some(&true)
    );
    assert!(stamp.belongs_to(&db));

    let hot = execute_with_allowance(&db, program, set, SatisfactionKind::Never, true, usize::MAX);
    assert_eq!(hot.result, Ok(Ok(calibration.result)));
    assert!(hot.delivered);
    assert!(hot.observations.before_never_cache.is_empty());
    assert!(hot.observations.fetched.is_empty());
    assert!(hot.observations.imported.is_empty());
    assert!(hot.observations.paths.is_empty());
    assert!(hot.reads.is_empty());
    assert!(executed(&hot.events).is_empty());
    let spent = hot
        .entry_remaining
        .unwrap()
        .checked_sub(hot.exit_remaining.unwrap())
        .unwrap();
    assert!(spent > 0, "the cache hit still debits real semantic work");
    assert_typevar_builder_support(&db, &builder);
    assert_root_preserved();
    assert_eq!(
        builder
            .storage
            .borrow()
            .never_satisfied_cache
            .get(&set.node),
        Some(&true)
    );
    assert!(stamp.belongs_to(&db));
    compare_ordinary(
        &calibration_db,
        calibration_vars,
        &calibration,
        Formula::ContradictoryChain,
        false,
        SatisfactionKind::Never,
    );
    eprintln!("real allowance cutoff={cutoff}; same-builder hot-hit debit={spent}");
}

fn assert_concrete_equivalence<'db>(
    set: ConstraintSet<'db, '_>,
    typevar: BoundTypeVarInstance<'db>,
    bound: Type<'db>,
) -> Constraint<'db> {
    assert_interior(set);
    let storage = set.builder.storage.borrow();
    let interior = storage.interior_node_data(set.node);
    let constraint = storage.constraint_data(interior.constraint);
    let Constraint::ConcreteEquivalence(equivalence) = constraint else {
        panic!("the original constructor must retain a concrete equivalence");
    };
    assert_eq!(equivalence.typevar, typevar);
    assert_eq!(equivalence.bound, bound);
    assert!(constraint.provides_lower() && constraint.provides_upper());
    constraint
}

#[test]
fn concrete_equivalence_searches_stored_typevar_and_fetches_cold_single_sequent() {
    let db = setup_db();
    let env = db.program_environment();
    let [t, u, ..] = variables(&db);
    let bound = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(u)));
    let builder = ConstraintSetBuilder::new();
    let set = ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, t, bound);
    let constraint = assert_concrete_equivalence(set, t, bound);
    assert_consistent(&db, &builder.storage.borrow());
    assert!(builder.storage.borrow().never_satisfied_cache.is_empty());
    let stamp = Stamp::current(&db);
    let record = execute(&db, env.program(&db), set, SatisfactionKind::Never, true);
    assert!(!record.result);
    assert_eq!(
        record.observations.bound_searches,
        [(bound, BoundSearch::TypeVar, true)]
    );
    let [(request, map)] = record.observations.fetched.as_slice() else {
        panic!("isolated equality must fetch its actual single-sequent map");
    };
    assert_eq!(*request, Request::Single(constraint));
    assert!(map.sequents.is_empty());
    let [read] = record.reads.as_slice() else {
        panic!("isolated equality must read exactly its single-sequent query");
    };
    assert_eq!(read_request(&db, read.key), Request::Single(constraint));
    assert_eq!(executed(&record.events), [read.key]);
    certify_sequent_reads(&db, &record.reads);
    assert_eq!(
        builder
            .storage
            .borrow()
            .never_satisfied_cache
            .get(&set.node),
        Some(&false)
    );
    assert_consistent(&db, &builder.storage.borrow());

    let retry = execute_with_allowance(
        &db,
        env.program(&db),
        set,
        SatisfactionKind::Never,
        true,
        usize::MAX,
    );
    assert_eq!(retry.result, Ok(Ok(false)));
    assert!(retry.delivered);
    assert!(retry.reads.is_empty());
    assert!(executed(&retry.events).is_empty());
    assert!(retry.observations.bound_searches.is_empty());
    assert!(retry.observations.fetched.is_empty());
    assert!(retry.observations.before_never_cache.is_empty());
    certify_sequent_reads(&db, &record.reads);
    assert_consistent(&db, &builder.storage.borrow());
    assert!(stamp.belongs_to(&db));

    let ordinary = setup_db();
    let env = ordinary.program_environment();
    let [t, u, ..] = variables(&ordinary);
    let bound = Type::TypeForm(TypeFormType::new(&ordinary, Type::TypeVar(u)));
    let builder = ConstraintSetBuilder::new();
    let set =
        ConstraintSet::constrain_typevar_equivalence_bound(&ordinary, &env, &builder, t, bound);
    let constraint = assert_concrete_equivalence(set, t, bound);
    let mut reader = ordinary.clone();
    reader.clear_salsa_events();
    let expected =
        prepared_source_probe::capture(&ordinary, || set.is_never_satisfied(&ordinary, &env))
            .unwrap();
    expected.check_root_reads().unwrap();
    assert_eq!(record.result, expected.value);
    let [read] = expected.reads.as_slice() else {
        panic!("ordinary isolated equality must read its single-sequent query");
    };
    assert_eq!(
        read_request(&ordinary, read.key),
        Request::Single(constraint)
    );
    assert_eq!(executed(&reader.take_salsa_events()), [read.key]);
    assert_consistent(&ordinary, &builder.storage.borrow());
    let spent = record.entry_remaining.unwrap() - record.exit_remaining.unwrap();
    let retry_spent = retry.entry_remaining.unwrap() - retry.exit_remaining.unwrap();
    eprintln!(
        "concrete equality debit={spent}; same-builder hot-hit debit={retry_spent}; admission sequence={:?}",
        record.work
    );
}

fn execute_search_with_allowance<'db>(
    db: &'db TestDb,
    bound: Type<'db>,
    search: BoundSearch,
    allowance: usize,
) -> Record<'db, Result<RunResult<bool>, Incomplete>> {
    let admission = Admission::default();
    let delivered = Cell::new(false);
    let entry_remaining = Cell::new(None);
    let exit_remaining = Cell::new(None);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, allowance, || {
            let registry = RegistryBuilder::new(db, &admission)?;
            let delivered = &delivered;
            let entry_remaining = &entry_remaining;
            let exit_remaining = &exit_remaining;
            registry.seal()?.run(move |endpoint| async move {
                entry_remaining.set(remaining_allowance_for_diagnostics(db));
                let result = type_search::search_bound(db, &endpoint, bound, search).await;
                if result.is_ok() {
                    delivered.set(true);
                    exit_remaining.set(remaining_allowance_for_diagnostics(db));
                }
                result
            })
        })
    })
    .unwrap();
    assert!(captured.reads.is_empty());
    Record {
        result: captured.value.0,
        observations: Observations::default(),
        events: reader.take_salsa_events(),
        reads: captured.reads,
        work: admission.0.into_inner(),
        delivered: delivered.get(),
        entry_remaining: entry_remaining.get(),
        exit_remaining: exit_remaining.get(),
    }
}

#[test]
fn unspecialized_bound_search_uses_shared_allowance_and_retries() {
    let db = setup_db();
    let env = db.program_environment();
    let inner = Type::TypeForm(TypeFormType::new(
        &db,
        Type::Dynamic(DynamicType::UnspecializedTypeVar),
    ));
    let bound = Type::TypeForm(TypeFormType::new(&db, inner));
    let mut ordinary = OrdinaryConstraintTypes { db: &db, env: &env };
    for (search, expected) in [
        (BoundSearch::TypeVar, false),
        (BoundSearch::UnspecializedTypeVar, true),
    ] {
        assert_eq!(ordinary.search_bound(bound, search), Ok(expected));
        let record = execute_search_with_allowance(&db, bound, search, usize::MAX);
        assert_eq!(record.result, Ok(Ok(expected)));
        assert!(record.delivered);
        assert!(executed(&record.events).is_empty());
    }

    let search = BoundSearch::UnspecializedTypeVar;
    let calibration = execute_search_with_allowance(&db, bound, search, usize::MAX);
    assert_eq!(calibration.result, Ok(Ok(true)));
    let Some(remaining) = calibration.exit_remaining else {
        panic!("completed search must record its actual remaining allowance");
    };
    let consumed = usize::MAX.checked_sub(remaining).unwrap();
    assert!(consumed > 1);
    // The remainder is captured before the root task returns. Removing one unit interrupts
    // the shared search itself, while its cursor and seen set still belong to that task.
    let cutoff = consumed - 1;
    let stamp = Stamp::current(&db);
    let limited = execute_search_with_allowance(&db, bound, search, cutoff);
    assert_eq!(limited.result, Err(Incomplete::Allowance));
    assert!(!limited.delivered);
    assert!(limited.entry_remaining.is_some());
    assert_eq!(limited.exit_remaining, None);
    assert!(executed(&limited.events).is_empty());
    let retry = execute_search_with_allowance(&db, bound, search, usize::MAX);
    assert_eq!(retry.result, Ok(Ok(true)));
    assert!(retry.delivered);
    assert!(executed(&retry.events).is_empty());
    assert!(stamp.belongs_to(&db));
    eprintln!(
        "stored bound search allowance cutoff={cutoff}; admission sequence={:?}",
        calibration.work
    );
}

#[test]
fn ground_equivalence_reaches_materialization_without_publishing_never() {
    let db = setup_db();
    let env = db.program_environment();
    let [t, ..] = variables(&db);
    let bound = Type::int_literal(1);
    let builder = ConstraintSetBuilder::new();
    let set = ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, t, bound);
    assert_concrete_equivalence(set, t, bound);
    let record = execute_with_allowance(
        &db,
        env.program(&db),
        set,
        SatisfactionKind::Never,
        true,
        usize::MAX,
    );
    assert_eq!(
        record.result,
        Err(Incomplete::UnsupportedSatisfactionOperation(
            UnsupportedSatisfactionOperation::BoundMaterialization,
        ))
    );
    assert!(!record.delivered);
    assert_eq!(
        record.observations.bound_searches,
        [
            (bound, BoundSearch::TypeVar, false),
            (bound, BoundSearch::UnspecializedTypeVar, false),
        ]
    );
    assert!(record.observations.before_never_cache.is_empty());
    assert!(record.reads.is_empty());
    assert!(executed(&record.events).is_empty());
    assert!(
        !builder
            .storage
            .borrow()
            .never_satisfied_cache
            .contains_key(&set.node)
    );
    assert_consistent(&db, &builder.storage.borrow());
}

#[test]
fn eager_newtype_bound_refuses_instance_conversion_before_query_execution() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/bound.py",
            "from typing import NewType\nclass Base: ...\nToken = NewType(\"Token\", Base)\n",
        )
        .build()?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/bound.py")?;
    let module = ProgramFile::new(&db, file, env.program(&db));
    let Type::KnownInstance(KnownInstanceType::NewType(declaration)) =
        global_symbol(&db, module, "Token").place.expect_type()
    else {
        anyhow::bail!("Token did not produce a NewType declaration");
    };
    let Some(base) = global_symbol(&db, module, "Base")
        .place
        .expect_type()
        .to_class_type(&db)
    else {
        anyhow::bail!("Base did not produce a class type");
    };
    let bound = Type::NewTypeInstance(NewType::new(
        &db,
        declaration.name(&db),
        declaration.definition(&db),
        Some(NewTypeBase::ClassType(base)),
    ));
    let [t, ..] = variables(&db);
    let builder = ConstraintSetBuilder::new();
    let set = ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, t, bound);
    assert_concrete_equivalence(set, t, bound);
    let record = execute_with_allowance(
        &db,
        env.program(&db),
        set,
        SatisfactionKind::Never,
        true,
        usize::MAX,
    );
    assert_eq!(
        record.result,
        Err(Incomplete::UnsupportedSearchOperation(
            SearchOperation::NewTypeInstance,
        ))
    );
    assert!(!record.delivered);
    assert!(record.observations.bound_searches.is_empty());
    assert!(record.observations.before_never_cache.is_empty());
    assert!(record.reads.is_empty());
    assert!(executed(&record.events).is_empty());
    assert!(
        !builder
            .storage
            .borrow()
            .never_satisfied_cache
            .contains_key(&set.node)
    );
    assert_consistent(&db, &builder.storage.borrow());
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum WalkOwnerOperation {
    Advance,
    Enqueue,
    Remember,
    Enter,
    Revisit,
    Leave,
}

#[derive(Debug, PartialEq, Eq)]
struct WalkOwnerSnapshot<'db> {
    pending: Vec<Type<'db>>,
    pending_len: usize,
    pending_capacity: usize,
    remembered: bool,
    active: bool,
    active_len: usize,
    active_capacity: usize,
}

struct WalkOwner<'a, 'db> {
    cursor: TypeWalkCursor<'db>,
    seen: TypeCollector<'db>,
    active: FxHashSet<Type<'db>>,
    target: Type<'db>,
    live: &'a Cell<bool>,
    journal: &'a RefCell<Vec<&'static str>>,
    snapshot: &'a RefCell<Option<WalkOwnerSnapshot<'db>>>,
}
impl Drop for WalkOwner<'_, '_> {
    fn drop(&mut self) {
        // Inspect the real collections while they still belong to the root's future. Assertions
        // run after destruction so a failed assertion cannot mask a cleanup failure.
        *self.snapshot.borrow_mut() = Some(WalkOwnerSnapshot {
            pending: self
                .cursor
                .pending
                .iter()
                .filter_map(|action| match action {
                    WalkAction::Visit(ty) => Some(*ty),
                    _ => None,
                })
                .collect(),
            pending_len: self.cursor.pending.len(),
            pending_capacity: self.cursor.pending.capacity(),
            remembered: unrestricted(
                self.seen
                    .type_was_already_seen_with(self.target, &mut UnrestrictedCollections),
            ),
            active: self.active.contains(&self.target),
            active_len: self.active.len(),
            active_capacity: self.active.capacity(),
        });
        self.live.set(false);
        self.journal.borrow_mut().push("walk");
    }
}

struct WalkOwnerRun<'db> {
    result: Result<RunResult<bool>, Incomplete>,
    work: Vec<ExecutionWork>,
    operation_range: std::ops::Range<usize>,
    snapshot: WalkOwnerSnapshot<'db>,
}

fn run_walk_owner<'db>(
    db: &'db TestDb,
    types: &[Type<'db>],
    operation: WalkOwnerOperation,
    refused: Option<usize>,
) -> WalkOwnerRun<'db> {
    let target = types[32];
    let live = Cell::new(false);
    let delivered = Cell::new(false);
    let journal = RefCell::new(Vec::new());
    let snapshot = RefCell::new(None);
    let work = RefCell::new(Vec::new());
    let operation_start = Cell::new(0);
    let operation_end = Cell::new(0);
    let child_started = Cell::new(false);
    let child_drops = Cell::new(0);
    let cleanup_observation = Cell::new(None);
    let cleanup = || {
        cleanup_observation.set(Some((
            live.get(),
            delivered.get(),
            snapshot.borrow().is_none(),
        )));
        child_drops.set(child_drops.get() + 1);
        journal.borrow_mut().push("child");
    };
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, usize::MAX, || {
            let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
            let pending = RefCell::new(None);
            let admission = CacheAdmission {
                events: RefCell::new(Vec::new()),
                refuse: refused,
                fired: Cell::new(false),
                endpoint: &endpoint_slot,
                pending: &pending,
                cleanup: &cleanup,
                child_started: &child_started,
            };
            let reset = CacheReset {
                endpoint: &endpoint_slot,
                pending: &pending,
            };
            let registry = RegistryBuilder::new(db, &admission)?;
            let admission = &admission;
            let live = &live;
            let delivered = &delivered;
            let journal = &journal;
            let snapshot = &snapshot;
            let operation_start = &operation_start;
            let operation_end = &operation_end;
            let result = registry.seal()?.run(move |endpoint| {
                **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                async move {
                    let cursor = TypeWalkCursor {
                        pending: types[..8].iter().copied().map(WalkAction::Visit).collect(),
                    };
                    let mut seen = TypeCollector::default();
                    for ty in &types[..8] {
                        assert!(!unrestricted(
                            seen.type_was_already_seen_with(*ty, &mut UnrestrictedCollections)
                        ));
                    }
                    let mut active = FxHashSet::default();
                    active.reserve(1);
                    let initial_capacity = active.capacity();
                    for ty in &types[..initial_capacity] {
                        active.insert(*ty);
                    }
                    if matches!(
                        operation,
                        WalkOwnerOperation::Revisit | WalkOwnerOperation::Leave
                    ) {
                        active.clear();
                        active.insert(target);
                    }
                    live.set(true);
                    let mut owner = WalkOwner {
                        cursor,
                        seen,
                        active,
                        target,
                        live,
                        journal,
                        snapshot,
                    };
                    let mut effects =
                        type_search::RuntimeTypeWalk::new(db, &endpoint, BoundSearch::TypeVar);
                    operation_start.set(admission.events.borrow().len());
                    let value = match operation {
                        WalkOwnerOperation::Advance => matches!(
                            effects.take_action(&mut owner.cursor).await?,
                            Some(WalkAction::Visit(_))
                        ),
                        WalkOwnerOperation::Enqueue => {
                            effects
                                .enqueue(&mut owner.cursor, WalkAction::Visit(target))
                                .await?;
                            true
                        }
                        WalkOwnerOperation::Remember => {
                            effects.remember_type(&mut owner.seen, target).await?
                        }
                        WalkOwnerOperation::Enter | WalkOwnerOperation::Revisit => {
                            effects.enter_active(&mut owner.active, target).await?
                        }
                        WalkOwnerOperation::Leave => {
                            effects.leave_active(&mut owner.active, target).await?;
                            true
                        }
                    };
                    operation_end.set(admission.events.borrow().len());
                    delivered.set(true);
                    Ok(value)
                }
            });
            *work.borrow_mut() = admission.events.borrow().clone();
            assert_eq!(admission.fired.get(), refused.is_some());
            drop(reset);
            result
        })
    })
    .expect("walk owner capture");
    assert!(captured.reads.is_empty());
    assert!(!live.get());
    assert!(!child_started.get());
    assert_eq!(child_drops.get(), usize::from(refused.is_some()));
    if refused.is_some() {
        assert_eq!(cleanup_observation.get(), Some((true, false, true)));
        assert!(!delivered.get());
        assert_eq!(&*journal.borrow(), &["child", "walk"]);
    } else {
        assert_eq!(cleanup_observation.get(), None);
        assert!(delivered.get());
        assert_eq!(&*journal.borrow(), &["walk"]);
    }
    WalkOwnerRun {
        result: captured.value.0,
        work: work.into_inner(),
        operation_range: operation_start.get()..operation_end.get(),
        snapshot: snapshot.into_inner().expect("the walk owner was destroyed"),
    }
}

/// Refusal at each admission preserves changes completed by earlier admitted phases and drains
/// pending endpoint children before destroying the walk owner. A pop commits after its work and
/// byte admissions; later refusal leaves exactly one fewer pending frame.
#[test]
fn endpoint_walk_owners_preserve_admitted_progress_and_drain_children() {
    let db = setup_db();
    let types: Vec<_> = (0..33)
        .map(|value| Type::TypeForm(TypeFormType::new(&db, Type::int_literal(value))))
        .collect();
    for operation in [
        WalkOwnerOperation::Advance,
        WalkOwnerOperation::Enqueue,
        WalkOwnerOperation::Remember,
        WalkOwnerOperation::Enter,
        WalkOwnerOperation::Revisit,
        WalkOwnerOperation::Leave,
    ] {
        let complete = run_walk_owner(&db, &types, operation, None);
        let expected = !matches!(
            operation,
            WalkOwnerOperation::Remember | WalkOwnerOperation::Revisit
        );
        assert_eq!(complete.result, Ok(Ok(expected)), "{operation:?}");
        assert!(!complete.operation_range.is_empty());
        if matches!(
            operation,
            WalkOwnerOperation::Revisit | WalkOwnerOperation::Leave
        ) {
            assert_eq!(complete.operation_range.len(), 1, "{operation:?}");
        }
        if matches!(operation, WalkOwnerOperation::Advance) {
            // Pop, frame quotation selection, and frame transfer each admit work and bytes.
            //
            let phases = &complete.work[complete.operation_range.clone()];
            assert_eq!(phases.len(), 6);
            for phase in phases.chunks_exact(2) {
                assert!(matches!(phase[0], ExecutionWork::Work { units } if units > 0));
                assert!(matches!(phase[1], ExecutionWork::Resource { requested_bytes } if requested_bytes > 0));
            }
            assert_eq!(complete.snapshot.pending, types[..7]);
            assert_eq!(complete.snapshot.pending_len, 7);
        }
        for index in complete.operation_range.clone() {
            let refused = run_walk_owner(&db, &types, operation, Some(index));
            assert_eq!(
                refused.result,
                Err(Incomplete::Allowance),
                "{operation:?}: {index}"
            );
            let remaining = if matches!(operation, WalkOwnerOperation::Advance)
                && index >= complete.operation_range.start + 2
            {
                7
            } else {
                8
            };
            assert_eq!(refused.snapshot.pending, types[..remaining], "{operation:?}: {index}");
            assert_eq!(refused.snapshot.pending_len, remaining, "{operation:?}: {index}");
            assert_eq!(refused.snapshot.pending_capacity, 8);
            assert!(!refused.snapshot.remembered);
            assert_eq!(
                refused.snapshot.active,
                matches!(
                    operation,
                    WalkOwnerOperation::Revisit | WalkOwnerOperation::Leave
                )
            );
            assert_eq!(refused.work[index], complete.work[index]);
        }
        let retry = run_walk_owner(&db, &types, operation, None);
        assert_eq!(retry.result, complete.result);
        assert_eq!(retry.snapshot, complete.snapshot);
        assert_eq!(retry.work, complete.work);
    }
}

/// Repeated stored visits pay identical positive work and byte phases. Empty completion still
/// admits the next-event wrapper, pop attempt, quote selection, and final frame transfer.
#[test]
fn endpoint_walk_progress_charges_repeated_items_and_empty_completion() {
    let db = setup_db();
    let ty = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(1)));
    let admission = Admission::default();
    let db = &db;
    let admission = &admission;
    let result = expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(|endpoint| async move {
                let mut effects =
                    type_search::RuntimeTypeWalk::new(db, &endpoint, BoundSearch::TypeVar);
                let mut cursor = TypeWalkCursor {
                    pending: [WalkAction::Visit(ty), WalkAction::Visit(ty)]
                        .into_iter()
                        .collect(),
                };
                let before = admission.0.borrow().len();
                let mut trace = Vec::new();
                while let Some(event) = effects
                    .next_event(
                        &mut cursor,
                        TypeWalkPolicy::search(TypeSearchMode::SkipLazyAttributes),
                    )
                    .await?
                {
                    match event {
                        TypeWalkEvent::Visit(ty) => trace.push(ty),
                        _ => panic!("only the stored visit frames are present"),
                    }
                }
                assert_eq!(trace, [ty, ty]);
                let admissions = admission.0.borrow();
                let phases = &admissions[before..];
                assert_eq!(phases.len(), 24);
                let (first_visit, remainder) = phases.split_at(8);
                let (second_visit, completion) = remainder.split_at(8);
                assert_eq!(first_visit, second_visit);
                assert_eq!(&first_visit[..6], &completion[..6]);
                for phase in phases.chunks_exact(2) {
                    assert!(matches!(phase[0], ExecutionWork::Work { units } if units > 0));
                    assert!(matches!(phase[1], ExecutionWork::Resource { requested_bytes } if requested_bytes > 0));
                }
                let total_work: usize = phases.iter().map(|admission| match admission {
                    ExecutionWork::Work { units } => *units,
                    _ => 0,
                }).sum();
                let total_bytes: usize = phases.iter().map(|admission| match admission {
                    ExecutionWork::Resource { requested_bytes } => *requested_bytes,
                    _ => 0,
                }).sum();
                assert!(total_work > trace.len());
                assert!(total_bytes > 0);
                assert!(cursor.pending.is_empty());
                Ok(())
            })
    });
    assert_eq!(result.0, Ok(Ok(())));
}

fn run_walk_depth<'db>(
    db: &'db TestDb,
    ty: Type<'db>,
    allowance: usize,
) -> Result<RunResult<(u16, u16)>, Incomplete> {
    let admission = Admission::default();
    expansion_probe::run(db, allowance, || {
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move { type_search::type_depth(db, &endpoint, ty).await })
    })
    .0
}

#[test]
fn endpoint_walk_depth_revisits_shared_children_and_preserves_nominal_refusals()
-> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/walk_depth.py",
            "class Plain: ...\nclass Box[T]: ...\n",
        )
        .build()?;
    let env = db.program_environment();
    let [variable, ..] = variables(&db);
    let shared = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(variable)));
    let deeper = Type::TypeForm(TypeFormType::new(&db, shared));
    let root = Type::Callable(CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(shared)]),
            deeper,
        ),
    ));
    let expected = max_constructor_and_typevar_depth(&db, &env, root);
    assert_eq!(expected, (3, 3));
    assert_eq!(run_walk_depth(&db, root, usize::MAX), Ok(Ok(expected)));
    assert_eq!(run_walk_depth(&db, root, 1), Err(Incomplete::Allowance));
    assert_eq!(run_walk_depth(&db, root, usize::MAX), Ok(Ok(expected)));
    assert_eq!(
        run_walk_depth(&db, Type::TypeVar(variable), usize::MAX),
        Ok(Ok((0, 0)))
    );
    let module = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/walk_depth.py")?,
        env.program(&db),
    );
    let plain = global_symbol(&db, module, "Plain")
        .place
        .expect_type()
        .to_class_type(&db)
        .expect("Plain is a class");
    let plain = Type::instance(&db, &env, plain);
    assert_eq!(run_walk_depth(&db, plain, usize::MAX), Ok(Ok((0, 0))));
    let tuple = Type::tuple(TupleType::heterogeneous(&db, &env, [shared]));
    for unsupported in [tuple, Type::object()] {
        assert_eq!(
            run_walk_depth(&db, unsupported, usize::MAX),
            Err(Incomplete::UnsupportedSatisfactionOperation(
                UnsupportedSatisfactionOperation::BoundDepth
            ))
        );
    }
    Ok(())
}

#[test]
fn endpoint_walk_semantic_checkpoints_remain_protected() {
    let db = setup_db();
    for operation in [
        SearchOperation::ProtocolInterface,
        SearchOperation::ProtocolMember,
        SearchOperation::TypeVarBounds,
        SearchOperation::TypeVarDefault,
        SearchOperation::AliasValue,
        SearchOperation::RecursiveUnfold,
        SearchOperation::TypedDictItems,
        SearchOperation::TypedDictOpenness,
        SearchOperation::NewTypeBase,
        SearchOperation::NewTypeInstance,
    ] {
        let admission = Admission::default();
        let delivered = Cell::new(false);
        let captured = prepared_source_probe::capture(&db, || {
            expansion_probe::run(&db, usize::MAX, || {
                let db = &db;
                let delivered = &delivered;
                RegistryBuilder::new(db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let mut effects =
                            type_search::RuntimeTypeWalk::new(db, &endpoint, BoundSearch::TypeVar);
                        effects
                            .checkpoint(TypeWalkWork::Search(SearchWork::Semantic(operation)))
                            .await?;
                        delivered.set(true);
                        Ok(())
                    })
            })
        })
        .expect("semantic refusal capture");
        assert_eq!(
            captured.value.0,
            Err(Incomplete::UnsupportedSearchOperation(operation))
        );
        assert!(!delivered.get());
        assert!(captured.reads.is_empty());
    }
}

#[derive(Debug)]
struct WalkNativePanic;

struct WalkPanicAdmission<'run, 'db: 'run>(CacheAdmission<'run, 'db>);
impl ExecutionAdmission for WalkPanicAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let result = self.0.admit(work);
        if result.is_err() {
            std::panic::panic_any(WalkNativePanic);
        }
        result
    }
}

#[test]
fn endpoint_walk_native_panic_retains_cursor_until_child_cleanup() {
    let db = setup_db();
    let types: Vec<_> = (0..33)
        .map(|value| Type::TypeForm(TypeFormType::new(&db, Type::int_literal(value))))
        .collect();
    let baseline = run_walk_owner(&db, &types, WalkOwnerOperation::Advance, None);
    let live = Cell::new(false);
    let journal = RefCell::new(Vec::new());
    let snapshot = RefCell::new(None);
    let child_started = Cell::new(false);
    let child_drops = Cell::new(0);
    let cleanup_observation = Cell::new(None);
    let cleanup = || {
        cleanup_observation.set(Some((live.get(), snapshot.borrow().is_none())));
        child_drops.set(child_drops.get() + 1);
        journal.borrow_mut().push("child");
    };
    let captured = prepared_source_probe::capture(&db, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            expansion_probe::run(&db, usize::MAX, || {
                let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
                let pending = RefCell::new(None);
                let admission = WalkPanicAdmission(CacheAdmission {
                    events: RefCell::new(Vec::new()),
                    refuse: Some(baseline.operation_range.start),
                    fired: Cell::new(false),
                    endpoint: &endpoint_slot,
                    pending: &pending,
                    cleanup: &cleanup,
                    child_started: &child_started,
                });
                let _reset = CacheReset {
                    endpoint: &endpoint_slot,
                    pending: &pending,
                };
                let registry = RegistryBuilder::new(&db, &admission)?;
                let admission = &admission;
                let db = &db;
                let types = &types;
                let live = &live;
                let journal = &journal;
                let snapshot = &snapshot;
                registry.seal()?.run(move |endpoint| {
                    **admission.0.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        live.set(true);
                        let mut owner = WalkOwner {
                            cursor: TypeWalkCursor {
                                pending: types[..8]
                                    .iter()
                                    .copied()
                                    .map(WalkAction::Visit)
                                    .collect(),
                            },
                            seen: TypeCollector::default(),
                            active: FxHashSet::default(),
                            target: types[32],
                            live,
                            journal,
                            snapshot,
                        };
                        let mut effects =
                            type_search::RuntimeTypeWalk::new(db, &endpoint, BoundSearch::TypeVar);
                        let _action = effects.take_action(&mut owner.cursor).await?;
                        Ok(())
                    }
                })
            })
        }))
    })
    .expect("native walk panic capture");
    let payload = captured
        .value
        .expect_err("the original native panic is resumed");
    assert!(payload.is::<WalkNativePanic>());
    assert!(captured.reads.is_empty());
    assert!(!child_started.get());
    assert_eq!(child_drops.get(), 1);
    assert_eq!(cleanup_observation.get(), Some((true, true)));
    assert!(!live.get());
    assert_eq!(&*journal.borrow(), &["child", "walk"]);
    let snapshot = snapshot
        .into_inner()
        .expect("the actual cursor owner was destroyed");
    assert_eq!(snapshot.pending, types[..8]);
    assert_eq!(snapshot.pending_len, 8);
    assert_eq!(snapshot.pending_capacity, 8);
}
