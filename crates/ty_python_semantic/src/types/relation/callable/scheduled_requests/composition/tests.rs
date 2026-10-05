use std::panic::AssertUnwindSafe;

use salsa::Database;

use super::*;
use crate::types::callable::CallableTypeKind;
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::relation::callable::scheduled_requests::{EvidenceMode, RootPolicy};
use crate::types::relation::{TypeRelation, TypeVarEvaluation};
use crate::types::relation_error::ErrorContextTree;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::signatures::CallableSignature;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    InternedType, KnownBoundMethodType, KnownClass, KnownInstanceType, Parameters, Signature,
    UnionType,
};

const COMPLETE: usize = 20_000;
const ORDERS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];

fn policy(allowance: usize, order: (bool, bool)) -> ComposedRelationPolicy {
    ComposedRelationPolicy {
        allowance,
        reverse_execution: order.0,
        reverse_merge: order.1,
    }
}

fn wrap<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
        db,
        ty,
        MethodWrapperKind::Staticmethod,
    )))
}

fn regularize<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(InternedType::new(db, ty)))
}

fn ordered_union<'db>(db: &'db dyn Db, elements: [Type<'db>; 2]) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        Vec::from(elements).into_boxed_slice(),
        RecursivelyDefined::No,
    ))
}

fn nullary<'db>(db: &'db dyn Db, result: Type<'db>) -> CallableType<'db> {
    CallableType::single(db, Signature::new(Parameters::empty(), result))
}

fn request<'db, 'c>(
    constraints: &'c ConstraintSetBuilder<'db>,
    source: Type<'db>,
    target: CallableType<'db>,
) -> ComposedCallableRequest<'db, 'c> {
    ComposedCallableRequest::new(
        constraints,
        source,
        target,
        RelationContext {
            relation: TypeRelation::Assignability,
            typevar_evaluation: TypeVarEvaluation::Eager,
            inferable: TypeVarSet::None,
            given: ConstraintSet::from_bool(constraints, false),
            policy: RootPolicy {
                materialize_bounds: true,
                expensive_checks: true,
                evidence: EvidenceMode::Context,
            },
        },
    )
    .expect("fresh constraint domain")
}

fn assert_allowance(outcome: &CallableRelationOutcome<'_, '_>, total: usize) {
    let semantic = total.saturating_sub(2) / 6;
    assert!(outcome.semantic_work <= semantic);
    assert!(outcome.transport_work <= total - semantic);
    assert_eq!(outcome.work, outcome.semantic_work + outcome.transport_work);
    assert!(outcome.work <= total);
    if total >= 2 {
        assert!(outcome.transport_work <= 5 * outcome.semantic_work + 2);
        assert!(outcome.unvisited.is_empty());
    } else {
        assert_eq!(outcome.verdict, CallableRelationVerdict::Incomplete);
        assert!(outcome.completed.is_empty());
        assert!(outcome.unresolved.is_empty());
        assert!(matches!(
            outcome.unvisited.as_slice(),
            [RelationFrontier::Inspect(_)]
        ));
    }
}

#[test]
fn composed_relation_retains_rejection_beside_unsupported_conversion() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let known = wrap(&db, Type::Callable(nullary(&db, Type::int_literal(1))));
    let unsupported = KnownClass::Int.to_instance(&db, &env);
    let expected = known
        .assignability_error_context(&db, &env, Type::Callable(target))
        .freeze();
    for elements in [[known, unsupported], [unsupported, known]] {
        for order in ORDERS {
            let constraints = ConstraintSetBuilder::new();
            let router = Router::with_constraints(&constraints);
            let outcome = request(&constraints, ordered_union(&db, elements), target)
                .run(&db, &env, &router, policy(COMPLETE, order))
                .expect("composed root");
            assert_allowance(&outcome, COMPLETE);
            assert_eq!(outcome.verdict, CallableRelationVerdict::Incompatible);
            assert_eq!(outcome.completed.len(), 1);
            assert_eq!(outcome.completed[0].request.comparison_source(), known);
            assert!(
                outcome.completed[0]
                    .output
                    .constraints
                    .is_trivially_never_satisfied()
            );
            assert_eq!(outcome.completed[0].output.context, Some(expected.clone()));
            assert_eq!(outcome.unresolved.len(), 1);
            assert_eq!(
                outcome.unresolved[0].request.comparison_source(),
                unsupported
            );
            assert_eq!(
                outcome.unresolved[0].reason,
                UnresolvedReason::Boundary(Boundary::SemanticOperation)
            );
        }
    }
}

#[test]
fn composed_relation_total_allowance_preserves_completed_evidence() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let known = wrap(&db, Type::Callable(nullary(&db, Type::int_literal(1))));
    let mut slow = Type::Callable(target);
    for _ in 0..12 {
        slow = wrap(&db, slow);
    }
    // In particular, the earlier slow alternative grows a longer traversal prefix as the
    // scheduler receives more work. Export must still retain the later known rejection.
    for elements in [[slow, known], [known, slow]] {
        let source = ordered_union(&db, elements);
        let mut saw_partial_rejection = false;
        let mut prior_completed = Vec::new();
        for total in (0..=4096).chain([COMPLETE]) {
            let mut baseline = None;
            for order in ORDERS {
                let constraints = ConstraintSetBuilder::new();
                let router = Router::with_constraints(&constraints);
                let outcome = request(&constraints, source, target)
                    .run(&db, &env, &router, policy(total, order))
                    .expect("composed root");
                assert_allowance(&outcome, total);
                let completed = outcome
                    .completed
                    .iter()
                    .map(|leaf| {
                        (
                            leaf.request.comparison_source(),
                            leaf.output.constraints.is_trivially_never_satisfied(),
                            leaf.output.context.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                let observation = (
                    outcome.verdict,
                    completed.clone(),
                    outcome
                        .unresolved
                        .iter()
                        .map(|leaf| (leaf.request.comparison_source(), leaf.reason))
                        .collect::<Vec<_>>(),
                    outcome.unvisited.len(),
                );
                if let Some(baseline) = &baseline {
                    assert_eq!(&observation, baseline);
                } else {
                    for previous in &prior_completed {
                        assert!(
                            completed.contains(previous),
                            "lost completed comparison at total allowance {total}"
                        );
                    }
                    prior_completed = completed;
                    baseline = Some(observation);
                }
                if outcome.verdict == CallableRelationVerdict::Incompatible
                    && !outcome.unresolved.is_empty()
                {
                    assert!(
                        outcome
                            .unresolved
                            .iter()
                            .any(|leaf| leaf.request.comparison_source() == slow
                                && leaf.reason == UnresolvedReason::Pending)
                    );
                    saw_partial_rejection = true;
                }
                if total == COMPLETE {
                    assert_eq!(outcome.verdict, CallableRelationVerdict::Incompatible);
                    assert_eq!(outcome.completed.len(), 2);
                    assert!(outcome.unresolved.is_empty());
                }
            }
        }
        assert!(saw_partial_rejection);
    }
}

#[test]
fn composed_relation_low_level_transport_limit_keeps_an_explicit_frontier() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let source = ordered_union(
        &db,
        [
            Type::Callable(target),
            wrap(&db, Type::Callable(nullary(&db, Type::Never))),
        ],
    );
    let constraints = ConstraintSetBuilder::new();
    let router = Router::with_constraints(&constraints);
    let request = request(&constraints, source, target);
    request
        .run(&db, &env, &router, policy(COMPLETE, ORDERS[0]))
        .expect("composed root");
    // Independent transport allowances are an internal reader control, outside the public
    // execution policy that reserves enough traversal before scheduling.
    let mut completed = 0;
    for allowance in 0..=16 {
        let outcome = request
            .request
            .callable_outcome(&router, allowance)
            .expect("composed root");
        assert!(outcome.transport_work <= allowance);
        assert!(outcome.completed.len() >= completed);
        completed = outcome.completed.len();
        if outcome.unvisited.is_empty() {
            assert_eq!(outcome.verdict, CallableRelationVerdict::Compatible);
            assert_eq!(outcome.completed.len(), 2);
        } else {
            assert_eq!(outcome.verdict, CallableRelationVerdict::Incomplete);
        }
    }
    assert_eq!(completed, 2);
}

#[test]
fn composed_relation_children_preserve_incoming_diagnostic_seed() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let source = Type::Callable(nullary(&db, Type::int_literal(1)));
    let unsupported = KnownClass::Int.to_instance(&db, &env);
    for enabled in [false, true] {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let seed = ErrorContextTree::from_context(
            ErrorContext::NotAssignableToNOtherUnionElements { n: 3 },
            TypeRelation::Assignability,
        );
        seed.set_enabled(enabled);
        let frozen = seed.snapshot();
        let seed_id = router
            .intern_diagnostic_seed(frozen.clone())
            .expect("own seed");
        let root = request(
            &constraints,
            ordered_union(&db, [source, unsupported]),
            target,
        )
        .with_seed(seed_id);
        let outcome = root
            .run(&db, &env, &router, policy(COMPLETE, ORDERS[0]))
            .expect("composed root");
        assert_allowance(&outcome, COMPLETE);
        assert_eq!(outcome.completed.len(), 1);
        assert_eq!(outcome.completed[0].request.request.key.seed, Some(seed_id));
        assert_eq!(
            outcome.unresolved[0].request.request.key.seed,
            Some(seed_id)
        );
        assert_eq!(seed.snapshot(), frozen);
        if !enabled {
            assert_eq!(outcome.completed[0].output.context, Some(frozen));
        }
    }
}

#[test]
fn composed_relation_sees_alternatives_under_admitted_wrappers() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let known = Type::Callable(nullary(&db, Type::int_literal(1)));
    let unsupported = KnownClass::Int.to_instance(&db, &env);
    for elements in [[known, unsupported], [unsupported, known]] {
        let source = ordered_union(&db, elements);
        for wrapped in [wrap(&db, source), regularize(&db, source)] {
            for order in ORDERS {
                let constraints = ConstraintSetBuilder::new();
                let router = Router::with_constraints(&constraints);
                let outcome = request(&constraints, wrapped, target)
                    .run(&db, &env, &router, policy(COMPLETE, order))
                    .expect("composed root");
                assert_allowance(&outcome, COMPLETE);
                assert_eq!(outcome.verdict, CallableRelationVerdict::Incompatible);
                assert_eq!(outcome.completed.len(), 1);
                assert_eq!(outcome.completed[0].request.source(), known);
                assert_eq!(outcome.completed[0].request.comparison_source(), known);
                assert_eq!(outcome.unresolved.len(), 1);
                assert_eq!(
                    outcome.unresolved[0].request.comparison_source(),
                    unsupported
                );
                let expected = known
                    .assignability_error_context(&db, &env, Type::Callable(target))
                    .freeze();
                assert_eq!(outcome.completed[0].output.context, Some(expected));
            }
        }
    }
}

#[test]
fn composed_relation_preserves_overload_grouping_and_regularization() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let overloaded = CallableType::new(
        &db,
        CallableSignature::from_overloads([
            Signature::new(Parameters::empty(), Type::int_literal(1)),
            Signature::new(Parameters::empty(), Type::int_literal(2)),
        ]),
        CallableTypeKind::Regular,
    );
    let function = CallableType::function_like(
        &db,
        Signature::new(Parameters::empty(), Type::int_literal(2)),
    );
    for (source, target, compatible) in [
        (wrap(&db, Type::Callable(overloaded)), target, true),
        (Type::Callable(function), function, true),
        (regularize(&db, Type::Callable(function)), function, false),
    ] {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let outcome = request(&constraints, source, target)
            .run(&db, &env, &router, policy(COMPLETE, ORDERS[0]))
            .expect("composed root");
        assert_allowance(&outcome, COMPLETE);
        assert_eq!(outcome.completed.len(), 1);
        assert!(outcome.unresolved.is_empty());
        assert_eq!(
            outcome.verdict,
            if compatible {
                CallableRelationVerdict::Compatible
            } else {
                CallableRelationVerdict::Incompatible
            }
        );
        assert_eq!(
            source.is_assignable_to(&db, &env, Type::Callable(target)),
            compatible
        );
        if !compatible {
            let expected = source
                .assignability_error_context(&db, &env, Type::Callable(target))
                .freeze();
            assert_eq!(outcome.completed[0].output.context, Some(expected));
        }
    }
}

#[test]
fn composed_relation_preserves_context_and_constraint_domain() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let source = ordered_union(
        &db,
        [
            Type::Callable(target),
            Type::Callable(nullary(&db, Type::Never)),
        ],
    );
    let constraints = ConstraintSetBuilder::new();
    let foreign = ConstraintSetBuilder::new();
    let context = RelationContext {
        relation: TypeRelation::Subtyping,
        typevar_evaluation: TypeVarEvaluation::Lazy,
        inferable: TypeVarSet::None,
        given: ConstraintSet::from_bool(&constraints, true),
        policy: RootPolicy {
            materialize_bounds: false,
            expensive_checks: false,
            evidence: EvidenceMode::None,
        },
    };
    assert!(matches!(
        ComposedCallableRequest::new(&foreign, source, target, context),
        Err(Boundary::ConstraintDomain)
    ));
    let request =
        ComposedCallableRequest::new(&constraints, source, target, context).expect("own domain");
    assert!(matches!(
        request.run(
            &db,
            &env,
            &Router::with_constraints(&foreign),
            policy(COMPLETE, ORDERS[0])
        ),
        Err(Boundary::ConstraintDomain)
    ));
    let router = Router::with_constraints(&constraints);
    let outcome = request
        .run(&db, &env, &router, policy(COMPLETE, ORDERS[0]))
        .expect("own domain");
    assert_allowance(&outcome, COMPLETE);
    assert_eq!(outcome.verdict, CallableRelationVerdict::Compatible);
    assert_eq!(outcome.completed.len(), 2);
    for completed in outcome.completed {
        let child = completed.request.request;
        let root = request.request;
        assert_eq!(child.key.relation, root.key.relation);
        assert_eq!(child.key.typevar_evaluation, root.key.typevar_evaluation);
        assert_eq!(child.key.inferable, root.key.inferable);
        assert_eq!(child.key.given, root.key.given);
        assert_eq!(child.key.policy, root.key.policy);
        assert_eq!(child.key.seed, root.key.seed);
        assert!(completed.output.context.is_none());
    }
}

#[test]
fn composed_relation_cancellation_does_not_return_an_aggregate_answer() {
    for order in ORDERS {
        let db = crate::db::tests::setup_db();
        let env = db.program_environment();
        let target = nullary(&db, Type::int_literal(2));
        let known = Type::Callable(nullary(&db, Type::int_literal(1)));
        let mut slow = Type::Callable(target);
        for _ in 0..12 {
            slow = wrap(&db, slow);
        }
        let source = ordered_union(&db, [known, slow]);
        let mut stop = None;
        for total in (0..=4096).step_by(8) {
            let constraints = ConstraintSetBuilder::new();
            let router = Router::with_constraints(&constraints);
            let outcome = request(&constraints, source, target)
                .run(&db, &env, &router, policy(total, order))
                .expect("composed root");
            if outcome.verdict == CallableRelationVerdict::Incompatible
                && !outcome.unresolved.is_empty()
            {
                stop = Some(outcome.semantic_work);
                break;
            }
        }
        let stop = stop.expect("known rejection completes while wrapped sibling is pending");
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let root = request(&constraints, source, target);
        router.cancel_at(stop, db.cancellation_token());
        let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            root.run(&db, &env, &router, policy(COMPLETE, order))
        }));
        assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    }
}

#[test]
fn composed_relation_public_budget_covers_shared_children() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let source = ordered_union(&db, [Type::Callable(target), Type::Callable(target)]);
    for total in 0..=1024 {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let outcome = request(&constraints, source, target)
            .run(&db, &env, &router, policy(total, ORDERS[0]))
            .expect("composed root");
        assert_allowance(&outcome, total);
        if outcome.verdict == CallableRelationVerdict::Compatible {
            assert_eq!(outcome.completed.len(), 1);
            assert!(outcome.unresolved.is_empty());
        }
        if total == 1024 {
            assert_eq!(outcome.verdict, CallableRelationVerdict::Compatible);
        }
    }
}

#[test]
fn composed_relation_raw_requests_cannot_escape_as_aggregate_answers() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let constraints = ConstraintSetBuilder::new();
    let router = Router::with_constraints(&constraints);
    let root = request(&constraints, Type::Callable(target), target);
    let snapshot = run_with(&db, &env, &router, 16, false, false, |router| async {
        router
            .consumer_relation_demand(root.request)
            .await
            .map(|_| ())
    })
    .expect("fresh root");
    assert_eq!(snapshot.consumer, Some(Err(Boundary::RelationContext)));
}

#[test]
fn composed_relation_low_level_cycles_remain_explicit() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = nullary(&db, Type::int_literal(2));
    let constraints = ConstraintSetBuilder::new();
    let router = Router::with_constraints(&constraints);
    let cyclic = request(&constraints, Type::Callable(target), target);
    // This fixture only exercises the reader boundary; admitted conversion shapes are acyclic.
    run_with(&db, &env, &router, 1, false, false, |router| async {
        router.publish_relation_composition(cyclic.request.key(), Rc::from([cyclic.request]))?;
        std::future::pending::<()>().await;
        Ok::<_, Boundary>(())
    })
    .expect("fresh root");
    let outcome = cyclic
        .request
        .callable_outcome(&router, 7)
        .expect("composed root");
    assert_eq!(outcome.transport_work, 7);
    assert_eq!(outcome.verdict, CallableRelationVerdict::Incomplete);
    assert!(outcome.completed.is_empty());
    assert_eq!(outcome.unresolved.len(), 1);
    assert_eq!(outcome.unresolved[0].reason, UnresolvedReason::Cycle);
    assert!(outcome.unvisited.is_empty());
}
