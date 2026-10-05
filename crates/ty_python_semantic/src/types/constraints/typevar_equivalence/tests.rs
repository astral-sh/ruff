use std::ops::ControlFlow;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::{PendingTypevarEquivalence, UnsupportedCompactedBuilder, check_id_length};
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork,
};
use crate::types::constraints::support::{Support, SupportId};
use crate::types::constraints::variables::{
    Constraint, ConstraintProvenance, TypeVarEquivalenceBound,
};
use crate::types::constraints::{
    ConstraintId, ConstraintSet, ConstraintSetBuilder, ConstraintSetStorage, NodeId, SourceOrder,
    SourceOrderId, TypeVarId,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::relation::execution::attempt::AttemptAdmission;
use crate::types::typevar::{
    BindingContext, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind,
    TypeVarNonce,
};
use crate::types::{BoundTypeVarInstance, Type, TypeVarVariance};

#[derive(Default)]
struct RecordingControl {
    events: Vec<TddWork>,
    refuse: Option<usize>,
}

impl TddControl for RecordingControl {
    type Error = usize;

    fn admit(&mut self, work: TddWork) -> Result<(), usize> {
        let index = self.events.len();
        self.events.push(work);
        if self.refuse == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}

pub(in crate::types::constraints) fn variable<'db>(
    db: &'db TestDb,
    name: &str,
    kind: TypeVarKind,
) -> BoundTypeVarInstance<'db> {
    let env = db.program_environment();
    let declaration = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new(name), None, kind),
        None,
        Some(TypeVarVariance::Invariant),
        None,
    );
    BoundTypeVarInstance::new(
        db,
        declaration,
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

fn rematerialized<'db>(
    db: &'db TestDb,
    variable: BoundTypeVarInstance<'db>,
) -> BoundTypeVarInstance<'db> {
    let declaration = TypeVarInstance::new(
        db,
        variable.typevar(db).identity(db),
        None,
        Some(TypeVarVariance::Invariant),
        Some(TypeVarDefaultEvaluation::Eager(Type::object())),
    );
    BoundTypeVarInstance::new(
        db,
        declaration,
        variable.binding_context(db),
        variable.paramspec_attr(db),
        variable.freshness(db),
    )
}

fn ordinary<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    requested: BoundTypeVarInstance<'db>,
    bound: BoundTypeVarInstance<'db>,
) -> ConstraintSet<'db, 'c> {
    ConstraintSet::constrain_typevar_equivalence_bound(
        db,
        &db.program_environment(),
        builder,
        requested,
        Type::TypeVar(bound),
    )
}

fn finish<'db, 'c>(
    cursor: &mut PendingTypevarEquivalence<'db, 'c>,
    control: &mut RecordingControl,
) -> Result<ConstraintSet<'db, 'c>, TddError<usize>> {
    for _ in 0..10_000 {
        let result = cursor.advance_with(control);
        let storage = cursor
            .builder
            .storage
            .try_borrow_mut()
            .expect("every advance releases storage");
        assert_consistent(cursor.db, &storage);
        drop(storage);
        if let ControlFlow::Break(set) = result? {
            return Ok(set);
        }
    }
    panic!("the fixed finite equivalence did not finish");
}

fn controlled<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    requested: BoundTypeVarInstance<'db>,
    bound: BoundTypeVarInstance<'db>,
    control: &mut RecordingControl,
) -> Result<ConstraintSet<'db, 'c>, TddError<usize>> {
    let mut cursor = PendingTypevarEquivalence::new(db, builder, requested, bound)
        .expect("an original builder supports fixed equivalence");
    finish(&mut cursor, control)
}

pub(in crate::types::constraints) fn assert_consistent<'db>(
    db: &'db dyn crate::Db,
    storage: &ConstraintSetStorage<'db>,
) {
    assert!(storage.supports.iter().all(Support::is_complete));
    assert_eq!(storage.typevars.len(), storage.typevar_cache.len());
    for (id, variable) in storage.typevars.iter_enumerated() {
        assert_eq!(storage.typevar_cache.get(&variable.identity(db)), Some(&id));
    }
    assert_eq!(storage.constraints.len(), storage.constraint_supports.len());
    assert_eq!(storage.constraints.len(), storage.constraint_cache.len());
    for (id, constraint) in storage.constraints.iter_enumerated() {
        assert_eq!(storage.constraint_cache.get(constraint), Some(&id));
        let support = &storage.supports[storage.constraint_supports[id]];
        assert!(support.iter().all(|id| id.index() < storage.typevars.len()));
    }
    assert_eq!(storage.nodes.len(), storage.node_supports.len());
    assert_eq!(storage.nodes.len(), storage.node_cache.len());
    for (id, node) in storage.nodes.iter_enumerated() {
        assert_eq!(storage.node_cache.get(node), Some(&id));
        assert!(node.constraint.index() < storage.constraints.len());
        assert!(storage.node_supports[id].index() < storage.supports.len());
    }
    assert_eq!(
        storage.source_orders.len(),
        storage.source_order_cache.len()
    );
    for (id, source) in storage.source_orders.iter_enumerated() {
        assert_eq!(storage.source_order_cache.get(source), Some(&id));
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::types::constraints) struct State<'db> {
    typevars: Vec<BoundTypeVarInstance<'db>>,
    constraints: Vec<Constraint<'db>>,
    supports: Vec<Vec<usize>>,
    constraint_supports: Vec<usize>,
    nodes: Vec<(usize, NodeId, NodeId, NodeId)>,
    node_supports: Vec<usize>,
    sources: Vec<SourceOrder>,
}

impl<'db> State<'db> {
    pub(in crate::types::constraints) fn capture(storage: &ConstraintSetStorage<'db>) -> Self {
        Self {
            typevars: storage.typevars.raw.clone(),
            constraints: storage.constraints.raw.clone(),
            supports: storage
                .supports
                .iter()
                .map(|support| support.words().to_vec())
                .collect(),
            constraint_supports: storage
                .constraint_supports
                .iter()
                .map(|id| id.index())
                .collect(),
            nodes: storage
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.constraint.index(),
                        node.if_true,
                        node.if_uncertain,
                        node.if_false,
                    )
                })
                .collect(),
            node_supports: storage.node_supports.iter().map(|id| id.index()).collect(),
            sources: storage.source_orders.raw.clone(),
        }
    }
}

// Terminal node IDs participate in exact graph identity but do not index the interior-node arena.
fn result_ids(set: ConstraintSet<'_, '_>) -> (NodeId, Option<SourceOrderId>) {
    (set.node, set.source_order)
}

#[test]
fn typed_pairs_match_ordinary_identity_support_and_source_order() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let t = variable(&db, "T", TypeVarKind::Pep695TypeVar);
    let u = variable(&db, "U", TypeVarKind::Pep695TypeVar);
    let p = variable(&db, "P", TypeVarKind::Pep695ParamSpec);
    let q = variable(&db, "Q", TypeVarKind::Pep695ParamSpec);
    let other_t = rematerialized(&db, t);
    assert_ne!(t, other_t);
    assert_eq!(t.identity(&db), other_t.identity(&db));
    let pairs = [
        (t, u),
        (u, t),
        (t, other_t),
        (other_t, t),
        (t, p),
        (p, t),
        (p, q),
        (q, p),
    ];
    for (requested, bound) in pairs {
        let ordinary_builder = ConstraintSetBuilder::new();
        let controlled_builder = ConstraintSetBuilder::new();
        let expected = ordinary(&db, &ordinary_builder, requested, bound);
        let actual = controlled(
            &db,
            &controlled_builder,
            requested,
            bound,
            &mut RecordingControl::default(),
        )?;
        assert_eq!(result_ids(actual), result_ids(expected));
        assert_eq!(
            State::capture(&controlled_builder.storage.borrow()),
            State::capture(&ordinary_builder.storage.borrow())
        );
        assert_eq!(
            controlled_builder.storage.borrow().typevars[TypeVarId::from_usize(0)],
            requested
        );
        let before = State::capture(&controlled_builder.storage.borrow());
        let warm = ordinary(&db, &controlled_builder, requested, bound);
        assert!(actual.ownership_probe_same_set(warm));
        let repeated = controlled(
            &db,
            &controlled_builder,
            requested,
            bound,
            &mut RecordingControl::default(),
        )?;
        assert!(actual.ownership_probe_same_set(repeated));
        assert_eq!(State::capture(&controlled_builder.storage.borrow()), before);
    }
    let builder = ConstraintSetBuilder::new();
    let first = controlled(&db, &builder, t, u, &mut RecordingControl::default())?;
    let reversed = controlled(&db, &builder, u, t, &mut RecordingControl::default())?;
    assert!(first.ownership_probe_same_set(reversed));
    assert_eq!(builder.storage.borrow().typevars.raw, [t, u]);
    let identical = controlled(&db, &builder, other_t, t, &mut RecordingControl::default())?;
    assert_eq!(identical.node, super::ALWAYS_TRUE);
    assert_eq!(
        builder.storage.borrow().typevars.raw,
        [t, u],
        "the first materialized occurrence is retained"
    );
    Ok(())
}

fn prefix<'db>(db: &'db TestDb, count: usize) -> Vec<BoundTypeVarInstance<'db>> {
    (0..count)
        .map(|index| variable(db, &format!("V{index}"), TypeVarKind::Pep695TypeVar))
        .collect()
}

fn populated<'db>(
    db: &'db TestDb,
    variables: &[BoundTypeVarInstance<'db>],
) -> ConstraintSetBuilder<'db> {
    let builder = ConstraintSetBuilder::new();
    for variable in variables {
        builder.storage.borrow_mut().intern_typevar(db, *variable);
    }
    builder
}

#[test]
fn identity_cache_growth_and_hits_preserve_complete_equivalence() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let variables = prefix(&db, 66);
    let builder = ConstraintSetBuilder::new();
    let mut typevar_growths = 0;
    let mut constraint_growths = 0;
    for index in 1..variables.len() {
        let mut trace = RecordingControl::default();
        let actual = controlled(&db, &builder, variables[0], variables[index], &mut trace)?;
        for work in &trace.events {
            if let TddWork::Grow {
                allocation: AllocationKind::Table(table),
                ..
            } = work
            {
                match table {
                    TableKind::Typevars => typevar_growths += 1,
                    TableKind::Constraints => constraint_growths += 1,
                    _ => {}
                }
            }
        }
        assert_consistent(&db, &builder.storage.borrow());
        let before = State::capture(&builder.storage.borrow());
        let mut warm = RecordingControl::default();
        let repeated = controlled(&db, &builder, variables[0], variables[index], &mut warm)?;
        assert!(repeated.ownership_probe_same_set(actual));
        assert_eq!(State::capture(&builder.storage.borrow()), before);
        assert!(
            !warm
                .events
                .iter()
                .any(|work| matches!(work, TddWork::Grow { .. }))
        );
        for table in [TableKind::Typevars, TableKind::Constraints] {
            assert!(warm.events.iter().any(
                |work| matches!(work, TddWork::HashAccess { table: actual, .. } if *actual == table)
            ));
        }
        assert!(
            warm.events
                .iter()
                .filter(|work| matches!(work, TddWork::HashAccess { .. }))
                .all(|work| work.work_units() == 1)
        );
        if [1, 64].contains(&index) {
            let units = warm
                .events
                .iter()
                .map(|work| work.work_units())
                .sum::<usize>();
            let mut completed = 0;
            let (limited, _) = expansion_probe::run(&db, 3 * units, || {
                let admission = AttemptAdmission { db: &db };
                let mut control = ExecutionControl::new(&admission);
                for _ in 0..8 {
                    let mut cursor = PendingTypevarEquivalence::new(
                        &db,
                        &builder,
                        variables[0],
                        variables[index],
                    )
                    .expect("original builder");
                    loop {
                        if let ControlFlow::Break(result) = cursor.advance_with(&mut control)? {
                            assert!(result.ownership_probe_same_set(actual));
                            completed += 1;
                            break;
                        }
                    }
                }
                Ok::<_, TddError<Incomplete>>(())
            });
            assert_eq!(limited, Err(Incomplete::Allowance));
            assert_eq!(completed, 3);
            assert_eq!(State::capture(&builder.storage.borrow()), before);
        }
    }
    assert!(typevar_growths >= 3 && constraint_growths >= 3);

    for prefix_len in [7, 8, 14, 15, 28, 29, 56, 57] {
        let make_builder = || -> Result<_, TddError<usize>> {
            let result = ConstraintSetBuilder::new();
            for index in 1..prefix_len {
                controlled(
                    &db,
                    &result,
                    variables[0],
                    variables[index],
                    &mut RecordingControl::default(),
                )?;
            }
            Ok(result)
        };
        let baseline = make_builder()?;
        let mut events = RecordingControl::default();
        let expected = controlled(
            &db,
            &baseline,
            variables[0],
            variables[prefix_len],
            &mut events,
        )?;
        let expected_ids = result_ids(expected);
        let expected_state = State::capture(&baseline.storage.borrow());
        assert!(events.events.iter().any(|work| matches!(
            work,
            TddWork::Grow {
                allocation: AllocationKind::Table(TableKind::Typevars | TableKind::Constraints),
                ..
            }
        )));
        for refusal in 0..events.events.len() {
            let partial = make_builder()?;
            let mut control = RecordingControl {
                refuse: Some(refusal),
                ..RecordingControl::default()
            };
            assert_eq!(
                controlled(
                    &db,
                    &partial,
                    variables[0],
                    variables[prefix_len],
                    &mut control
                )
                .map(result_ids),
                Err(TddError::Refused(refusal))
            );
            assert_consistent(&db, &partial.storage.borrow());
            let actual = controlled(
                &db,
                &partial,
                variables[0],
                variables[prefix_len],
                &mut RecordingControl::default(),
            )?;
            assert_eq!(result_ids(actual), expected_ids);
            assert_eq!(State::capture(&partial.storage.borrow()), expected_state);
        }
    }
    Ok(())
}

#[test]
fn each_admission_refuses_before_partial_publication_and_allows_retry()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let variables = prefix(&db, 2);
    let [requested, bound] = variables.as_slice() else {
        panic!("two fixture variables")
    };
    let baseline_builder = ConstraintSetBuilder::new();
    let mut baseline = RecordingControl::default();
    let expected = controlled(&db, &baseline_builder, *requested, *bound, &mut baseline)?;
    let expected_ids = result_ids(expected);
    let expected_state = State::capture(&baseline_builder.storage.borrow());
    for refuse in 0..baseline.events.len() {
        let builder = ConstraintSetBuilder::new();
        let mut control = RecordingControl {
            refuse: Some(refuse),
            ..RecordingControl::default()
        };
        let result = controlled(&db, &builder, *requested, *bound, &mut control);
        assert!(matches!(result, Err(TddError::Refused(index)) if index == refuse));
        assert_eq!(control.events, baseline.events[..=refuse]);
        assert_consistent(&db, &builder.storage.borrow());
        let actual = controlled(
            &db,
            &builder,
            *requested,
            *bound,
            &mut RecordingControl::default(),
        )?;
        assert_eq!(result_ids(actual), expected_ids);
        assert_eq!(State::capture(&builder.storage.borrow()), expected_state);
        assert!(actual.ownership_probe_same_set(ordinary(&db, &builder, *requested, *bound)));
    }
    for allocation in [
        AllocationKind::Typevars,
        AllocationKind::Constraints,
        AllocationKind::ConstraintSupports,
    ] {
        assert!(baseline.events.iter().any(
            |work| matches!(work, TddWork::Grow { allocation: actual, .. } if *actual == allocation)
        ));
    }
    Ok(())
}

#[test]
fn large_support_initialization_is_batched_and_refusable() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let variables = prefix(&db, 65 * usize::BITS as usize + 1);
    let requested = variables[variables.len() - 1];
    let bound = variables[variables.len() - 2];
    let builder = populated(&db, &variables);
    let ordinary_builder = populated(&db, &variables);
    let expected = ordinary(&db, &ordinary_builder, requested, bound);
    let mut baseline = RecordingControl::default();
    let actual = controlled(&db, &builder, requested, bound, &mut baseline)?;
    assert_eq!(result_ids(actual), result_ids(expected));
    assert_eq!(
        State::capture(&builder.storage.borrow()),
        State::capture(&ordinary_builder.storage.borrow())
    );
    let words = Support::words_needed(TypeVarId::from_usize(variables.len() - 1));
    assert!(words > 64);
    {
        let storage = builder.storage.borrow();
        let support = &storage.supports[SupportId::from_usize(0)];
        assert_eq!(support.words().len(), words);
        assert_eq!(
            support.iter().map(TypeVarId::index).collect::<Vec<_>>(),
            [variables.len() - 2, variables.len() - 1]
        );
    }
    assert!(
        baseline
            .events
            .contains(&TddWork::SupportWords { words: 64 })
    );
    assert!(
        baseline
            .events
            .iter()
            .all(|work| !matches!(work, TddWork::SupportWords { words } if *words > 64))
    );
    for (refuse, event) in baseline.events.iter().enumerate() {
        if !matches!(
            event,
            TddWork::SupportWords { .. }
                | TddWork::Grow {
                    allocation: AllocationKind::SupportWords,
                    ..
                }
        ) {
            continue;
        }
        let retry_builder = populated(&db, &variables);
        let mut control = RecordingControl {
            refuse: Some(refuse),
            ..RecordingControl::default()
        };
        assert!(
            matches!(controlled(&db, &retry_builder, requested, bound, &mut control), Err(TddError::Refused(index)) if index == refuse)
        );
        assert_consistent(&db, &retry_builder.storage.borrow());
        let actual = controlled(
            &db,
            &retry_builder,
            requested,
            bound,
            &mut RecordingControl::default(),
        )?;
        assert_eq!(result_ids(actual), result_ids(expected));
        assert_eq!(
            State::capture(&retry_builder.storage.borrow()),
            State::capture(&ordinary_builder.storage.borrow())
        );
    }
    let before = State::capture(&builder.storage.borrow());
    let mut hit = RecordingControl::default();
    controlled(&db, &builder, bound, requested, &mut hit)?;
    assert_eq!(State::capture(&builder.storage.borrow()), before);
    assert!(
        hit.events.iter().any(|work| matches!(
            work,
            TddWork::Grow {
                allocation: AllocationKind::SupportWords,
                ..
            }
        )),
        "a constraint-cache hit still builds temporary support"
    );
    assert!(hit.events.contains(&TddWork::SupportWords { words: 64 }));
    Ok(())
}

#[test]
fn ordinary_interning_between_advances_cannot_publish_duplicate_identities()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let t = variable(&db, "T", TypeVarKind::Pep695TypeVar);
    let u = variable(&db, "U", TypeVarKind::Pep695TypeVar);
    let v = variable(&db, "V", TypeVarKind::Pep695TypeVar);
    let baseline_builder = ConstraintSetBuilder::new();
    let mut baseline = PendingTypevarEquivalence::new(&db, &baseline_builder, t, u).unwrap();
    let mut boundaries = 0;
    while baseline
        .advance_with(&mut RecordingControl::default())?
        .is_continue()
    {
        boundaries += 1;
    }
    for boundary in 0..=boundaries {
        let builder = ConstraintSetBuilder::new();
        let mut cursor = PendingTypevarEquivalence::new(&db, &builder, t, u).unwrap();
        let mut control = RecordingControl::default();
        for _ in 0..boundary {
            assert!(cursor.advance_with(&mut control)?.is_continue());
            assert!(builder.storage.try_borrow_mut().is_ok());
        }
        ordinary(&db, &builder, v, u);
        let expected = ordinary(&db, &builder, t, u);
        let before = State::capture(&builder.storage.borrow());
        let actual = finish(&mut cursor, &mut control)?;
        assert!(actual.ownership_probe_same_set(expected));
        assert_eq!(State::capture(&builder.storage.borrow()), before);
        let end = control.events.len();
        control.refuse = Some(end);
        assert!(
            matches!(cursor.advance_with(&mut control), Err(TddError::Refused(index)) if index == end)
        );
        control.refuse = None;
        assert!(
            matches!(cursor.advance_with(&mut control)?, ControlFlow::Break(result) if result.ownership_probe_same_set(actual))
        );
    }
    Ok(())
}

#[test]
fn original_interning_keeps_native_collection_growth() {
    let db = setup_db();
    let env = db.program_environment();
    let variables = prefix(&db, 2 * usize::BITS as usize + 5);
    let mut storage = ConstraintSetStorage::default();
    let mut native_variables = Vec::new();
    let mut native_typevar_cache = FxHashMap::default();
    for variable in &variables {
        let id = storage.intern_typevar(&db, *variable);
        let expected_id = TypeVarId::from_usize(native_variables.len());
        native_variables.push(*variable);
        native_typevar_cache.insert(variable.identity(&db), expected_id);
        assert_eq!(id, expected_id);
        assert_eq!(storage.typevars.raw.capacity(), native_variables.capacity());
        assert_eq!(
            storage.typevar_cache.capacity(),
            native_typevar_cache.capacity()
        );
    }
    let mut native_constraints = Vec::new();
    let mut native_constraint_supports = Vec::new();
    let mut native_supports = Vec::new();
    let mut native_constraint_cache = FxHashMap::default();
    for requested in variables
        .iter()
        .skip(1)
        .copied()
        .chain(variables.iter().skip(1).copied())
    {
        let pair = TypeVarEquivalenceBound::new(
            &db,
            ConstraintProvenance::Evidence,
            requested,
            variables[0],
        );
        let data = Constraint::from(pair);
        let mut words: SmallVec<[usize; 2]> = SmallVec::new();
        for variable in [pair.left, pair.right] {
            let id = native_typevar_cache[&variable.identity(&db)].index();
            let extent = id / usize::BITS as usize + 1;
            if words.len() < extent {
                words.resize(extent, 0);
            }
            words[id / usize::BITS as usize] |= 1 << (id % usize::BITS as usize);
        }
        let mut support = storage.intern_constraint_typevars(&db, &env, data);
        assert_eq!(support.words(), words.as_slice());
        assert_eq!(support.words_mut().capacity(), words.capacity());
        let expected = if let Some(id) = native_constraint_cache.get(&data) {
            *id
        } else {
            let id = ConstraintId::from_usize(native_constraints.len());
            native_constraint_supports.push(SupportId::from_usize(native_supports.len()));
            native_supports.push(support);
            native_constraints.push(data);
            native_constraint_cache.insert(data, id);
            id
        };
        let actual = storage.intern_constraint(&db, &env, data);
        assert_eq!(actual, expected);
        assert_eq!(
            storage.constraints.raw.capacity(),
            native_constraints.capacity()
        );
        assert_eq!(
            storage.constraint_supports.raw.capacity(),
            native_constraint_supports.capacity()
        );
        assert_eq!(storage.supports.raw.capacity(), native_supports.capacity());
        assert_eq!(
            storage.constraint_cache.capacity(),
            native_constraint_cache.capacity()
        );
        assert_eq!(
            storage.supports.len(),
            native_supports.len(),
            "a warm constraint adds no support arena entry"
        );
    }
    assert!(
        storage
            .supports
            .iter()
            .any(|support| support.words().len() > 2)
    );
}

#[test]
fn compacted_start_is_distinct_from_refusal_and_does_not_initialize_caches() {
    let db = setup_db();
    let t = variable(&db, "T", TypeVarKind::Pep695TypeVar);
    let u = variable(&db, "U", TypeVarKind::Pep695TypeVar);
    let owned = ConstraintSetBuilder::new().into_owned(|builder| ordinary(&db, builder, t, u));
    owned.query(|builder, _| {
        let before = State::capture(&builder.storage.borrow());
        assert!(builder.storage.borrow().compacted.is_some());
        assert!(builder.storage.borrow().typevar_cache.is_empty());
        assert!(matches!(
            PendingTypevarEquivalence::new(&db, builder, t, u),
            Err(UnsupportedCompactedBuilder)
        ));
        assert_eq!(State::capture(&builder.storage.borrow()), before);
        assert!(builder.storage.borrow().typevar_cache.is_empty());
        assert!(builder.storage.borrow().constraint_cache.is_empty());
    });
}

#[test]
fn scalar_arena_limits_accept_the_last_valid_index() {
    for maximum in [TypeVarId::MAX_VALUE, ConstraintId::MAX_VALUE, u32::MAX - 1] {
        let maximum = maximum as usize;
        assert_eq!(maximum, u32::MAX as usize - 1);
        assert_eq!(check_id_length::<()>(maximum, maximum), Ok(()));
        assert_eq!(
            check_id_length::<()>(maximum + 1, maximum),
            Err(TddError::CapacityExhausted)
        );
        assert_eq!(
            check_id_length::<()>(usize::MAX, maximum),
            Err(TddError::CapacityExhausted)
        );
    }
    assert_eq!(SupportId::from_u32(u32::MAX - 1).as_u32(), u32::MAX - 1);
}
