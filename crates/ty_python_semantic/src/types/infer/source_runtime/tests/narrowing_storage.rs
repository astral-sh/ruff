use std::cell::RefCell;

use ruff_index::{Idx, IndexSlice};
use rustc_hash::FxHashMap;
use ty_python_core::PredicateNarrowingTargets;
use ty_python_core::narrowing_constraints::NarrowingConstraints;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::ScopedPredicateId;
use ty_python_core::symbol::ScopedSymbolId;

use super::*;
use crate::reachability::narrowing_construction::{
    Construction, Frame, NarrowingConstructionEffects,
};
use crate::reachability::narrowing_entry::{
    NarrowingEntryEffects, OrdinaryNarrowingEntryEffects, SynchronousNarrowingEntryEffects,
};
use crate::reachability::{
    NarrowingProjector, ProjectedNarrowingEntry, ProjectedNarrowingNode, ProjectedNarrowingNodeId,
};
use crate::types::todo_type;

const TRUE: ProjectedNarrowingNodeId = ProjectedNarrowingNodeId::ALWAYS_TRUE;
const FALSE: ProjectedNarrowingNodeId = ProjectedNarrowingNodeId::ALWAYS_FALSE;
const LONG_KEY: Type<'static> = todo_type!(
    "narrowing cache key with a retained inline debug payload larger than later incoming keys"
);

struct Inputs<'db> {
    env: ProgramEnvironment<'db>,
    constraints: NarrowingConstraints,
    targets: PredicateNarrowingTargets,
}

impl<'db> Inputs<'db> {
    fn new(prepared: &PreparedAnalysisFile<'db>) -> Self {
        Self {
            env: ProgramEnvironment::from_file(prepared.program_file()),
            constraints: NarrowingConstraints::from_test_nodes(Vec::new()),
            targets: PredicateNarrowingTargets::default(),
        }
    }

    fn storage<'map>(&'map self, db: &'db dyn Db) -> Storage<'map, 'db> {
        Storage {
            projector: NarrowingProjector::new(
                db,
                &self.env,
                &self.constraints,
                IndexSlice::empty(),
                &self.targets,
                ScopedPlaceId::Symbol(ScopedSymbolId::new(0)),
                Type::unknown(),
            ),
            frames: None,
        }
    }
}

struct Storage<'map, 'db> {
    projector: NarrowingProjector<'map, 'db>,
    frames: Option<Construction<'db>>,
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot<'db> {
    nodes: Vec<(
        Option<ProjectedNarrowingNode>,
        Option<(ScopedNarrowingConstraint, Type<'db>)>,
    )>,
    referenced: Vec<bool>,
    joins: Vec<bool>,
    node_cache: FxHashMap<ProjectedNarrowingNode, ProjectedNarrowingNodeId>,
    or_cache:
        FxHashMap<(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId), ProjectedNarrowingNodeId>,
    project_cache: FxHashMap<(ScopedNarrowingConstraint, Type<'db>), ProjectedNarrowingNodeId>,
    capacities: [usize; 6],
    project_backing: usize,
    project_key_bytes: usize,
    frames: Option<(usize, usize, bool)>,
}

impl<'db> Storage<'_, 'db> {
    fn snapshot(&self) -> Snapshot<'db> {
        let graph = &self.projector.graph;
        Snapshot {
            nodes: graph
                .nodes
                .iter()
                .map(|entry| match *entry {
                    ProjectedNarrowingEntry::Predicate(node) => (Some(node), None),
                    ProjectedNarrowingEntry::Checkpoint { constraint, ty } => {
                        (None, Some((constraint, ty)))
                    }
                })
                .collect(),
            referenced: graph.referenced.clone(),
            joins: graph.joins.clone(),
            node_cache: graph.node_cache.clone(),
            or_cache: graph.or_cache.clone(),
            project_cache: self.projector.project_cache.clone(),
            capacities: [
                graph.nodes.capacity(),
                graph.referenced.capacity(),
                graph.joins.capacity(),
                graph.node_cache.capacity(),
                graph.or_cache.capacity(),
                self.projector.project_cache.capacity(),
            ],
            project_backing: self.projector.source_project_backing,
            project_key_bytes: self.projector.source_project_key_bytes,
            frames: self.frames.as_ref().map(|construction| {
                (
                    construction.frames.len(),
                    construction.frames.capacity(),
                    construction.frames.spilled(),
                )
            }),
        }
    }
}

#[derive(Clone, Copy)]
enum Operation<'db> {
    Append(ProjectedNarrowingNode),
    Checkpoint,
    PublishOr,
    PublishProjection(usize),
    RemoveProjection(usize),
    PrepareFrames,
    PushFrame,
    Add(ProjectedNarrowingNode),
    Or(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId),
    SetBase(Type<'db>),
    Narrow {
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
        expected: Type<'db>,
    },
    Owned(Option<AnalysisIncomplete>, bool),
}

#[derive(Default)]
struct Progress {
    entered: Cell<bool>,
    before: Cell<Option<usize>>,
    after: Cell<Option<usize>>,
    retired: RefCell<Vec<Retired>>,
}

#[derive(Debug, Eq, PartialEq)]
enum Retired {
    Frames { len: usize, spilled: bool },
    Graph { nodes: usize },
}

struct OwnedStorage<'owner, 'map, 'db> {
    frames: Option<Construction<'db>>,
    projector: Option<NarrowingProjector<'map, 'db>>,
    progress: &'owner Progress,
}

impl Drop for OwnedStorage<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(frames) = self.frames.take() {
            let event = Retired::Frames {
                len: frames.frames.len(),
                spilled: frames.frames.spilled(),
            };
            drop(frames);
            self.progress.retired.borrow_mut().push(event);
        }
        if let Some(projector) = self.projector.take() {
            let event = Retired::Graph {
                nodes: projector.graph.nodes.len(),
            };
            drop(projector);
            self.progress.retired.borrow_mut().push(event);
        }
    }
}

fn leaf(atom: usize) -> ProjectedNarrowingNode {
    ProjectedNarrowingNode {
        atom: ScopedPredicateId::new(atom),
        if_true: TRUE,
        if_uncertain: FALSE,
        if_false: FALSE,
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    storage: &mut Storage<'_, 'db>,
    operation: Operation<'db>,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<ProjectedNarrowingNodeId>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        let storage = &mut *storage;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            progress.entered.set(true);
            progress
                .before
                .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    session.db(),
                ));
            let result = match operation {
                Operation::Append(node) => {
                    effects
                        .append_predicate(&mut storage.projector, node)
                        .await?
                }
                Operation::Checkpoint => {
                    // This tests checkpoint storage, without invoking its semantic producer.
                    effects
                        .append_checkpoint(
                            &mut storage.projector,
                            ScopedNarrowingConstraint::new(0),
                            Type::unknown(),
                        )
                        .await?
                }
                Operation::PublishOr => {
                    effects
                        .publish_or(
                            &mut storage.projector,
                            (ProjectedNarrowingNodeId(0), ProjectedNarrowingNodeId(1)),
                            ProjectedNarrowingNodeId(2),
                        )
                        .await?;
                    ProjectedNarrowingNodeId(2)
                }
                Operation::PublishProjection(id) => {
                    effects
                        .publish_projection(
                            &mut storage.projector,
                            ScopedNarrowingConstraint::new(id),
                            ProjectedNarrowingNodeId(0),
                        )
                        .await?;
                    ProjectedNarrowingNodeId(0)
                }
                Operation::RemoveProjection(id) => {
                    effects
                        .remove_projection(
                            &mut storage.projector,
                            ScopedNarrowingConstraint::new(id),
                        )
                        .await?;
                    FALSE
                }
                Operation::PrepareFrames => {
                    let mut frames = effects.start(Frame::Or(TRUE, FALSE)).await?;
                    for _ in 1..8 {
                        effects.push(&mut frames, Frame::Or(TRUE, FALSE)).await?;
                    }
                    storage.frames = Some(frames);
                    FALSE
                }
                Operation::PushFrame => {
                    let Some(frames) = &mut storage.frames else {
                        return Err(RunError::Contract("fixture has no construction"));
                    };
                    effects.push(frames, Frame::Or(TRUE, FALSE)).await?;
                    FALSE
                }
                Operation::Add(node) => {
                    effects
                        .build_narrowing_graph(&mut storage.projector, Frame::Add(node))
                        .await?
                }
                Operation::Or(left, right) => {
                    effects
                        .build_narrowing_graph(&mut storage.projector, Frame::Or(left, right))
                        .await?
                }
                Operation::SetBase(ty) => {
                    effects.set_base_type(&mut storage.projector, ty).await?;
                    FALSE
                }
                Operation::Narrow {
                    constraint,
                    base_ty,
                    expected,
                } => {
                    assert_eq!(
                        effects
                            .narrow_graph(&mut storage.projector, constraint, base_ty)
                            .await?,
                        expected,
                    );
                    TRUE
                }
                Operation::Owned(refusal, cancel) => {
                    let evaluator = prepared
                        .semantic_index()
                        .use_def_map(FileScopeId::global())
                        .narrowing_evaluator(ScopedNarrowingConstraint::ALWAYS_TRUE);
                    let projector = effects
                        .create_projector(
                            storage.projector.env,
                            &evaluator,
                            storage.projector.place,
                            Type::unknown(),
                        )
                        .await?;
                    let mut owned = OwnedStorage {
                        frames: None,
                        projector: Some(projector),
                        progress,
                    };
                    owned.frames = Some(effects.start(Frame::Or(TRUE, FALSE)).await?);
                    let (Some(projector), Some(frames)) = (&mut owned.projector, &mut owned.frames)
                    else {
                        return Err(RunError::Contract("fixture owners were consumed"));
                    };
                    effects.append_predicate(projector, leaf(0)).await?;
                    for _ in 1..9 {
                        effects.push(frames, Frame::Or(TRUE, FALSE)).await?;
                    }
                    if cancel {
                        access
                            .endpoint
                            .local_call(|| {
                                session.db().cancellation_token().cancel();
                                access.endpoint.check_completion()
                            })
                            .await;
                    }
                    if let Some(reason) = refusal {
                        access
                            .endpoint
                            .local_call(|| match reason {
                                AnalysisIncomplete::WorkLimit => {
                                    access.endpoint.admit_work(funded().semantic_work_limit)
                                }
                                AnalysisIncomplete::RequestedAllocationLimit => {
                                    access.endpoint.admit(ExecutionWork::Resource {
                                        requested_bytes: funded().requested_bytes_limit,
                                    })
                                }
                                AnalysisIncomplete::UnavailableOperation(_) => {
                                    Err(RunError::Contract("invalid fixture refusal"))
                                }
                            })
                            .await;
                    }
                    TRUE
                }
            };
            progress
                .after
                .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    session.db(),
                ));
            Ok(result)
        })
    })
}

fn complete<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    storage: &mut Storage<'_, 'db>,
    operation: Operation<'db>,
) -> ProjectedNarrowingNodeId {
    let outcome = controlled(
        prepared,
        storage,
        operation,
        &funded(),
        &Progress::default(),
    );
    let Ok(AnalysisOutcome::Complete(result)) = outcome else {
        panic!("funded structural operation failed: {outcome:?}");
    };
    result
}

#[derive(Clone, Copy, Debug)]
enum Boundary {
    Predicate,
    Checkpoint,
    OrCache,
    ProjectionCache,
    FrameSpill,
}

fn seed_projection<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    storage: &mut Storage<'_, 'db>,
    long: bool,
) {
    complete(prepared, storage, Operation::Add(leaf(0)));
    complete(
        prepared,
        storage,
        Operation::SetBase(if long { LONG_KEY } else { Type::unknown() }),
    );
    complete(prepared, storage, Operation::PublishProjection(0));
    complete(prepared, storage, Operation::SetBase(Type::unknown()));
    let mut id = 1;
    while storage.projector.project_cache.len() < storage.projector.project_cache.capacity() {
        complete(prepared, storage, Operation::PublishProjection(id));
        id += 1;
    }
    assert!(id > 1);
}

fn seed<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    storage: &mut Storage<'_, 'db>,
    boundary: Boundary,
) -> Operation<'db> {
    match boundary {
        Boundary::Predicate => {
            for atom in 0..3 {
                complete(prepared, storage, Operation::Add(leaf(atom)));
            }
            Operation::Append(ProjectedNarrowingNode {
                if_true: ProjectedNarrowingNodeId(0),
                if_uncertain: ProjectedNarrowingNodeId(0),
                if_false: ProjectedNarrowingNodeId(1),
                ..leaf(3)
            })
        }
        Boundary::Checkpoint => Operation::Checkpoint,
        Boundary::OrCache => {
            let left = complete(prepared, storage, Operation::Add(leaf(0)));
            let right = complete(prepared, storage, Operation::Add(leaf(1)));
            assert_eq!(
                complete(prepared, storage, Operation::Or(left, right)),
                ProjectedNarrowingNodeId(2)
            );
            // Keep the actual graph result, but prepare a cache miss outside the measured run.
            storage.projector.graph.or_cache = FxHashMap::default();
            Operation::PublishOr
        }
        Boundary::ProjectionCache => {
            seed_projection(prepared, storage, true);
            Operation::PublishProjection(1)
        }
        Boundary::FrameSpill => {
            complete(prepared, storage, Operation::PrepareFrames);
            assert_eq!(storage.snapshot().frames, Some((8, 8, false)));
            Operation::PushFrame
        }
    }
}

#[test]
fn construction_storage_refuses_before_mutation_and_retries() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    // These owners outlive the run so their complete storage can be checked after refusal.
    // Fixture preparation and final owner disposal are outside the operation being measured.
    for boundary in [
        Boundary::Predicate,
        Boundary::Checkpoint,
        Boundary::OrCache,
        Boundary::ProjectionCache,
        Boundary::FrameSpill,
    ] {
        let mut measured = inputs.storage(&db);
        let operation = seed(&prepared, &mut measured, boundary);
        let progress = Progress::default();
        assert!(matches!(
            controlled(&prepared, &mut measured, operation, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(_))
        ));
        let Some(remaining) = progress.after.get() else {
            panic!("operation did not complete");
        };
        let work = funded().semantic_work_limit - remaining;
        let expected = measured.snapshot();
        if matches!(boundary, Boundary::Predicate) {
            assert!(expected.referenced[0] && expected.joins[0]);
            assert!(expected.referenced[1]);
        }

        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        for _ in 0..usize::BITS {
            if lower == upper {
                break;
            }
            let middle = lower + (upper - lower) / 2;
            let mut probe = inputs.storage(&db);
            let operation = seed(&prepared, &mut probe, boundary);
            let progress = Progress::default();
            let outcome = controlled(
                &prepared,
                &mut probe,
                operation,
                &AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                },
                &progress,
            );
            assert!(
                matches!(
                    outcome,
                    Ok(AnalysisOutcome::Complete(_))
                        | Ok(AnalysisOutcome::Incomplete {
                            reason: AnalysisIncomplete::RequestedAllocationLimit,
                            ..
                        })
                ),
                "{outcome:?}"
            );
            if progress.after.get().is_some() {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        assert_eq!(lower, upper);
        assert!(work > 0 && upper > 0);
        for (policy, reason) in [
            (
                AnalysisPolicy {
                    semantic_work_limit: work - 1,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: upper - 1,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            let mut storage = inputs.storage(&db);
            let operation = seed(&prepared, &mut storage, boundary);
            let before = storage.snapshot();
            let progress = Progress::default();
            assert_eq!(
                controlled(&prepared, &mut storage, operation, &policy, &progress),
                Ok(AnalysisOutcome::Incomplete {
                    reason,
                    completed: ()
                }),
                "{boundary:?}"
            );
            assert!(progress.entered.get());
            assert!(progress.after.get().is_none());
            assert_eq!(storage.snapshot(), before, "{boundary:?}");
            assert_no_active_attempt();
            complete(&prepared, &mut storage, operation);
            assert_eq!(storage.snapshot(), expected, "{boundary:?}");
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn shared_constructor_retains_graph_shape_edges_and_commutative_cache() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut storage = inputs.storage(&db);
    let first = complete(&prepared, &mut storage, Operation::Add(leaf(0)));
    let before = storage.snapshot();
    assert_eq!(
        complete(&prepared, &mut storage, Operation::Add(leaf(0))),
        first
    );
    assert_eq!(storage.snapshot(), before);
    let mut root = FALSE;
    for atom in 1..=4 {
        root = complete(
            &prepared,
            &mut storage,
            Operation::Add(ProjectedNarrowingNode {
                if_uncertain: root,
                ..leaf(atom)
            }),
        );
    }
    let combined = complete(&prepared, &mut storage, Operation::Or(root, first));
    assert_eq!(storage.projector.graph.nodes.len(), 9);
    let before = storage.snapshot();
    assert_eq!(
        complete(&prepared, &mut storage, Operation::Or(first, root)),
        combined
    );
    assert_eq!(storage.snapshot(), before);
    let mut current = combined;
    for atom in (0..=4).rev() {
        let ProjectedNarrowingEntry::Predicate(node) = storage.projector.graph.nodes[current.0]
        else {
            panic!("structural OR introduced a checkpoint");
        };
        assert_eq!(node.atom, ScopedPredicateId::new(atom));
        assert_eq!((node.if_true, node.if_false), (TRUE, FALSE));
        current = node.if_uncertain;
    }
    assert_eq!(current, FALSE);
    assert!(storage.projector.graph.referenced[first.0]);
    complete(
        &prepared,
        &mut storage,
        Operation::Add(ProjectedNarrowingNode {
            if_true: first,
            ..leaf(5)
        }),
    );
    assert!(storage.projector.graph.joins[first.0]);
    assert_eq!(
        storage.projector.graph.nodes.len(),
        storage.projector.graph.referenced.len()
    );
    assert_eq!(
        storage.projector.graph.nodes.len(),
        storage.projector.graph.joins.len()
    );
    assert_no_active_attempt();
}

#[test]
fn source_call_gate_constructs_terminal_results_and_retains_descendant_refusal() {
    for (parameter, unsupported_annotation) in [
        ("value", false),
        ("value: bool", false),
        ("value: \"bool\"", true),
    ] {
        let mut db = setup_db();
        db.write_file(
            "src/main.py",
            format!("def choose({parameter}):\n    return value\nchoose(True)\nif choose:\n    pass\n"),
        )
        .unwrap();
        let prepared = prepare(&db);
        let index = prepared.semantic_index();
        let scope = FileScopeId::global();
        let Some(symbol) = index.place_table(scope).symbol_id("choose") else {
            panic!("fixture has no choose symbol");
        };
        let Some(binding) = index
            .use_def_map(scope)
            .end_of_scope_symbol_bindings(symbol)
            .find(|binding| binding.binding.definition().is_some())
        else {
            panic!("fixture has no choose definition");
        };
        let evaluator = binding.narrowing_constraint;
        let constraint = evaluator.constraint();
        assert!(!constraint.is_terminal());
        let node = evaluator
            .narrowing_constraints()
            .get_interior_node(constraint);
        assert!(matches!(
            evaluator.predicates()[node.atom].node,
            PredicateNode::IsNonTerminalCall(_)
        ));
        assert_eq!(
            (node.if_true, node.if_uncertain, node.if_false),
            (
                ScopedNarrowingConstraint::ALWAYS_TRUE,
                ScopedNarrowingConstraint::ALWAYS_FALSE,
                ScopedNarrowingConstraint::ALWAYS_FALSE
            ),
        );
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let place = ScopedPlaceId::Symbol(symbol);
        let base_ty = Type::bool_literal(true);
        let projector = || {
            NarrowingProjector::new(
                &db,
                &env,
                evaluator.narrowing_constraints(),
                evaluator.predicates(),
                evaluator.predicate_narrowing_targets(),
                place,
                base_ty,
            )
        };
        let mut storage = Storage {
            projector: projector(),
            frames: None,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let operation = Operation::Narrow {
            constraint,
            base_ty,
            expected: base_ty,
        };
        let progress = Progress::default();
        let outcome = controlled(&prepared, &mut storage, operation, &funded(), &progress);
        if unsupported_annotation {
            assert_eq!(outcome, Ok(unavailable(OperationId::TypeExpressionLegacy)));
            assert!(progress.entered.get() && progress.after.get().is_none());
            assert!(storage.projector.project_cache.is_empty());
            assert!(storage.projector.graph.nodes.is_empty());
            assert_eq!(
                controlled(
                    &prepared,
                    &mut storage,
                    operation,
                    &funded(),
                    &Progress::default()
                ),
                outcome,
            );
        } else {
            assert_eq!(outcome, Ok(AnalysisOutcome::Complete(TRUE)));
            assert_eq!(
                storage.projector.project_cache.get(&(constraint, base_ty)),
                Some(&TRUE)
            );
            complete(&prepared, &mut storage, operation);
            // The gate comes from source. Its false child tests the terminal Never result;
            // it does not establish support for analyzing a Never-returning callable.
            complete(
                &prepared,
                &mut storage,
                Operation::Narrow {
                    constraint: node.if_false,
                    base_ty,
                    expected: Type::Never,
                },
            );
            let ordinary = OrdinaryNarrowingEntryEffects::new(&db);
            for (constraint, expected) in [(constraint, base_ty), (node.if_false, Type::Never)] {
                assert_eq!(
                    ordinary.narrow_graph(&mut projector(), constraint, base_ty),
                    Ok(expected),
                );
            }
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn projection_cache_keeps_removed_backing_and_resident_key_costs() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut costs = Vec::new();
    for long in [false, true] {
        let mut storage = inputs.storage(&db);
        seed_projection(&prepared, &mut storage, long);
        let mut next_id = storage.projector.project_cache.len();
        while next_id < 32
            || storage.projector.project_cache.len() < storage.projector.project_cache.capacity()
        {
            complete(
                &prepared,
                &mut storage,
                Operation::PublishProjection(next_id),
            );
            next_id += 1;
        }
        let count = storage.projector.project_cache.len();
        let progress = Progress::default();
        assert!(matches!(
            controlled(
                &prepared,
                &mut storage,
                Operation::PublishProjection(1),
                &funded(),
                &progress
            ),
            Ok(AnalysisOutcome::Complete(_))
        ));
        let (Some(before), Some(after)) = (progress.before.get(), progress.after.get()) else {
            panic!("replacement did not complete");
        };
        costs.push(before - after);
        assert_eq!(storage.projector.project_cache.len(), count);
        while storage.projector.project_cache.len() < storage.projector.project_cache.capacity() {
            complete(
                &prepared,
                &mut storage,
                Operation::PublishProjection(next_id),
            );
            next_id += 1;
        }
        let count = storage.projector.project_cache.len();
        let capacity = storage.projector.project_cache.capacity();
        let backing = storage.projector.source_project_backing;
        let key_bytes = storage.projector.source_project_key_bytes;
        complete(
            &prepared,
            &mut storage,
            Operation::SetBase(if long { LONG_KEY } else { Type::unknown() }),
        );
        complete(&prepared, &mut storage, Operation::RemoveProjection(0));
        complete(&prepared, &mut storage, Operation::SetBase(Type::unknown()));
        for id in 1..count {
            complete(&prepared, &mut storage, Operation::RemoveProjection(id));
        }
        assert!(storage.projector.project_cache.is_empty());
        assert!(storage.projector.project_cache.capacity() < capacity);
        assert_eq!(storage.projector.source_project_backing, backing);
        assert_eq!(storage.projector.source_project_key_bytes, key_bytes);
        complete(&prepared, &mut storage, Operation::PublishProjection(count));
        assert!(storage.projector.source_project_backing >= backing);
        assert!(storage.projector.source_project_key_bytes >= key_bytes);
    }
    if cfg!(debug_assertions) {
        assert!(
            costs[1] > costs[0],
            "resident inline keys must contribute to rehashing work"
        );
    }
    assert_no_active_attempt();
}

#[test]
fn owned_graph_and_spilled_frames_retire_after_refusal_and_retry() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut storage = inputs.storage(&db);
    let revision = salsa::plumbing::current_revision(&db);
    for reason in [
        AnalysisIncomplete::WorkLimit,
        AnalysisIncomplete::RequestedAllocationLimit,
    ] {
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                &mut storage,
                Operation::Owned(Some(reason), false),
                &funded(),
                &progress
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert_eq!(
            *progress.retired.borrow(),
            [
                Retired::Frames {
                    len: 9,
                    spilled: true
                },
                Retired::Graph { nodes: 1 }
            ]
        );
        assert!(progress.after.get().is_none());
        assert_no_active_attempt();
        let retry = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                &mut storage,
                Operation::Owned(None, false),
                &funded(),
                &retry
            ),
            Ok(AnalysisOutcome::Complete(TRUE))
        );
        assert_eq!(*retry.retired.borrow(), *progress.retired.borrow());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn owned_graph_and_spilled_frames_retire_after_native_cancellation() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut storage = inputs.storage(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        controlled(
            &prepared,
            &mut storage,
            Operation::Owned(None, true),
            &funded(),
            &progress,
        )
    }));
    assert!(
        matches!(outcome, Err(salsa::Cancelled::Local)),
        "{outcome:?}"
    );
    assert_eq!(
        *progress.retired.borrow(),
        [
            Retired::Frames {
                len: 9,
                spilled: true
            },
            Retired::Graph { nodes: 1 }
        ],
    );
    assert!(progress.after.get().is_none());
    assert_no_active_attempt();
    let retry = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &mut storage,
            Operation::Owned(None, false),
            &funded(),
            &retry,
        ),
        Ok(AnalysisOutcome::Complete(TRUE)),
    );
    assert_eq!(*retry.retired.borrow(), *progress.retired.borrow());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
