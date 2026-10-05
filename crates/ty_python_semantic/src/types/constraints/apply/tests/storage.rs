use super::*;
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork, hash_access, hash_slots, map_growth,
    reserve_map, reserve_smallvec, reserve_vec,
};
use crate::types::constraints::storage::{OverlayIdentityState, next_node_ids};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::relation::execution::attempt::AttemptAdmission;
use smallvec::SmallVec;

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

fn finish_controlled(
    storage: &mut ConstraintSetStorage<'_>,
    operation: Operation,
    control: &mut RecordingControl,
) -> Result<NodeId, TddError<usize>> {
    let mut cursor = TddApply::new(operation);
    for _ in 0..100_000 {
        if let ControlFlow::Break(node) = cursor.advance_with(storage, control)? {
            return Ok(node);
        }
    }
    panic!("the finite controlled fixture did not complete");
}

fn completion_trace_fixtures<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
) -> Vec<(&'static str, ConstraintSetStorage<'db>, Operation)> {
    let mut storage = ConstraintSetStorage::default();
    let ids = constraints(db, env, &mut storage, 2);
    let left = NodeId::new(&mut storage, ids[0], ALWAYS_TRUE, ALWAYS_FALSE);
    let right = NodeId::new(&mut storage, ids[1], ALWAYS_TRUE, ALWAYS_FALSE);
    let operation = Operation::And(left, right);
    let new_node = ("new_node", fork_storage(&storage), operation);
    let mut warm_identity = fork_storage(&storage);
    operation.apply(&mut warm_identity);
    warm_identity.and_cache.clear();
    warm_identity.or_cache.clear();
    warm_identity.negate_cache.clear();
    let inverse = NodeId::new(&mut storage, ids[0], ALWAYS_FALSE, ALWAYS_TRUE);
    vec![
        new_node,
        ("warm_identity", warm_identity, operation),
        (
            "existing_inverse",
            fork_storage(&storage),
            Operation::Negate(left),
        ),
        ("terminal_reduction", storage, Operation::And(left, inverse)),
    ]
}

fn write_completion_admission_trace(mut output: impl Write) -> std::io::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    for (name, mut storage, operation) in completion_trace_fixtures(&db, &env) {
        let mut control = RecordingControl::default();
        let node = finish_controlled(&mut storage, operation, &mut control)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        writeln!(output, "TRACE {name} result={node:?}")?;
        for (index, work) in control.events.iter().enumerate() {
            writeln!(output, "{index:04} {work:?}")?;
        }
        writeln!(output, "END {name}")?;
    }
    Ok(())
}

#[test]
#[ignore]
fn completion_admission_trace_probe() -> std::io::Result<()> {
    write_completion_admission_trace(std::io::stdout().lock())
}

#[test]
fn ready_completion_preserves_the_frozen_admission_stream() -> std::io::Result<()> {
    let mut trace = Vec::new();
    write_completion_admission_trace(&mut trace)?;
    // Recorded from the preceding cursor on the pinned 64-bit collection backend. This
    // scratch fixture compares all admissions, including capacity and payload calculations.
    assert_eq!(
        trace.as_slice(),
        include_bytes!("completion-admission-trace.txt").as_slice()
    );
    Ok(())
}

fn controlled_publications(
    storage: &mut ConstraintSetStorage<'_>,
    operation: Operation,
) -> Result<(NodeId, Vec<StructuralState>), TddError<usize>> {
    let mut cursor = TddApply::new(operation);
    let mut control = RecordingControl::default();
    let mut publications = Vec::new();
    let mut grouped = false;
    for _ in 0..100_000 {
        let before = StructuralState::capture(storage);
        let start = control.events.len();
        let had_pending = cursor.pending.is_some();
        let progress = cursor.advance_with(storage, &mut control)?;
        let events = &control.events[start..];
        let advances = events
            .iter()
            .filter(|work| matches!(work, TddWork::Advance))
            .count();
        assert!(advances <= 8, "at most four paired engine/node phases");
        grouped |= advances > 2;
        if events.iter().any(|work| {
            matches!(
                work,
                TddWork::SupportWords { .. } | TddWork::OverlayScan { .. }
            )
        }) {
            assert!(advances <= 2, "storage batches remain separate transitions");
        }
        let commits = events
            .iter()
            .filter(|work| matches!(work, TddWork::Commit))
            .count();
        assert!(commits <= 1, "one advance publishes at most one operation");
        let after = StructuralState::capture(storage);
        if commits == 1 {
            publications.push(after);
        } else {
            assert_eq!(after, before);
        }
        if !had_pending && cursor.pending.is_some() {
            assert!(
                !events.contains(&TddWork::CoverageReduction),
                "creating a pending completion remains a separate transition"
            );
        }
        if let ControlFlow::Break(node) = progress {
            assert!(grouped, "the fixture exercises ready-phase grouping");
            return Ok((node, publications));
        }
    }
    panic!("the finite publication fixture did not complete");
}

#[test]
fn grouped_completion_preserves_each_publication() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut fixtures = completion_trace_fixtures(&db, &env);
    let mut mixed = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut mixed, 3);
    for operation in mixed_operations(&mut mixed, &ids).into_iter().take(6) {
        fixtures.push(("mixed", fork_storage(&mixed), operation));
    }
    for (name, mut storage, operation) in fixtures {
        let mut reference = RecursiveReference::new(&storage);
        let expected = reference.apply(operation);
        let (actual, publications) = controlled_publications(&mut storage, operation)?;
        assert_eq!(actual, expected, "{name}");
        assert_eq!(publications, reference.publications, "{name}");
        assert_eq!(
            StructuralState::capture(&storage),
            StructuralState::capture(&reference.storage),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn every_admission_can_refuse_without_unfinished_publication() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut initial = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut initial, 3);
    let operations = mixed_operations(&mut initial, &ids);
    let initial_state = StructuralState::capture(&initial);
    for operation in operations.into_iter().take(6) {
        let mut reference = RecursiveReference::new(&initial);
        let expected = reference.apply(operation);
        let final_state = StructuralState::capture(&reference.storage);
        let mut complete = fork_storage(&initial);
        let mut trace = RecordingControl::default();
        assert_eq!(
            finish_controlled(&mut complete, operation, &mut trace)?,
            expected
        );
        assert_eq!(StructuralState::capture(&complete), final_state);
        assert!(
            trace
                .events
                .iter()
                .any(|work| matches!(work, TddWork::Commit))
        );
        for refusal in 0..trace.events.len() {
            let mut storage = fork_storage(&initial);
            let mut control = RecordingControl {
                refuse: Some(refusal),
                ..RecordingControl::default()
            };
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut control),
                Err(TddError::Refused(refusal))
            );
            let partial = StructuralState::capture(&storage);
            assert!(partial == initial_state || reference.publications.contains(&partial));
            assert_eq!(control.events.len(), refusal + 1);
            let mut retry = RecordingControl::default();
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut retry)?,
                expected
            );
            assert_eq!(StructuralState::capture(&storage), final_state);
        }
    }
    Ok(())
}

#[test]
fn warm_cache_lookup_is_admitted_before_use() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut storage = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut storage, 3);
    let operations = mixed_operations(&mut storage, &ids);
    let operation = operations[0];
    let expected = operation.apply(&mut storage);
    let before = StructuralState::capture(&storage);
    let mut control = RecordingControl {
        refuse: Some(1),
        ..RecordingControl::default()
    };
    assert_eq!(
        finish_controlled(&mut storage, operation, &mut control),
        Err(TddError::Refused(1))
    );
    assert!(matches!(
        control.events[1],
        TddWork::HashAccess {
            table: TableKind::And,
            ..
        }
    ));
    assert_eq!(StructuralState::capture(&storage), before);
    let mut retry = RecordingControl::default();
    assert_eq!(
        finish_controlled(&mut storage, operation, &mut retry)?,
        expected
    );
    assert!(
        !retry
            .events
            .iter()
            .any(|work| matches!(work, TddWork::Grow { .. }))
    );
    Ok(())
}

#[test]
fn growing_operator_caches_preserve_publications_and_warm_access() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut storage = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut storage, 72);
    let operations: Vec<_> = ids
        .chunks_exact(3)
        .flat_map(|ids| mixed_operations(&mut storage, ids).into_iter().take(6))
        .collect();
    let mut table_growths = [0usize; 4];
    for (operation_index, operation) in operations.into_iter().enumerate() {
        let initial = fork_storage(&storage);
        let initial_state = StructuralState::capture(&initial);
        let mut reference = RecursiveReference::new(&initial);
        let expected = reference.apply(operation);
        let mut trace = RecordingControl::default();
        assert_eq!(
            finish_controlled(&mut storage, operation, &mut trace)?,
            expected
        );
        let expected_state = StructuralState::capture(&reference.storage);
        assert_eq!(StructuralState::capture(&storage), expected_state);
        let mut later_growth = false;
        for work in &trace.events {
            if let TddWork::Grow {
                allocation: AllocationKind::Table(table),
                ..
            } = work
            {
                let slot = match table {
                    TableKind::And => 0,
                    TableKind::Or => 1,
                    TableKind::Negate => 2,
                    TableKind::Nodes => 3,
                    _ => continue,
                };
                table_growths[slot] += 1;
                later_growth |= table_growths[slot] == 2 || (slot == 3 && table_growths[slot] == 1);
            }
        }
        if later_growth {
            for refusal in 0..trace.events.len() {
                let mut partial = fork_storage(&initial);
                let mut control = RecordingControl {
                    refuse: Some(refusal),
                    ..RecordingControl::default()
                };
                assert_eq!(
                    finish_controlled(&mut partial, operation, &mut control),
                    Err(TddError::Refused(refusal))
                );
                let state = StructuralState::capture(&partial);
                assert!(state == initial_state || reference.publications.contains(&state));
                assert_eq!(
                    finish_controlled(&mut partial, operation, &mut RecordingControl::default())?,
                    expected
                );
                assert_eq!(StructuralState::capture(&partial), expected_state);
            }
        }
        let before = StructuralState::capture(&storage);
        let mut warm = RecordingControl::default();
        assert_eq!(
            finish_controlled(&mut storage, operation, &mut warm)?,
            expected
        );
        assert_eq!(StructuralState::capture(&storage), before);
        assert!(
            !warm
                .events
                .iter()
                .any(|work| matches!(work, TddWork::Grow { .. }))
        );
        assert!(
            warm.events
                .iter()
                .any(|work| matches!(work, TddWork::HashAccess { .. }))
        );
        assert!(
            warm.events
                .iter()
                .filter(|work| matches!(work, TddWork::HashAccess { .. }))
                .all(|work| work.work_units() == 1)
        );
        if operation_index < 6 || operation_index >= 138 {
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
                    let mut cursor = TddApply::new(operation);
                    loop {
                        if let ControlFlow::Break(node) =
                            cursor.advance_with(&mut storage, &mut control)?
                        {
                            assert_eq!(node, expected);
                            completed += 1;
                            break;
                        }
                    }
                }
                Ok::<_, TddError<Incomplete>>(())
            });
            assert_eq!(limited, Err(Incomplete::Allowance));
            assert_eq!(completed, 3);
            assert_eq!(StructuralState::capture(&storage), before);
        }
    }
    assert!(
        table_growths[..3].iter().all(|count| *count >= 2) && table_growths[3] > 0,
        "{table_growths:?}"
    );
    Ok(())
}

#[test]
fn support_merging_admits_words_and_preserves_incompleteness() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut initial = ConstraintSetStorage::default();
    // Place the relevant typevar beyond 64 machine words so merging must suspend mid-support.
    for index in 0..=(usize::BITS as usize * 65) {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new(format!("T{index}")),
            TypeVarVariance::Invariant,
        );
        initial.intern_typevar(&db, variable);
    }
    let variable = *initial
        .typevars
        .raw
        .last()
        .ok_or(TddError::CapacityExhausted)?;
    let Some(Ok(constraint)) = Constraint::new_lower_bound(
        &db,
        ConstraintProvenance::Evidence,
        variable,
        Type::int_literal(1),
    )
    .next() else {
        panic!("the fixture has a concrete bound");
    };
    let constraint = initial.intern_constraint(&db, &env, constraint);
    let support_id = initial.constraint_support_id(constraint);
    initial.supports[support_id].mark_incomplete();
    let node = NodeId::new(&mut initial, constraint, ALWAYS_TRUE, ALWAYS_FALSE);
    let operation = Operation::Negate(node);
    let mut expected_storage = fork_storage(&initial);
    let expected = operation.apply(&mut expected_storage);
    let mut actual = fork_storage(&initial);
    let mut trace = RecordingControl::default();
    assert_eq!(
        finish_controlled(&mut actual, operation, &mut trace)?,
        expected
    );
    assert_eq!(
        StructuralState::capture(&actual),
        StructuralState::capture(&expected_storage)
    );
    assert!(
        actual
            .node_support(expected)
            .is_some_and(|support| !support.is_complete() && support.words().len() > 64)
    );
    let growth = trace.events.iter().position(|work| {
        matches!(
            work,
            TddWork::Grow {
                allocation: AllocationKind::SupportWords,
                ..
            }
        )
    });
    let first_word = trace
        .events
        .iter()
        .position(|work| matches!(work, TddWork::SupportWords { words } if *words > 0));
    assert!(matches!((growth, first_word), (Some(growth), Some(word)) if growth < word));
    for (index, work) in trace.events.iter().enumerate() {
        if matches!(
            work,
            TddWork::SupportWords { .. }
                | TddWork::Grow {
                    allocation: AllocationKind::SupportWords,
                    ..
                }
        ) {
            let mut storage = fork_storage(&initial);
            let mut refused = RecordingControl {
                refuse: Some(index),
                ..RecordingControl::default()
            };
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut refused),
                Err(TddError::Refused(index))
            );
            assert_eq!(storage.supports.len(), initial.supports.len());
            assert_eq!(storage.nodes.len(), initial.nodes.len());
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut RecordingControl::default())?,
                expected
            );
        }
        if let TddWork::SupportWords { words } = work {
            assert!(*words <= 64);
        }
    }
    Ok(())
}

fn sparse_owned<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>) -> OwnedConstraintSet<'db> {
    ConstraintSetBuilder::new().into_owned(|builder| {
        let mut storage = builder.storage.borrow_mut();
        let ids = constraints(db, env, &mut storage, 70);
        let leaves: Vec<_> = ids
            .iter()
            .map(|id| NodeId::new(&mut storage, *id, ALWAYS_TRUE, ALWAYS_FALSE))
            .collect();
        let root = NodeId::new(&mut storage, ids[69], leaves[0], ALWAYS_FALSE);
        let first = storage.constraint_source_order(ids[0]);
        let middle = storage.constraint_source_order(ids[35]);
        let last = storage.constraint_source_order(ids[69]);
        let source = storage.ordered_source_order(Some(first), Some(middle));
        let source = storage.ordered_source_order(source, Some(last));
        ConstraintSet::from_node(builder, root, source)
    })
}

fn overlay<'db>(owned: &OwnedConstraintSet<'db>) -> ConstraintSetStorage<'db> {
    ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    }
}

#[test]
fn partial_overlay_initialization_can_resume_through_every_identity_consumer()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let owned = sparse_owned(&db, &env);
    let initial = overlay(&owned);
    let original = initial.interior_node_data(owned.node);
    let original_constraint = initial.constraint_data(original.constraint);
    let operation = Operation::Negate(owned.node);
    let mut complete = overlay(&owned);
    let mut trace = RecordingControl::default();
    let expected = finish_controlled(&mut complete, operation, &mut trace)?;
    let final_state = StructuralState::capture(&complete);
    let mut partial_nodes_observed = false;
    for refusal in 0..trace.events.len() {
        let mut storage = overlay(&owned);
        let mut control = RecordingControl {
            refuse: Some(refusal),
            ..RecordingControl::default()
        };
        assert_eq!(
            finish_controlled(&mut storage, operation, &mut control),
            Err(TddError::Refused(refusal))
        );
        partial_nodes_observed |= !storage.node_cache.is_empty()
            && storage.overlay_identity_state != OverlayIdentityState::Ready;
        let mut cached_nodes: Vec<_> = storage.node_cache.iter().collect();
        cached_nodes.sort_unstable_by_key(|(_, node)| node.0);
        for (data, node) in cached_nodes {
            assert_eq!(storage.interior_node_data(*node), *data);
        }
        let mut ordinary = fork_storage(&storage);
        let old_node_count = ordinary.nodes.len();
        assert_eq!(ordinary.intern_interior_node(original), owned.node);
        assert_eq!(ordinary.nodes.len(), old_node_count);
        assert_eq!(ordinary.overlay_identity_state, OverlayIdentityState::Ready);

        let mut ordinary = fork_storage(&storage);
        assert_eq!(
            ordinary.intern_constraint(&db, &env, original_constraint),
            original.constraint
        );
        assert_eq!(ordinary.overlay_identity_state, OverlayIdentityState::Ready);

        let mut ordinary = fork_storage(&storage);
        if let Some(source) = owned.source_order {
            let data = ordinary.source_order_data(source);
            assert_eq!(ordinary.intern_source_order(data), source);
            assert_eq!(ordinary.overlay_identity_state, OverlayIdentityState::Ready);
        }
        let mut ordinary = fork_storage(&storage);
        let variable = ordinary.typevar_data(crate::types::constraints::TypeVarId::from_usize(0));
        assert_eq!(
            ordinary.intern_typevar(&db, variable),
            crate::types::constraints::TypeVarId::from_usize(0)
        );
        assert_eq!(ordinary.overlay_identity_state, OverlayIdentityState::Ready);

        assert_eq!(
            finish_controlled(&mut storage, operation, &mut RecordingControl::default())?,
            expected
        );
        assert_eq!(StructuralState::capture(&storage), final_state);
    }
    assert!(partial_nodes_observed);
    Ok(())
}

#[test]
fn frame_spill_can_refuse_before_allocating_or_publishing() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let env = db.program_environment();
    let mut initial = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut initial, 16);
    let mut root = ALWAYS_TRUE;
    for id in ids {
        root = NodeId::new(&mut initial, id, root, ALWAYS_FALSE);
    }
    let operation = Operation::Negate(root);
    let mut complete = fork_storage(&initial);
    let mut trace = RecordingControl::default();
    let expected = finish_controlled(&mut complete, operation, &mut trace)?;
    let mut spills = 0;
    for (index, work) in trace.events.iter().enumerate() {
        if matches!(
            work,
            TddWork::Grow {
                allocation: AllocationKind::Frames,
                ..
            }
        ) {
            spills += 1;
            let mut storage = fork_storage(&initial);
            let mut control = RecordingControl {
                refuse: Some(index),
                ..RecordingControl::default()
            };
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut control),
                Err(TddError::Refused(index))
            );
            assert_eq!(
                StructuralState::capture(&storage),
                StructuralState::capture(&initial)
            );
            assert_eq!(
                finish_controlled(&mut storage, operation, &mut RecordingControl::default())?,
                expected
            );
        }
    }
    assert!(spills > 0);
    Ok(())
}

thread_local! {
    static COLLISION_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[test]
fn map_growth_is_geometric_and_charges_only_actual_reservations() -> Result<(), TddError<usize>> {
    let mut table = FxHashMap::<u64, u64>::default();
    let mut control = RecordingControl::default();
    let mut bulk = 0;
    let mut growths = 0;
    for key in 0..256 {
        let before = (table.len(), table.capacity());
        let start = control.events.len();
        reserve_map(&mut table, TableKind::Nodes, &mut control)?;
        let events = &control.events[start..];
        if before.0 == before.1 {
            let [TddWork::Grow { plan, .. }, TddWork::HashAccess { .. }] = events else {
                panic!("a full map reserves before its insertion access: {events:?}");
            };
            assert_eq!(
                plan.requested_capacity,
                (2 * before.1).max(before.0 + 1).max(4)
            );
            assert_eq!(
                plan.requested_payload_bytes,
                plan.requested_capacity * size_of::<(u64, u64)>()
            );
            let old_extent = if before.1 == 0 {
                0
            } else {
                hash_slots::<usize>(before.1)?
            };
            assert_eq!(
                plan.relocation_units,
                old_extent + before.0 + hash_slots::<usize>(2 * plan.requested_capacity)?
            );
            assert!(table.capacity() >= plan.requested_capacity);
            assert!(table.capacity() <= 2 * plan.requested_capacity);
            bulk += plan.relocation_units;
            growths += 1;
        } else {
            assert!(matches!(events, [TddWork::HashAccess { .. }]));
            assert_eq!(table.capacity(), before.1);
        }
        assert_eq!(events.last().unwrap().work_units(), 1);
        assert!(table.insert(key, key + 1).is_none());
        for old in 0..=key {
            assert_eq!(table.get(&old), Some(&(old + 1)));
        }
    }
    assert!(growths >= 3);
    assert!(bulk <= 32 * table.capacity() + 72 * growths);
    // The last tuple isolates checked addition with a synthetic, unreachable map length.
    for (len, capacity, required) in [(0, usize::MAX, 1), (0, 0, usize::MAX), (usize::MAX, 1, 2)] {
        assert_eq!(
            map_growth::<u64, u64, usize>(len, capacity, required),
            Err(TddError::CapacityExhausted)
        );
    }
    Ok(())
}

#[test]
fn map_reservation_and_commit_refusals_retain_only_completed_entries() -> Result<(), TddError<usize>>
{
    for requested in [0, 4, 32] {
        for refusal in 0..3 {
            let mut table =
                FxHashMap::<usize, usize>::with_capacity_and_hasher(requested, Default::default());
            while table.len() < table.capacity() {
                let key = table.len();
                table.insert(key, key + 1);
            }
            let before = (table.len(), table.capacity());
            let mut control = RecordingControl {
                refuse: Some(refusal),
                ..RecordingControl::default()
            };
            let result = reserve_map(&mut table, TableKind::Nodes, &mut control).and_then(|()| {
                control.admit(TddWork::Commit)?;
                table.insert(before.0, before.0 + 1);
                Ok(())
            });
            assert_eq!(result, Err(TddError::Refused(refusal)));
            assert_eq!(table.len(), before.0);
            assert!(!table.contains_key(&before.0));
            for key in 0..before.0 {
                assert_eq!(table.get(&key), Some(&(key + 1)));
            }
            if refusal == 0 {
                assert_eq!(table.capacity(), before.1);
            } else {
                assert!(table.capacity() > before.1);
            }
            let mut retry = RecordingControl::default();
            reserve_map(&mut table, TableKind::Nodes, &mut retry)?;
            retry.admit(TddWork::Commit)?;
            table.insert(before.0, before.0 + 1);
            assert_eq!(table.len(), before.0 + 1);
            assert_eq!(table.get(&before.0), Some(&(before.0 + 1)));
            assert_eq!(
                retry
                    .events
                    .iter()
                    .filter(|work| matches!(work, TddWork::Grow { .. }))
                    .count(),
                usize::from(refusal == 0)
            );
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq)]
struct Collision(usize);

impl PartialEq for Collision {
    fn eq(&self, other: &Self) -> bool {
        COLLISION_COMPARISONS.set(COLLISION_COMPARISONS.get() + 1);
        self.0 == other.0
    }
}

impl std::hash::Hash for Collision {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u8(0);
    }
}

#[test]
fn controlled_hash_access_preserves_its_event_and_refuses_before_lookup() {
    let mut table = FxHashMap::default();
    table.insert(Collision(0), 7);
    table.insert(Collision(1), 9);
    let expected = TddWork::HashAccess {
        table: TableKind::Nodes,
        slots: 4 * (table.capacity() + 1) + 32,
    };
    let mut refused = RecordingControl {
        refuse: Some(0),
        ..RecordingControl::default()
    };
    COLLISION_COMPARISONS.set(0);
    let result = refused
        .admit_hash_access(TableKind::Nodes, table.capacity())
        .map(|()| table.get(&Collision(0)).copied());
    assert_eq!(result, Err(TddError::Refused(0)));
    assert_eq!(refused.events, [expected]);
    assert_eq!(COLLISION_COMPARISONS.get(), 0);

    let mut admitted = RecordingControl::default();
    let result = hash_access(&mut admitted, TableKind::Nodes, table.capacity())
        .map(|()| table.get(&Collision(0)).copied());
    assert_eq!(result, Ok(Some(7)));
    assert_eq!(admitted.events, [expected]);
    assert_eq!(expected.work_units(), 1);
    assert!(COLLISION_COMPARISONS.get() > 0);
}

#[test]
fn growth_accounting_handles_sparse_buffers_collisions_and_overflow() -> Result<(), TddError<usize>>
{
    let mut vector = Vec::<u64>::with_capacity(32);
    vector.push(1);
    let old_capacity = vector.capacity();
    let mut control = RecordingControl::default();
    reserve_vec(
        &mut vector,
        old_capacity + 1,
        AllocationKind::Nodes,
        &mut control,
    )?;
    assert!(
        matches!(control.events[0], TddWork::Grow { plan, .. } if plan.relocation_units >= old_capacity)
    );
    let mut small = SmallVec::<[u64; 2]>::from_vec(Vec::with_capacity(32));
    small.push(1);
    let old_capacity = small.capacity();
    let mut control = RecordingControl::default();
    reserve_smallvec(
        &mut small,
        old_capacity + 1,
        AllocationKind::Frames,
        &mut control,
    )?;
    assert!(
        matches!(control.events[0], TddWork::Grow { plan, .. } if plan.relocation_units >= old_capacity)
    );

    let mut table = FxHashMap::default();
    let mut control = RecordingControl::default();
    for index in 0..32 {
        let start = control.events.len();
        COLLISION_COMPARISONS.set(0);
        reserve_map(&mut table, TableKind::Nodes, &mut control)?;
        table.insert(Collision(index), index);
        assert!(
            control.events[start..]
                .iter()
                .all(|work| work.work_units() > 0)
        );
        if index > 0 {
            assert!(COLLISION_COMPARISONS.get() > 0);
        }
    }
    for index in 0..=32 {
        let start = control.events.len();
        COLLISION_COMPARISONS.set(0);
        hash_access(&mut control, TableKind::Nodes, table.capacity())?;
        assert_eq!(table.get(&Collision(index)), (index < 32).then_some(&index));
        assert_eq!(control.events[start].work_units(), 1);
        assert!(COLLISION_COMPARISONS.get() > 0);
    }
    assert_eq!(COLLISION_COMPARISONS.get(), 32);
    // One logical miss can compare every retained key under this deliberately colliding hash.
    assert!(COLLISION_COMPARISONS.get() > control.events.last().unwrap().work_units());
    assert!(control.events.iter().any(|work| matches!(
        work,
        TddWork::Grow {
            allocation: AllocationKind::Table(_),
            ..
        }
    )));

    let mut control = RecordingControl::default();
    assert_eq!(
        hash_access(&mut control, TableKind::Nodes, usize::MAX),
        Err(TddError::CapacityExhausted)
    );
    assert_eq!(
        reserve_vec(
            &mut Vec::<u64>::new(),
            isize::MAX as usize,
            AllocationKind::Nodes,
            &mut control
        ),
        Err(TddError::CapacityExhausted)
    );
    assert!(control.events.is_empty());
    assert_eq!(
        next_node_ids::<usize>(1, 2, 3, 4),
        Ok((NodeId::from_usize(3), SupportId::from_usize(7)))
    );
    for arguments in [
        (usize::MAX, 1, 0, 0),
        (
            0,
            crate::types::constraints::SMALLEST_TERMINAL.0 as usize,
            0,
            0,
        ),
        (0, 0, usize::MAX, 1),
        (0, 0, u32::MAX as usize, 0),
    ] {
        assert_eq!(
            next_node_ids::<usize>(arguments.0, arguments.1, arguments.2, arguments.3),
            Err(TddError::CapacityExhausted)
        );
    }
    Ok(())
}
