//! Scratch exact sharing of fresh-root callable relations in one constraint domain.
//!
//! The request captures admitted checker inputs rather than borrowing a caller's mutable
//! visitors or diagnostic tree. Composed callable roots retain completed comparisons when
//! another runtime alternative is still waiting for conversion or signature dependencies.

pub(in crate::types) mod composition;

pub(crate) use composition::{composition_payload_debit, composition_regularizes};

use super::scheduled_effects::QueuedSignatureEffects;
use super::{CallableRelationStep, TypeRelationChecker};
use crate::types::callable::CallableType;
use crate::types::callable::scheduled_probe::{Boundary, DiagnosticSeedId, Router};
use crate::types::constraints::{ConstraintHandleKey, ConstraintSet, ConstraintSetBuilder};
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeVarEvaluation,
};
use crate::types::relation_error::{ErrorContextTree, FrozenErrorContextTree};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, Type};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum EvidenceMode {
    None,
    Context,
}

/// Only fresh materialization roots are admitted by this experiment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RootPolicy {
    pub(crate) materialize_bounds: bool,
    pub(crate) expensive_checks: bool,
    pub(crate) evidence: EvidenceMode,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RelationGoal<'db> {
    CallableSource(CallableType<'db>),
    ComposedCallableSource {
        target: CallableType<'db>,
        regularize: bool,
        upcast_source: Option<Type<'db>>,
    },
    TypePair(Type<'db>),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RelationKey<'db> {
    source: Type<'db>,
    goal: RelationGoal<'db>,
    relation: TypeRelation,
    typevar_evaluation: TypeVarEvaluation,
    inferable: TypeVarSet<'db>,
    given: ConstraintHandleKey,
    policy: RootPolicy,
    seed: Option<DiagnosticSeedId>,
}

#[derive(Clone, Copy)]
pub(crate) struct RelationContext<'db, 'c> {
    pub(crate) relation: TypeRelation,
    pub(crate) typevar_evaluation: TypeVarEvaluation,
    pub(crate) inferable: TypeVarSet<'db>,
    pub(crate) given: ConstraintSet<'db, 'c>,
    pub(crate) policy: RootPolicy,
}

#[derive(Clone, Copy)]
pub(crate) struct RelationRequest<'db, 'c> {
    key: RelationKey<'db>,
    given: ConstraintSet<'db, 'c>,
}

impl<'db, 'c> RelationRequest<'db, 'c> {
    fn composed_callable(
        constraints: &'c ConstraintSetBuilder<'db>,
        source: Type<'db>,
        target: CallableType<'db>,
        context: RelationContext<'db, 'c>,
    ) -> Result<Self, Boundary> {
        Self::new(
            constraints,
            source,
            RelationGoal::ComposedCallableSource {
                target,
                regularize: false,
                upcast_source: None,
            },
            context,
        )
        .ok_or(Boundary::ConstraintDomain)
    }

    pub(crate) fn binder_assignability(
        constraints: &'c ConstraintSetBuilder<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<Self, Boundary> {
        Self::new(
            constraints,
            source,
            RelationGoal::TypePair(target),
            RelationContext {
                relation: TypeRelation::Assignability,
                typevar_evaluation: TypeVarEvaluation::Eager,
                inferable: TypeVarSet::None,
                given: ConstraintSet::from_bool(constraints, false),
                policy: RootPolicy {
                    materialize_bounds: true,
                    expensive_checks: true,
                    evidence: EvidenceMode::None,
                },
            },
        )
        .ok_or(Boundary::ConstraintDomain)
    }

    fn new(
        constraints: &'c ConstraintSetBuilder<'db>,
        source: Type<'db>,
        goal: RelationGoal<'db>,
        context: RelationContext<'db, 'c>,
    ) -> Option<Self> {
        Some(Self {
            key: RelationKey {
                source,
                goal,
                relation: context.relation,
                typevar_evaluation: context.typevar_evaluation,
                inferable: context.inferable,
                given: context.given.scheduling_key(constraints)?,
                policy: context.policy,
                seed: None,
            },
            given: context.given,
        })
    }

    pub(crate) fn key(self) -> RelationKey<'db> {
        self.key
    }

    pub(crate) fn is_composed(self) -> bool {
        matches!(self.key.goal, RelationGoal::ComposedCallableSource { .. })
    }

    pub(crate) fn source(self) -> Type<'db> {
        self.key.source
    }

    pub(crate) fn belongs_to(self, constraints: &'c ConstraintSetBuilder<'db>) -> bool {
        self.given.scheduling_key(constraints) == Some(self.key.given)
    }

    pub(crate) fn with_seed(mut self, seed: DiagnosticSeedId) -> Self {
        self.key.seed = Some(seed);
        self
    }

    pub(super) fn inherit(
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<Self, Boundary> {
        let mapping = checker.materialization_visitor;
        // This admission is deliberately narrower than an arbitrary checker snapshot. Active
        // mapping assumptions and observation sinks need explicit transport before suspension.
        if checker.observations.is_some()
            || mapping.recursion_context.is_some()
            || mapping.default.get().is_some()
            || mapping.top_materialization.get().is_some()
            || mapping.bottom_materialization.get().is_some()
            || mapping.top_specialization_materialization.get().is_some()
            || mapping
                .bottom_specialization_materialization
                .get()
                .is_some()
            || mapping.promotion.get().is_some()
            || mapping.skip_promotion.get().is_some()
            || mapping.materialization_equivalence.get().is_some()
        {
            return Err(Boundary::RelationContext);
        }
        Self::new(
            checker.constraints,
            source,
            RelationGoal::TypePair(target),
            RelationContext {
                relation: checker.relation,
                typevar_evaluation: checker.typevar_evaluation,
                inferable: checker.inferable,
                given: checker.given,
                policy: RootPolicy {
                    materialize_bounds: mapping.materialize_typevar_bounds_and_defaults,
                    expensive_checks: checker.perform_expensive_checks,
                    evidence: if checker.is_context_collection_enabled() {
                        EvidenceMode::Context
                    } else {
                        EvidenceMode::None
                    },
                },
            },
        )
        .ok_or(Boundary::ConstraintDomain)
    }
}

#[derive(Clone)]
pub(crate) struct RelationOutput<'db, 'c> {
    pub(crate) constraints: ConstraintSet<'db, 'c>,
    pub(crate) context: Option<FrozenErrorContextTree<'db>>,
}

pub(crate) async fn evaluate<'db, 'c>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    router: &Router<'db, 'c>,
    request: RelationRequest<'db, 'c>,
) -> Result<RelationOutput<'db, 'c>, Boundary> {
    let relation_visitor = HasRelationToVisitor::default(constraints);
    let disjointness_visitor = IsDisjointVisitor::default(constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mut mapping_visitor = ApplyTypeMappingVisitor::new(env);
    mapping_visitor.materialize_typevar_bounds_and_defaults = request.key.policy.materialize_bounds;
    let mut checker = TypeRelationChecker::new(
        env,
        request.key.relation,
        constraints,
        request.key.inferable,
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    );
    checker.given = request.given;
    checker.typevar_evaluation = request.key.typevar_evaluation;
    checker.perform_expensive_checks = request.key.policy.expensive_checks;
    checker.context_tree = match request.key.seed {
        Some(seed) => Some(router.diagnostic_seed(seed)?.instantiate()),
        None => match request.key.policy.evidence {
            EvidenceMode::None => None,
            EvidenceMode::Context => Some(ErrorContextTree::new(request.key.relation)),
        },
    };

    let result = match request.key.goal {
        RelationGoal::CallableSource(target) => {
            match CallableRelationStep::start(db, &checker, request.key.source, target) {
                CallableRelationStep::Complete(result) => result,
                CallableRelationStep::Convert(pending) => {
                    let callables = router
                        .relation_conversion_demand(request.key, pending.request)
                        .await?;
                    pending.resume(db, callables)
                }
            }
        }
        RelationGoal::TypePair(target) => {
            let effects = QueuedSignatureEffects {
                router,
                parent: request.key,
                env,
            };
            check_admitted_type_pair(db, &checker, &effects, request.key.source, target).await?
        }
        RelationGoal::ComposedCallableSource {
            target,
            regularize,
            upcast_source,
        } => {
            composition::evaluate_composed(
                db,
                &checker,
                router,
                request,
                target,
                regularize,
                upcast_source,
            )
            .await?
        }
    };
    let context = checker.context_tree.map(ErrorContextTree::freeze);
    Ok(RelationOutput {
        constraints: result,
        context,
    })
}

async fn check_admitted_type_pair<'db, 'c>(
    db: &'db dyn Db,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    effects: &QueuedSignatureEffects<'_, 'db, 'c>,
    source: Type<'db>,
    target: Type<'db>,
) -> Result<ConstraintSet<'db, 'c>, Boundary> {
    // For these atomic variants the existing relation performs no recursive semantic work.
    // Keep the actual relation authoritative, including gradual-type and redundancy rules.
    let atomic = |ty| matches!(ty, Type::Never | Type::Dynamic(_) | Type::LiteralValue(_));
    if atomic(source) && atomic(target) {
        return Ok(checker.check_type_pair(db, source, target));
    }
    if let (Type::Callable(source), Type::Callable(target)) = (source, target) {
        return checker
            .check_callable_pair_with(db, effects, source, target)
            .await;
    }
    Err(Boundary::SemanticOperation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::callable::scheduled_probe::run_with;
    use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
    use crate::types::relation_error::ErrorRelation;
    use crate::types::{
        BoundTypeVarInstance, ErrorContext, KnownClass, KnownInstanceType, Parameter, Parameters,
        Signature, TypeVarVariance,
    };
    use ruff_python_ast::name::Name;

    fn nested_pair(db: &dyn Db, depth: usize, valid: bool) -> (Type<'_>, Type<'_>) {
        let mut source = if valid {
            Type::Never
        } else {
            Type::int_literal(1)
        };
        let mut target = Type::int_literal(2);
        for _ in 0..depth {
            // Return and parameter checking demand the same ordered pair. The graph must
            // reuse that result while retaining the comparator's ordinary sequential order.
            let source_next = Type::Callable(CallableType::single(
                db,
                Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(None).with_annotated_type(target)
                    ]),
                    source,
                ),
            ));
            target = Type::Callable(CallableType::single(
                db,
                Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(None).with_annotated_type(source)
                    ]),
                    target,
                ),
            ));
            source = source_next;
        }
        (source, target)
    }

    #[test]
    fn scheduled_nested_signatures_share_return_and_parameter_demands() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        for depth in [0, 1, 4, 16, 64, 256, 1024] {
            for valid in [false, true] {
                // Frozen explanation instantiation still copies tree-sized payloads. Exercise
                // diagnostics separately at moderate depth; this chain measures relation work.
                let (source, target) = nested_pair(&db, depth, valid);
                let constraints = ConstraintSetBuilder::new();
                let request = RelationRequest::new(
                    &constraints,
                    source,
                    RelationGoal::TypePair(target),
                    RelationContext {
                        relation: TypeRelation::Assignability,
                        typevar_evaluation: TypeVarEvaluation::Eager,
                        inferable: TypeVarSet::None,
                        given: ConstraintSet::from_bool(&constraints, false),
                        policy: RootPolicy {
                            materialize_bounds: true,
                            expensive_checks: true,
                            evidence: EvidenceMode::None,
                        },
                    },
                )
                .expect("original builder");
                let router = Router::with_constraints(&constraints);
                let snapshot = run_with(
                    &db,
                    &env,
                    &router,
                    depth * 20 + 20,
                    false,
                    false,
                    |router| async {
                        let answer = router.consumer_relation_demand(request).await?;
                        Ok::<_, Boundary>(answer.constraints.is_trivially_always_satisfied())
                    },
                )
                .expect("fresh root");
                assert_eq!(
                    snapshot.consumer,
                    Some(Ok(valid)),
                    "depth {depth}, valid {valid}"
                );
                assert_eq!(snapshot.relation_polls.len(), depth + 1);
                let expected_polls = if valid { 3 * depth + 1 } else { 2 * depth + 1 };
                assert_eq!(
                    snapshot.relation_polls.values().sum::<usize>(),
                    expected_polls
                );
            }
        }
    }

    #[test]
    fn scheduled_nested_signatures_preserve_diagnostics_and_budget_boundaries() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        let (source, target) = nested_pair(&db, 4, false);
        let expected = source
            .assignability_error_context(&db, &env, target)
            .freeze();
        for budget in 0..=64 {
            let mut baseline = None;
            for reverse_execution in [false, true] {
                for reverse_merge in [false, true] {
                    let constraints = ConstraintSetBuilder::new();
                    let request = RelationRequest::new(
                        &constraints,
                        source,
                        RelationGoal::TypePair(target),
                        RelationContext {
                            relation: TypeRelation::Assignability,
                            typevar_evaluation: TypeVarEvaluation::Eager,
                            inferable: TypeVarSet::None,
                            given: ConstraintSet::from_bool(&constraints, false),
                            policy: RootPolicy {
                                materialize_bounds: true,
                                expensive_checks: true,
                                evidence: EvidenceMode::Context,
                            },
                        },
                    )
                    .expect("original builder");
                    let router = Router::with_constraints(&constraints);
                    let snapshot = run_with(
                        &db,
                        &env,
                        &router,
                        budget,
                        reverse_execution,
                        reverse_merge,
                        |router| async {
                            let answer = router.consumer_relation_demand(request).await?;
                            assert!(answer.constraints.is_trivially_never_satisfied());
                            assert_eq!(answer.context, Some(expected.clone()));
                            Ok::<_, Boundary>(())
                        },
                    )
                    .expect("fresh root");
                    let mut polls = snapshot
                        .relation_polls
                        .values()
                        .copied()
                        .collect::<Vec<_>>();
                    polls.sort_unstable();
                    let observation = (snapshot.consumer, snapshot.consumer_polls, polls);
                    if let Some(baseline) = &baseline {
                        assert_eq!(&observation, baseline);
                    } else {
                        baseline = Some(observation);
                    }
                    if budget == 64 {
                        assert_eq!(snapshot.consumer, Some(Ok(())));
                    }
                }
            }
        }
    }

    #[test]
    fn scheduled_nested_signatures_use_exact_incoming_diagnostics() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        for reverse_execution in [false, true] {
            for reverse_merge in [false, true] {
                for (source, target) in [
                    (Type::int_literal(1), Type::int_literal(1)),
                    nested_pair(&db, 3, false),
                ] {
                    for enabled in [false, true] {
                        let constraints = ConstraintSetBuilder::new();
                        let router = Router::with_constraints(&constraints);
                        let seed = ErrorContextTree::from_context(
                            ErrorContext::DisjointTypes {
                                left: Type::int_literal(3),
                                right: Type::int_literal(4),
                            },
                            ErrorRelation::Disjointness,
                        );
                        seed.set_enabled(enabled);
                        let frozen_seed = seed.snapshot();
                        let seed_id = router
                            .intern_diagnostic_seed(frozen_seed.clone())
                            .expect("seed domain");
                        let same_seed = ErrorContextTree::from_context(
                            ErrorContext::DisjointTypes {
                                left: Type::int_literal(3),
                                right: Type::int_literal(4),
                            },
                            ErrorRelation::Disjointness,
                        );
                        same_seed.set_enabled(enabled);
                        assert_eq!(
                            Ok(seed_id),
                            router.intern_diagnostic_seed(same_seed.snapshot())
                        );
                        let foreign = Router::with_constraints(&constraints);
                        assert!(matches!(
                            foreign.diagnostic_seed(seed_id),
                            Err(Boundary::DiagnosticSeed)
                        ));

                        let expected_context = frozen_seed.instantiate();
                        let relations = HasRelationToVisitor::default(&constraints);
                        let disjointness = IsDisjointVisitor::default(&constraints);
                        let signatures = SignatureRelationVisitor::default();
                        let mapping = ApplyTypeMappingVisitor::new(&env);
                        let mut checker = TypeRelationChecker::new(
                            &env,
                            TypeRelation::Assignability,
                            &constraints,
                            TypeVarSet::None,
                            &relations,
                            &disjointness,
                            &signatures,
                            &mapping,
                        );
                        checker.context_tree = Some(expected_context.clone());
                        let expected_result = checker.check_type_pair(&db, source, target);
                        let expected_context = expected_context.freeze();
                        let request = RelationRequest::new(
                            &constraints,
                            source,
                            RelationGoal::TypePair(target),
                            RelationContext {
                                relation: TypeRelation::Assignability,
                                typevar_evaluation: TypeVarEvaluation::Eager,
                                inferable: TypeVarSet::None,
                                given: ConstraintSet::from_bool(&constraints, false),
                                policy: RootPolicy {
                                    materialize_bounds: true,
                                    expensive_checks: true,
                                    evidence: EvidenceMode::Context,
                                },
                            },
                        )
                        .expect("original builder")
                        .with_seed(seed_id);
                        let snapshot = run_with(
                            &db,
                            &env,
                            &router,
                            100,
                            reverse_execution,
                            reverse_merge,
                            |router| async {
                                let result = router.consumer_relation_demand(request).await?;
                                assert_eq!(
                                    result.constraints.scheduling_key(&constraints),
                                    expected_result.scheduling_key(&constraints)
                                );
                                assert_eq!(result.context, Some(expected_context));
                                Ok::<_, Boundary>(())
                            },
                        )
                        .expect("fresh root");
                        assert_eq!(snapshot.consumer, Some(Ok(())));
                        assert_eq!(seed.snapshot(), frozen_seed);
                    }
                }
            }
        }
    }

    #[test]
    fn scheduled_relations_partition_distinct_diagnostic_inputs() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        for reverse in [false, true] {
            let constraints = ConstraintSetBuilder::new();
            let router = Router::with_constraints(&constraints);
            let seed = |n| {
                ErrorContextTree::from_context(
                    ErrorContext::NotAssignableToNOtherUnionElements { n },
                    TypeRelation::Assignability,
                )
                .freeze()
            };
            let seeds = [seed(1), seed(2)];
            let requests = seeds.each_ref().map(|seed| {
                RelationRequest::new(
                    &constraints,
                    Type::int_literal(1),
                    RelationGoal::TypePair(Type::int_literal(1)),
                    RelationContext {
                        relation: TypeRelation::Assignability,
                        typevar_evaluation: TypeVarEvaluation::Eager,
                        inferable: TypeVarSet::None,
                        given: ConstraintSet::from_bool(&constraints, false),
                        policy: RootPolicy {
                            materialize_bounds: true,
                            expensive_checks: true,
                            evidence: EvidenceMode::Context,
                        },
                    },
                )
                .expect("original builder")
                .with_seed(
                    router
                        .intern_diagnostic_seed(seed.clone())
                        .expect("seed domain"),
                )
            });
            assert_ne!(requests[0].key(), requests[1].key());
            let snapshot = run_with(&db, &env, &router, 30, reverse, reverse, |router| async {
                router.declare_relation(requests[0])?;
                router.declare_relation(requests[1])?;
                for index in if reverse { [1, 0] } else { [0, 1] } {
                    let answer = router.consumer_relation_demand(requests[index]).await?;
                    assert_eq!(answer.context, Some(seeds[index].clone()));
                }
                Ok::<_, Boundary>(())
            })
            .expect("fresh root");
            assert_eq!(snapshot.consumer, Some(Ok(())));
            assert_eq!(snapshot.relation_polls.len(), 2);
            assert!(snapshot.relation_polls.values().all(|polls| *polls == 1));
        }
    }

    fn request<'db, 'c>(
        constraints: &'c ConstraintSetBuilder<'db>,
        source: Type<'db>,
        target: CallableType<'db>,
        evidence: EvidenceMode,
    ) -> RelationRequest<'db, 'c> {
        RelationRequest::new(
            constraints,
            source,
            RelationGoal::CallableSource(target),
            RelationContext {
                relation: TypeRelation::Assignability,
                typevar_evaluation: TypeVarEvaluation::Eager,
                inferable: TypeVarSet::None,
                given: ConstraintSet::from_bool(constraints, false),
                policy: RootPolicy {
                    materialize_bounds: true,
                    expensive_checks: true,
                    evidence,
                },
            },
        )
        .expect("given belongs to this builder")
    }

    #[test]
    fn scheduled_shared_relations_retain_results_and_explanations() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let unary = CallableType::single(
            &db,
            Signature::new(
                Parameters::standard([Parameter::positional_only(None).with_annotated_type(int)]),
                int,
            ),
        );
        let nullary = CallableType::single(&db, Signature::new(Parameters::empty(), int));
        let source = Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
            &db,
            Type::Callable(unary),
            MethodWrapperKind::Staticmethod,
        )));

        for budget in 0..=50 {
            let mut baseline = None;
            for reverse_execution in [false, true] {
                for reverse_merge in [false, true] {
                    let constraints = ConstraintSetBuilder::new();
                    let router = Router::with_constraints(&constraints);
                    let valid = request(&constraints, source, unary, EvidenceMode::Context);
                    let invalid = request(&constraints, source, nullary, EvidenceMode::Context);
                    let snapshot = run_with(
                        &db,
                        &env,
                        &router,
                        budget,
                        reverse_execution,
                        reverse_merge,
                        |router| async {
                            router.declare_relation(valid)?;
                            router.declare_relation(invalid)?;
                            router.declare_relation(invalid)?;
                            let valid_result = router.consumer_relation_demand(valid).await?;
                            let first = router.consumer_relation_demand(invalid).await?;
                            let second = router.consumer_relation_demand(invalid).await?;
                            assert!(valid_result.constraints.is_always_satisfied(&db, &env));
                            assert!(first.constraints.is_never_satisfied(&db, &env));
                            assert_eq!(
                                first.constraints.scheduling_key(&constraints),
                                second.constraints.scheduling_key(&constraints)
                            );
                            let first_context = first.context.expect("explanation was requested");
                            let second_context = second.context.expect("explanation was requested");
                            assert_eq!(first_context, second_context);
                            let first_view = first_context.instantiate();
                            let second_view = second_context.instantiate();
                            assert!(!first_view.take().is_empty());
                            assert!(first_view.is_empty());
                            assert!(!second_view.is_empty());
                            assert!(!second_context.instantiate().is_empty());
                            Ok::<_, Boundary>(())
                        },
                    )
                    .expect("fresh root router");
                    let mut counts = snapshot
                        .relation_polls
                        .values()
                        .copied()
                        .collect::<Vec<_>>();
                    counts.sort_unstable();
                    let observation = (snapshot.consumer, snapshot.consumer_polls, counts);
                    if let Some(baseline) = &baseline {
                        assert_eq!(&observation, baseline);
                    } else {
                        baseline = Some(observation);
                    }
                    if budget == 50 {
                        assert_eq!(snapshot.consumer, Some(Ok(())));
                        assert_eq!(snapshot.relation_polls.len(), 2);
                        assert!(snapshot.relation_polls.values().all(|polls| *polls == 2));
                    }
                }
            }
        }
    }

    #[test]
    fn scheduled_shared_relations_partition_evidence_and_builder_domains() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        let constraints = ConstraintSetBuilder::new();
        let foreign = ConstraintSetBuilder::new();
        let target = CallableType::single(&db, Signature::unknown());
        let source = Type::Callable(target);
        let silent = request(&constraints, source, target, EvidenceMode::None);
        let detailed = request(&constraints, source, target, EvidenceMode::Context);
        assert_ne!(silent.key(), detailed.key());
        let foreign_request = request(&foreign, source, target, EvidenceMode::None);
        assert_eq!(silent.key(), foreign_request.key());
        let router = Router::with_constraints(&constraints);
        assert_eq!(
            router.declare_relation(foreign_request),
            Err(Boundary::ConstraintDomain)
        );
        let snapshot = run_with(&db, &env, &router, 100, false, false, |router| async {
            router.declare_relation(silent)?;
            router.declare_relation(detailed)?;
            let silent_result = router.consumer_relation_demand(silent).await?;
            let detailed_result = router.consumer_relation_demand(detailed).await?;
            assert!(silent_result.context.is_none());
            assert!(detailed_result.context.is_some());
            Ok::<_, Boundary>(())
        })
        .expect("fresh root router");
        assert_eq!(snapshot.consumer, Some(Ok(())));
        assert_eq!(snapshot.relation_polls.len(), 2);
        assert!(matches!(
            run_with(&db, &env, &router, 100, false, false, |_| async {}),
            Err(Boundary::RootReuse)
        ));
    }

    #[test]
    fn scheduled_shared_relations_distinguish_assumptions() {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let string = KnownClass::Str.to_instance(&db, &env);
        let typevar = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let source = Type::Callable(CallableType::single(
            &db,
            Signature::new(Parameters::empty(), int),
        ));
        let target = CallableType::single(
            &db,
            Signature::new(Parameters::empty(), Type::TypeVar(typevar)),
        );
        for execution in [false, true] {
            for merge in [false, true] {
                for reverse_request in [false, true] {
                    let constraints = ConstraintSetBuilder::new();
                    let router = Router::with_constraints(&constraints);
                    let given_int = ConstraintSet::constrain_typevar_equivalence_bound(
                        &db,
                        &env,
                        &constraints,
                        typevar,
                        int,
                    );
                    let given_str = ConstraintSet::constrain_typevar_equivalence_bound(
                        &db,
                        &env,
                        &constraints,
                        typevar,
                        string,
                    );
                    let make = |given| {
                        RelationRequest::new(
                            &constraints,
                            source,
                            RelationGoal::CallableSource(target),
                            RelationContext {
                                relation: TypeRelation::SubtypingAssuming,
                                typevar_evaluation: TypeVarEvaluation::Eager,
                                inferable: TypeVarSet::None,
                                given,
                                policy: RootPolicy {
                                    materialize_bounds: true,
                                    expensive_checks: true,
                                    evidence: EvidenceMode::None,
                                },
                            },
                        )
                        .expect("same builder domain")
                    };
                    let valid = make(given_int);
                    let invalid = make(given_str);
                    assert_ne!(valid.key(), invalid.key());
                    let ordered = if reverse_request {
                        [invalid, valid]
                    } else {
                        [valid, invalid]
                    };
                    let snapshot =
                        run_with(&db, &env, &router, 100, execution, merge, |router| async {
                            for request in ordered {
                                router.declare_relation(request)?;
                            }
                            for request in ordered {
                                let _ = router.consumer_relation_demand(request).await?;
                            }
                            let valid_result = router.consumer_relation_demand(valid).await?;
                            let invalid_result = router.consumer_relation_demand(invalid).await?;
                            assert!(valid_result.constraints.is_always_satisfied(&db, &env));
                            // SubtypingAssuming returns an implication. Its result can hold
                            // outside the assumed world; under T = str this comparison fails.
                            assert!(!invalid_result.constraints.is_always_satisfied(&db, &env));
                            assert!(
                                given_str
                                    .and(&db, &constraints, || invalid_result.constraints)
                                    .is_never_satisfied(&db, &env)
                            );
                            Ok::<_, Boundary>(())
                        })
                        .expect("fresh root router");
                    assert_eq!(snapshot.consumer, Some(Ok(())));
                    assert_eq!(snapshot.relation_polls.len(), 2);
                    assert!(snapshot.relation_polls.values().all(|polls| *polls == 2));
                }
            }
        }
    }
}
