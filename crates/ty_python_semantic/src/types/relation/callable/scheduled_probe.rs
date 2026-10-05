//! Scratch adapter for a real relation visit awaiting root-scheduled callable conversion.
//!
//! Signature comparison still uses the synchronous production continuation. Its nested type
//! relations and inference are not supervised merely because conversion has been scheduled.

use std::cell::Cell;
use std::future::Future;

use super::{CallableRelationStep, TypeRelationChecker};
use crate::types::callable::scheduled_probe::{Boundary, Router, run_with};
use crate::types::callable::{CallableConversionRequest, CallableType, CallableTypes};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::{
    ApplyTypeMappingVisitor, KnownClass, KnownInstanceType, Parameter, Parameters, Signature, Type,
};
use crate::{Db, ProgramEnvironment};

pub(crate) async fn check_callable_source<'checker, 'a, 'c, 'db, E, F>(
    db: &'db dyn Db,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    source: Type<'db>,
    target: CallableType<'db>,
    convert: impl FnOnce(CallableConversionRequest<'db>) -> F,
) -> Result<ConstraintSet<'db, 'c>, E>
where
    F: Future<Output = Result<Option<CallableTypes<'db>>, E>>,
{
    match CallableRelationStep::start(db, checker, source, target) {
        CallableRelationStep::Complete(result) => Ok(result),
        CallableRelationStep::Convert(pending) => {
            // Incomplete work and cancellation drop this visit without publishing a relation.
            let callables = convert(pending.request).await?;
            Ok(pending.resume(db, callables))
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CaseOutcome {
    consumer: Option<Result<bool, Boundary>>,
    consumer_polls: usize,
    conversion_requests: usize,
    context_present: bool,
}

fn run_relation_case<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: CallableType<'db>,
    budget: usize,
    reverse_execution: bool,
    reverse_merge: bool,
) -> CaseOutcome {
    let constraints = ConstraintSetBuilder::new();
    let relation_visitor = HasRelationToVisitor::default(&constraints);
    let disjointness_visitor = IsDisjointVisitor::default(&constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mapping_visitor = ApplyTypeMappingVisitor::new(env);
    let checker = TypeRelationChecker::assignability_with_context(
        env,
        &constraints,
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    );
    let conversion_requests = Cell::new(0);
    let router = Router::default();
    let snapshot = run_with(
        db,
        env,
        &router,
        budget,
        reverse_execution,
        reverse_merge,
        |router| {
            check_callable_source(db, &checker, source, target, |request| {
                conversion_requests.set(conversion_requests.get() + 1);
                assert_eq!(relation_visitor.ownership_probe_counts(), (1, 0));
                router.consumer_demand(request)
            })
        },
    )
    .expect("fresh root router");
    let completed = matches!(snapshot.consumer, Some(Ok(_)));
    // The driver has dropped its consumer task, including any unfinished relation scope.
    assert_eq!(
        relation_visitor.ownership_probe_counts(),
        (0, usize::from(completed))
    );
    let consumer = snapshot.consumer.map(|result| {
        result.map(|result| {
            let satisfied = result.is_always_satisfied(db, env);
            assert_eq!(result.is_never_satisfied(db, env), !satisfied);
            satisfied
        })
    });
    let context_present = checker
        .report_context()
        .is_some_and(|context| !context.is_empty());
    CaseOutcome {
        consumer,
        consumer_polls: snapshot.consumer_polls,
        conversion_requests: conversion_requests.get(),
        context_present,
    }
}

fn wrap<'db>(db: &'db dyn Db, inner: Type<'db>) -> Type<'db> {
    Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
        db,
        inner,
        MethodWrapperKind::Staticmethod,
    )))
}

#[test]
fn scheduled_relation_uses_real_conversion_and_resume() {
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
    let no_arguments = CallableType::single(&db, Signature::new(Parameters::standard([]), int));
    let source = wrap(&db, wrap(&db, Type::Callable(unary)));
    for (target, expected) in [(unary, true), (no_arguments, false)] {
        let full = run_relation_case(&db, &env, source, target, 1000, false, false);
        assert_eq!(full.consumer, Some(Ok(expected)));
        assert_eq!(full.consumer_polls, 2);
        assert_eq!(full.conversion_requests, 1);
        assert_eq!(full.context_present, !expected);

        let mut cancelled_started = 0;
        for budget in 0..=32 {
            let baseline = run_relation_case(&db, &env, source, target, budget, false, false);
            if baseline.consumer.is_none() && baseline.conversion_requests == 1 {
                cancelled_started += 1;
                assert_eq!(baseline.consumer_polls, 1);
                assert!(!baseline.context_present);
            }
            for execution in [false, true] {
                for merge in [false, true] {
                    assert_eq!(
                        baseline,
                        run_relation_case(&db, &env, source, target, budget, execution, merge)
                    );
                }
            }
        }
        assert!(cancelled_started > 0);
    }
}

#[test]
fn scheduled_relation_propagates_real_incomplete_without_finishing_visit() {
    let db = crate::db::tests::setup_db();
    let env = db.program_environment();
    let target = CallableType::single(&db, Signature::unknown());
    let source = wrap(&db, wrap(&db, Type::int_literal(0)));
    let full = run_relation_case(&db, &env, source, target, 1000, false, false);
    assert_eq!(full.consumer, Some(Err(Boundary::SemanticOperation)));
    assert_eq!(full.consumer_polls, 2);
    assert_eq!(full.conversion_requests, 1);
    assert!(!full.context_present);
    for budget in 0..=32 {
        let baseline = run_relation_case(&db, &env, source, target, budget, false, false);
        assert!(!matches!(baseline.consumer, Some(Ok(_))));
        for execution in [false, true] {
            for merge in [false, true] {
                assert_eq!(
                    baseline,
                    run_relation_case(&db, &env, source, target, budget, execution, merge)
                );
            }
        }
    }
}
