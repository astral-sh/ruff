use std::panic::AssertUnwindSafe;

use ruff_index::{Idx, IndexSlice};
use salsa::execution_probe::FinalSourceMemo;
use ty_python_core::PredicateNarrowingTargets;
use ty_python_core::narrowing_constraints::NarrowingConstraints;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::ScopedPredicateId;
use ty_python_core::symbol::ScopedSymbolId;

use super::*;
use crate::reachability::narrowing_construction::NarrowingConstructionEffects;
use crate::reachability::narrowing_entry::NarrowingEntryEffects;
use crate::reachability::narrowing_evaluation::{
    Evaluation, EvaluationMode, Frame, NarrowingEvaluationEffects, evaluation_observations,
};
use crate::reachability::{
    NarrowingProjector, ProjectedNarrowingEntry, ProjectedNarrowingNode, ProjectedNarrowingNodeId,
};
use crate::types::NarrowingConstraint;
use crate::types::narrow::application::tests::{Shape, constraint};

const TRUE: ProjectedNarrowingNodeId = ProjectedNarrowingNodeId::ALWAYS_TRUE;
const FALSE: ProjectedNarrowingNodeId = ProjectedNarrowingNodeId::ALWAYS_FALSE;

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

    fn projector<'map>(&'map self, db: &'db dyn Db) -> NarrowingProjector<'map, 'db> {
        NarrowingProjector::new(
            db,
            &self.env,
            &self.constraints,
            IndexSlice::empty(),
            &self.targets,
            ScopedPlaceId::Symbol(ScopedSymbolId::new(0)),
            Type::unknown(),
        )
    }
}

fn append<'db>(
    projector: &mut NarrowingProjector<'_, 'db>,
    branches: [ProjectedNarrowingNodeId; 3],
    positive: Option<NarrowingConstraint<'db>>,
    negative: Option<NarrowingConstraint<'db>>,
) -> ProjectedNarrowingNodeId {
    let id = ProjectedNarrowingNodeId(projector.graph.nodes.len());
    let atom = ScopedPredicateId::new(id.0);
    projector
        .graph
        .nodes
        .push(ProjectedNarrowingEntry::Predicate(ProjectedNarrowingNode {
            atom,
            if_true: branches[0],
            if_uncertain: branches[1],
            if_false: branches[2],
        }));
    projector.graph.referenced.push(false);
    projector.graph.joins.push(false);
    projector
        .graph
        .predicate_constraints_cache
        .insert(atom, (positive, negative));
    id
}

#[derive(Clone, Copy)]
enum Action<'db> {
    Evaluate(ProjectedNarrowingNodeId, Type<'db>),
    Owned,
    Spill,
}

#[derive(Default)]
struct Progress {
    cancel: bool,
    completed: Cell<bool>,
    retired: Cell<Option<(usize, usize)>>,
    retired_frames: Cell<Option<(usize, bool)>>,
}

struct OwnedEvaluation<'owner, 'db> {
    evaluation: Option<Evaluation<'db>>,
    progress: &'owner Progress,
}

impl Drop for OwnedEvaluation<'_, '_> {
    fn drop(&mut self) {
        if let Some(evaluation) = self.evaluation.take() {
            let retained = (evaluation.frames.len(), evaluation.frames.spilled());
            drop(evaluation);
            self.progress.retired_frames.set(Some(retained));
        }
    }
}

struct OwnedProjector<'owner, 'map, 'db> {
    projector: Option<NarrowingProjector<'map, 'db>>,
    progress: &'owner Progress,
}

impl Drop for OwnedProjector<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(projector) = self.projector.take() {
            let retained = (projector.graph.nodes.len(), projector.narrowed_cache.len());
            drop(projector);
            self.progress.retired.set(Some(retained));
        }
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    projector: &mut NarrowingProjector<'_, 'db>,
    action: Action<'db>,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    evaluation_observations::reset(progress.cancel);
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
        let projector = &mut *projector;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            let result = match action {
                Action::Evaluate(root, base_ty) => {
                    effects
                        .evaluate_narrowing_graph(projector, root, base_ty)
                        .await?
                }
                Action::Owned => {
                    let evaluator = prepared
                        .semantic_index()
                        .use_def_map(FileScopeId::global())
                        .narrowing_evaluator(ScopedNarrowingConstraint::ALWAYS_TRUE);
                    let retained = effects
                        .create_projector(
                            projector.env,
                            &evaluator,
                            projector.place,
                            Type::unknown(),
                        )
                        .await?;
                    let mut owner = OwnedProjector {
                        projector: Some(retained),
                        progress,
                    };
                    let Some(retained) = owner.projector.as_mut() else {
                        return Err(RunError::Contract("fixture projector was consumed"));
                    };
                    // The fixture supplies a completed suffix to isolate graph and frame ownership.
                    let root = effects
                        .append_checkpoint(
                            retained,
                            ScopedNarrowingConstraint::new(0),
                            Type::AlwaysTruthy,
                        )
                        .await?;
                    effects
                        .evaluate_narrowing_graph(retained, root, Type::unknown())
                        .await?
                }
                Action::Spill => {
                    let mut owner = OwnedEvaluation {
                        evaluation: Some(
                            NarrowingEvaluationEffects::start(&effects, TRUE, None).await?,
                        ),
                        progress,
                    };
                    let Some(evaluation) = owner.evaluation.as_mut() else {
                        return Err(RunError::Contract("fixture evaluation was consumed"));
                    };
                    for _ in 1..5 {
                        NarrowingEvaluationEffects::push(
                            &effects,
                            evaluation,
                            Frame::Evaluate(TRUE, None, EvaluationMode::Path),
                        )
                        .await?;
                    }
                    Type::unknown()
                }
            };
            progress.completed.set(true);
            Ok(result)
        })
    })
}

fn complete<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    projector: &mut NarrowingProjector<'_, 'db>,
    root: ProjectedNarrowingNodeId,
    base_ty: Type<'db>,
) -> Type<'db> {
    let result = controlled(
        prepared,
        projector,
        Action::Evaluate(root, base_ty),
        &funded(),
        &Progress::default(),
    );
    let Ok(AnalysisOutcome::Complete(ty)) = result else {
        panic!("graph evaluation did not complete: {result:?}");
    };
    assert_eq!(evaluation_observations::progress().0, 0);
    assert_no_active_attempt();
    ty
}

#[test]
fn retained_roots_and_joins_use_the_original_binding_type() {
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut projector = inputs.projector(&db);
    let suffix = append(&mut projector, [TRUE, FALSE, FALSE], None, None);
    projector.graph.referenced[suffix.0] = true;
    projector.graph.joins[suffix.0] = true;
    let root = append(&mut projector, [suffix, FALSE, FALSE], None, None);
    let revision = salsa::plumbing::current_revision(&db);

    for base_ty in [Type::bool_literal(true), Type::bool_literal(false)] {
        assert_eq!(complete(&prepared, &mut projector, root, base_ty), base_ty);
        assert_eq!(
            projector.narrowed_cache.get(&(root, base_ty)),
            Some(&base_ty)
        );
        assert_eq!(
            projector.narrowed_cache.get(&(suffix, base_ty)),
            Some(&base_ty)
        );
    }
    assert_eq!(projector.narrowed_cache.len(), 4);
    assert!(projector.graph.joins[root.0]);
    assert_eq!(
        complete(&prepared, &mut projector, root, Type::bool_literal(true)),
        Type::bool_literal(true),
    );
    assert_eq!(evaluation_observations::progress().1, 0);
    assert_eq!(projector.narrowed_cache.len(), 4);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn a_cached_root_is_recorded_as_a_join_before_lookup() {
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut projector = inputs.projector(&db);
    let root = append(&mut projector, [TRUE, FALSE, FALSE], None, None);
    assert_eq!(
        complete(&prepared, &mut projector, root, Type::unknown()),
        Type::unknown()
    );
    assert!(!projector.graph.joins[root.0]);
    assert_eq!(
        complete(&prepared, &mut projector, root, Type::unknown()),
        Type::unknown()
    );
    assert!(projector.graph.joins[root.0]);
    assert_eq!(evaluation_observations::progress().1, 0);
}

#[test]
fn replacement_order_is_preserved_across_paths_and_join_suffixes() {
    for join in [false, true] {
        for replacement_prefix in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let inputs = Inputs::new(&prepared);
            let mut projector = inputs.projector(&db);
            let replacement = constraint(Shape::Replacement);
            let intersection = NarrowingConstraint::intersection(Type::Never);
            let (prefix, suffix_constraint, expected) = if replacement_prefix {
                (replacement, intersection, Type::AlwaysTruthy)
            } else {
                (intersection, replacement, Type::Never)
            };
            let suffix = append(
                &mut projector,
                [TRUE, FALSE, FALSE],
                Some(suffix_constraint),
                None,
            );
            projector.graph.referenced[suffix.0] = join;
            projector.graph.joins[suffix.0] = join;
            let root = append(&mut projector, [suffix, FALSE, FALSE], Some(prefix), None);
            assert_eq!(
                complete(&prepared, &mut projector, root, Type::unknown()),
                expected
            );
            if join {
                let suffix_ty = if replacement_prefix {
                    Type::Never
                } else {
                    Type::AlwaysTruthy
                };
                assert_eq!(
                    projector.narrowed_cache.get(&(suffix, Type::unknown())),
                    Some(&suffix_ty)
                );
                assert!(!projector.narrowed_cache.contains_key(&(suffix, suffix_ty)));
            }
        }
    }
}

#[test]
fn unreachable_paths_discard_replacements_and_branches_publish_canonical_pairs() {
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut projector = inputs.projector(&db);
    let unreachable = append(
        &mut projector,
        [FALSE, FALSE, FALSE],
        None,
        Some(constraint(Shape::Replacement)),
    );
    assert_eq!(
        complete(&prepared, &mut projector, unreachable, Type::unknown()),
        Type::Never
    );
    let root = append(&mut projector, [FALSE, TRUE, FALSE], None, None);
    let base_ty = Type::AlwaysTruthy;
    assert_eq!(complete(&prepared, &mut projector, root, base_ty), base_ty);
    for (left, right) in [(Type::Never, base_ty), (base_ty, Type::Never)] {
        let key = TypePair::new(&db, inputs.env.program(&db), left, right);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                union_from_two_elements_ingredient(&db),
                key.as_id(),
            )
            .is_ok()
        );
    }
    assert_eq!(
        complete(&prepared, &mut projector, FALSE, base_ty),
        Type::Never
    );
    assert_eq!(complete(&prepared, &mut projector, TRUE, base_ty), base_ty);
    assert_eq!(projector.narrowed_cache.len(), 2);
}

#[test]
fn completed_join_survives_a_prefix_refusal_without_publishing_the_root() {
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let mut projector = inputs.projector(&db);
    let suffix = append(&mut projector, [TRUE, FALSE, FALSE], None, None);
    projector.graph.referenced[suffix.0] = true;
    projector.graph.joins[suffix.0] = true;
    // Generic filtering remains an explicit descendant boundary after suffix completion.
    let root = append(
        &mut projector,
        [suffix, FALSE, FALSE],
        Some(constraint(Shape::ReplacementFirst)),
        None,
    );
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        assert_eq!(
            controlled(
                &prepared,
                &mut projector,
                Action::Evaluate(root, Type::unknown()),
                &funded(),
                &Progress::default()
            ),
            Ok(unavailable(OperationId::Narrowing)),
        );
        assert_eq!(
            projector.narrowed_cache.get(&(suffix, Type::unknown())),
            Some(&Type::unknown())
        );
        assert!(
            !projector
                .narrowed_cache
                .contains_key(&(root, Type::unknown()))
        );
        assert_eq!(projector.narrowed_cache.len(), 1);
        assert_eq!(evaluation_observations::progress().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn owned_graph_and_evaluation_retire_on_work_refusal_and_native_cancellation() {
    let measured = fixture();
    let prepared = prepare(&measured);
    let inputs = Inputs::new(&prepared);
    assert_eq!(
        controlled(
            &prepared,
            &mut inputs.projector(&measured),
            Action::Owned,
            &funded(),
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Complete(Type::AlwaysTruthy)),
    );
    let (live, entered, remaining) = evaluation_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let Some(remaining) = remaining else {
        panic!("evaluation did not retain its owner");
    };
    let retained_work = funded().semantic_work_limit - remaining;
    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let inputs = Inputs::new(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel,
            ..Progress::default()
        };
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                &mut inputs.projector(&db),
                Action::Owned,
                &policy,
                &progress,
            )
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(progress.retired.get(), Some((1, 0)));
        assert!(!progress.completed.get());
        assert_eq!(evaluation_observations::progress().0, 0);
        assert_eq!(evaluation_observations::progress().1, 1);
        assert_no_active_attempt();
        let retry = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                &mut inputs.projector(&db),
                Action::Owned,
                &funded(),
                &retry
            ),
            Ok(AnalysisOutcome::Complete(Type::AlwaysTruthy)),
        );
        assert_eq!(retry.retired.get(), Some((1, 1)));
        assert_eq!(evaluation_observations::progress().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn allocation_refusal_retires_the_graph_and_evaluation_before_retry() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let inputs = Inputs::new(&prepared);
        match controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Owned,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        ) {
            Ok(AnalysisOutcome::Complete(Type::AlwaysTruthy)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_eq!(evaluation_observations::progress().0, 0);
        assert_no_active_attempt();
    }
    assert!(upper > 0);
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Owned,
            &AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            &progress
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }),
    );
    assert_eq!(progress.retired.get(), Some((1, 0)));
    assert!(!progress.completed.get());
    assert_eq!(evaluation_observations::progress().0, 0);
    assert_eq!(evaluation_observations::progress().1, 1);
    assert_no_active_attempt();
    let retry = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Owned,
            &funded(),
            &retry
        ),
        Ok(AnalysisOutcome::Complete(Type::AlwaysTruthy))
    );
    assert_eq!(retry.retired.get(), Some((1, 1)));
    assert_eq!(evaluation_observations::progress().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn frame_spill_refusal_preserves_the_inline_frames_until_retirement() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let inputs = Inputs::new(&prepared);
        match controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Spill,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        ) {
            Ok(AnalysisOutcome::Complete(_)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_eq!(evaluation_observations::progress().0, 0);
        assert_no_active_attempt();
    }
    assert!(upper > 0);
    let db = fixture();
    let prepared = prepare(&db);
    let inputs = Inputs::new(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Spill,
            &AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            &progress
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }),
    );
    assert_eq!(progress.retired_frames.get(), Some((4, false)));
    assert!(!progress.completed.get());
    assert_eq!(evaluation_observations::progress().0, 0);
    assert_eq!(evaluation_observations::progress().1, 1);
    assert_no_active_attempt();

    let retry = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &mut inputs.projector(&db),
            Action::Spill,
            &funded(),
            &retry
        ),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    assert_eq!(retry.retired_frames.get(), Some((5, true)));
    assert_eq!(evaluation_observations::progress().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
