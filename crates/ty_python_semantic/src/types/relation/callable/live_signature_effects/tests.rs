use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashSet;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, RelationObservationSite, RelationObservations,
    TypeVarEvaluation,
};
use crate::types::relation_error::ErrorContextTree;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::signatures::effects::{legacy_inline, try_poll_immediate};
use crate::types::{ApplyTypeMappingVisitor, MaterializationKind, TypeMapping};

fn fixture() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/queued_signature.py",
            r#"from typing import TypeVar, overload

T = TypeVar("T")
def legacy(value: T) -> T: ...
def source[T](value: T) -> T: ...
def target(value: int) -> int: ...
def rejected(value: int) -> str: ...

@overload
def alternatives[T](value: T) -> T: ...
@overload
def alternatives(value: str) -> str: ...
def alternatives(value): ...
"#,
        )
        .build()
}

fn callable<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<CallableType<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/queued_signature.py")?,
        env.program(db),
    );
    let Type::FunctionLiteral(function) = global_symbol(db, file, name).place.expect_type() else {
        anyhow::bail!("missing fixture function {name}");
    };
    Ok(CallableType::new(
        db,
        function.signature(db),
        CallableTypeKind::Regular,
    ))
}

// These finite fixtures deliberately use the ordinary evaluator to produce each typed reply.
// This checks the queue contract; it does not install recursive execution under an attempt.
fn complete_finite_dependency<'db>(
    db: &'db dyn Db,
    request: LiveSignatureRequest<'_, 'db, '_>,
) -> Answer<()> {
    let checker = request.checker;
    match request.dependency {
        SignatureDependency::Relate {
            source,
            target,
            reply,
        } => reply.respond(Ok(checker.check_type_pair(db, source, target))),
        SignatureDependency::IsNever { constraints, reply } => reply.respond(Ok(legacy_inline(
            LegacyInlineEffects.is_never(db, &checker, constraints),
        ))),
        SignatureDependency::IsAlways { constraints, reply } => reply.respond(Ok(legacy_inline(
            LegacyInlineEffects.is_always(db, &checker, constraints),
        ))),
        SignatureDependency::ConstraintBound {
            kind,
            typevar,
            bound,
            reply,
        } => reply.respond(Ok(legacy_inline(
            LegacyInlineEffects.constraint_bound(db, &checker, kind, typevar, bound),
        ))),
        SignatureDependency::ReceiverConstraints { signature, reply } => reply.respond(Ok(
            legacy_inline(LegacyInlineEffects.receiver_constraints(db, &checker, &signature)),
        )),
        SignatureDependency::ReduceInferable {
            constraints,
            inferable,
            reply,
        } => reply.respond(Ok(legacy_inline(LegacyInlineEffects.reduce_inferable(
            db,
            &checker,
            constraints,
            inferable,
        )))),
        SignatureDependency::MaxFreshness {
            signature,
            context,
            reply,
        } => reply.respond(Ok(legacy_inline(
            LegacyInlineEffects.max_freshness(db, &checker, &signature, context),
        ))),
        SignatureDependency::SignatureTypevars { signature, reply } => reply.respond(Ok(
            legacy_inline(LegacyInlineEffects.signature_typevars(db, &checker, &signature)),
        )),
    }
}

fn refuse(request: LiveSignatureRequest<'_, '_, '_>) -> Answer<()> {
    match request.dependency {
        SignatureDependency::Relate { reply, .. }
        | SignatureDependency::ConstraintBound { reply, .. }
        | SignatureDependency::ReceiverConstraints { reply, .. }
        | SignatureDependency::ReduceInferable { reply, .. } => {
            reply.respond(Err(SignatureQueueError::Cancelled))
        }
        SignatureDependency::IsNever { reply, .. }
        | SignatureDependency::IsAlways { reply, .. } => {
            reply.respond(Err(SignatureQueueError::Cancelled))
        }
        SignatureDependency::MaxFreshness { reply, .. } => {
            reply.respond(Err(SignatureQueueError::Cancelled))
        }
        SignatureDependency::SignatureTypevars { reply, .. } => {
            reply.respond(Err(SignatureQueueError::Cancelled))
        }
    }
}

fn drive_finite<'state, 'db, 'c>(
    db: &'db dyn Db,
    effects: &LiveSignatureEffects<'state, 'db, 'c>,
    mut task: Pin<&mut impl Future<Output = Answer<ConstraintSet<'db, 'c>>>>,
    mut inspect: impl FnMut(&LiveSignatureRequest<'state, 'db, 'c>),
) -> (Answer<ConstraintSet<'db, 'c>>, usize) {
    let mut cx = Context::from_waker(Waker::noop());
    for count in 0..128 {
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(result) => {
                assert!(effects.take_request().is_none());
                return (result, count);
            }
            Poll::Pending => {
                let Some(request) = effects.take_request() else {
                    panic!("missing finite dependency");
                };
                inspect(&request);
                assert!(task.as_mut().poll(&mut cx).is_pending());
                assert!(effects.take_request().is_none());
                assert_eq!(complete_finite_dependency(db, request), Ok(()));
            }
        }
    }
    panic!("finite signature fixture did not finish");
}

macro_rules! checker_resources {
    ($db:ident, $env:ident, $constraints:ident, $relations:ident, $disjoint:ident, $signatures:ident, $mapping:ident, $checker:ident) => {
        let $env = $db.program_environment();
        let $constraints = ConstraintSetBuilder::new();
        let $relations = HasRelationToVisitor::default(&$constraints);
        let $disjoint = IsDisjointVisitor::default(&$constraints);
        let $signatures = SignatureRelationVisitor::default();
        let $mapping = ApplyTypeMappingVisitor::new(&$env);
        let $checker = TypeRelationChecker::constraint_set_assignability_with_context(
            &$env,
            &$constraints,
            &$relations,
            &$disjoint,
            &$signatures,
            &$mapping,
        );
    };
}

#[test]
fn live_signature_queue_preserves_generic_local_views_and_replies() -> anyhow::Result<()> {
    let db = fixture()?;
    let target = callable(&db, "target")?;
    let rejected = callable(&db, "rejected")?;
    for (name, context_enabled) in [("source", true), ("legacy", true), ("alternatives", false)] {
        checker_resources!(
            db,
            env,
            constraints,
            relations,
            disjoint,
            signatures,
            mapping,
            checker
        );
        // The mapping owner already has live state, and an observation sink is installed.
        // Both are excluded by the older fresh-root request's admission rules.
        mapping.transformer(&TypeMapping::Materialize(MaterializationKind::Top));
        let patterns = FxHashSet::default();
        let observations = RelationObservations {
            patterns: &patterns,
            results: RefCell::default(),
            site: Cell::new(RelationObservationSite::Argument(3)),
            inferred: RefCell::default(),
        };
        let root = TypeRelationChecker {
            observations: Some(&observations),
            perform_expensive_checks: false,
            ..checker
        };
        let source = callable(&db, name)?;
        let expected = root.check_callable_pair(&db, source, target);
        assert!(expected.is_always_satisfied(&db, &env));
        let effects = LiveSignatureEffects::default();
        let mut task = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
        let mut relation_count = 0;
        let mut saved_local = None;
        let (actual, count) = drive_finite(&db, &effects, task.as_mut(), |request| {
            let local = &request.checker;
            assert!(std::ptr::eq(
                local.constraints,
                std::ptr::from_ref(&constraints)
            ));
            assert!(std::ptr::eq(
                local.relation_visitor,
                std::ptr::from_ref(&relations)
            ));
            assert!(std::ptr::eq(
                local.disjointness_visitor,
                std::ptr::from_ref(&disjoint)
            ));
            assert!(std::ptr::eq(
                local.signature_relation_visitor,
                std::ptr::from_ref(&signatures)
            ));
            assert!(std::ptr::eq(
                local.materialization_visitor,
                std::ptr::from_ref(&mapping)
            ));
            assert!(std::ptr::eq(
                local.observations.unwrap(),
                std::ptr::from_ref(&observations)
            ));
            assert!(local.given.ownership_probe_same_set(root.given));
            assert_eq!(local.relation, root.relation);
            assert_eq!(local.typevar_evaluation, TypeVarEvaluation::Lazy);
            assert!(!local.perform_expensive_checks);
            if let SignatureDependency::Relate { source, target, .. } = &request.dependency {
                relation_count += 1;
                assert!(!signatures.is_empty());
                assert_eq!(local.is_context_collection_enabled(), context_enabled);
                let variable = match (source, target) {
                    (Type::TypeVar(variable), _) | (_, Type::TypeVar(variable)) => *variable,
                    _ => panic!(
                        "generic fixture relation must contain its signature-local type variable"
                    ),
                };
                assert!(variable.is_inferable(&db, local.inferable));
                assert_eq!(root.inferable, TypeVarSet::None);
                let first = local.check_type_pair(&db, *source, *target);
                let repeated = local.check_type_pair(&db, *source, *target);
                assert!(!first.is_trivially_always_satisfied());
                assert!(!first.is_trivially_never_satisfied());
                assert!(first.ownership_probe_same_set(repeated));
                saved_local = Some(local.clone());
            }
        });
        let actual = actual.map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert!(actual.ownership_probe_same_set(expected));
        assert_eq!(relation_count, 2);
        assert!(count > relation_count);
        drop(task);
        assert!(signatures.is_empty());

        // A retained local view shares the explanation root after the signature task has ended.
        let saved = saved_local.ok_or_else(|| anyhow::anyhow!("missing derived checker view"))?;
        assert!(
            root.report_context()
                .is_some_and(ErrorContextTree::is_empty)
        );
        let failed = saved.check_callable_pair(&db, rejected, target);
        assert!(failed.is_never_satisfied(&db, &env));
        assert_eq!(
            root.report_context()
                .is_some_and(|context| !context.is_empty()),
            context_enabled
        );
        assert!(root.is_context_collection_enabled());
        assert!(signatures.is_empty());
    }
    Ok(())
}

#[test]
fn live_signature_queue_cancellation_refusal_and_retry() -> anyhow::Result<()> {
    enum Stop {
        Queued,
        Held,
        Refused,
        Abandoned,
    }

    let db = fixture()?;
    let source = callable(&db, "source")?;
    let target = callable(&db, "target")?;
    checker_resources!(
        db,
        env,
        constraints,
        relations,
        disjoint,
        signatures,
        mapping,
        root
    );
    let effects = LiveSignatureEffects::default();
    let expected = root.check_callable_pair(&db, source, target);
    let mut complete = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
    let (result, requests) = drive_finite(&db, &effects, complete.as_mut(), |_| {});
    assert!(result.is_ok_and(|result| result.ownership_probe_same_set(expected)));
    drop(complete);
    for stop in 0..requests {
        for mode in [Stop::Queued, Stop::Held, Stop::Refused, Stop::Abandoned] {
            let mut task = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
            let mut cx = Context::from_waker(Waker::noop());
            for index in 0..=stop {
                assert!(task.as_mut().poll(&mut cx).is_pending());
                if index < stop {
                    let request = effects
                        .take_request()
                        .ok_or_else(|| anyhow::anyhow!("missing prefix request"))?;
                    assert_eq!(complete_finite_dependency(&db, request), Ok(()));
                }
            }
            match mode {
                Stop::Queued => drop(task),
                Stop::Held => {
                    let request = effects
                        .take_request()
                        .ok_or_else(|| anyhow::anyhow!("missing held request"))?;
                    drop(task);
                    assert_eq!(refuse(request), Err(SignatureQueueError::Cancelled));
                }
                Stop::Refused => {
                    let request = effects
                        .take_request()
                        .ok_or_else(|| anyhow::anyhow!("missing refused request"))?;
                    assert_eq!(refuse(request), Ok(()));
                    assert!(matches!(
                        task.as_mut().poll(&mut cx),
                        Poll::Ready(Err(SignatureQueueError::Cancelled))
                    ));
                    drop(task);
                }
                Stop::Abandoned => {
                    let request = effects
                        .take_request()
                        .ok_or_else(|| anyhow::anyhow!("missing abandoned request"))?;
                    drop(request);
                    assert!(matches!(
                        task.as_mut().poll(&mut cx),
                        Poll::Ready(Err(SignatureQueueError::Cancelled))
                    ));
                    drop(task);
                }
            }
            assert!(effects.take_request().is_none());
            assert!(signatures.is_empty());
            let mut retry = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
            let (result, _) = drive_finite(&db, &effects, retry.as_mut(), |_| {});
            assert!(result.is_ok_and(|result| result.ownership_probe_same_set(expected)));
            drop(retry);
            assert!(signatures.is_empty());
        }
    }
    Ok(())
}

#[test]
fn live_signature_queue_preserves_admission_and_alternative_short_circuit() -> anyhow::Result<()> {
    let db = fixture()?;
    let source = callable(&db, "source")?;
    let target = callable(&db, "target")?;
    let rejected = callable(&db, "rejected")?;
    checker_resources!(
        db,
        env,
        constraints,
        relations,
        disjoint,
        signatures,
        mapping,
        root
    );
    let effects = LiveSignatureEffects::default();
    let mut parent = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        assert!(parent.as_mut().poll(&mut cx).is_pending());
        let request = effects
            .take_request()
            .ok_or_else(|| anyhow::anyhow!("missing admission dependency"))?;
        if matches!(request.dependency, SignatureDependency::Relate { .. }) {
            assert!(!signatures.is_empty());
            let mut revisit = Box::pin(effects.compare_pair(&db, root.clone(), source, target));
            let mut child_relations = 0;
            let (result, _) = drive_finite(&db, &effects, revisit.as_mut(), |request| {
                child_relations += usize::from(matches!(
                    request.dependency,
                    SignatureDependency::Relate { .. }
                ));
            });
            assert!(result.is_ok_and(ConstraintSet::is_trivially_always_satisfied));
            assert_eq!(child_relations, 0);
            drop(revisit);
            assert!(!signatures.is_empty());
            drop(parent);
            assert!(signatures.is_empty());
            assert_eq!(refuse(request), Err(SignatureQueueError::Cancelled));
            break;
        }
        assert_eq!(complete_finite_dependency(&db, request), Ok(()));
    }

    let alternatives = CallableTypes::from_elements([rejected, source]);
    let expected = root.check_callables_vs_callable(&db, &alternatives, target);
    let mut task = Box::pin(effects.compare_callables(&db, root.clone(), alternatives, target));
    let (result, requests) = drive_finite(&db, &effects, task.as_mut(), |request| {
        assert!(matches!(
            request.dependency,
            SignatureDependency::Relate { .. }
        ));
    });
    assert_eq!(requests, 1);
    assert!(result.is_ok_and(|result| result.ownership_probe_same_set(expected)));
    drop(task);
    assert!(signatures.is_empty());
    Ok(())
}

#[test]
fn live_signature_queue_refuses_unsupported_dependencies_before_work() -> anyhow::Result<()> {
    let db = fixture()?;
    let source = callable(&db, "source")?;
    checker_resources!(
        db,
        env,
        constraints,
        relations,
        disjoint,
        signatures,
        mapping,
        root
    );
    let effects = LiveSignatureEffects::default();
    let before = crate::types::typevar::ownership_probe_nonce_counts();
    let result = effects.freshen_signature(&db, &root, &source.signatures(&db).overloads[0], 1);
    assert!(matches!(
        try_poll_immediate(result),
        Poll::Ready(Err(SignatureQueueError::Unsupported(
            UnsupportedSignatureDependency::Freshening
        )))
    ));
    assert_eq!(
        crate::types::typevar::ownership_probe_nonce_counts(),
        before
    );
    let result = effects.disjoint(&db, &root, Type::object(), Type::object());
    assert!(matches!(
        try_poll_immediate(result),
        Poll::Ready(Err(SignatureQueueError::Unsupported(
            UnsupportedSignatureDependency::Disjointness
        )))
    ));
    assert!(effects.take_request().is_none());
    assert!(signatures.is_empty());
    assert!(
        root.report_context()
            .is_some_and(ErrorContextTree::is_empty)
    );
    Ok(())
}

type CheckedReplyState = Rc<RefCell<ReplyState<u8>>>;

thread_local! {
    static CHECKED_REPLY: RefCell<Option<CheckedReplyState>> = const { RefCell::new(None) };
    static REPLY_WAKE_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct InspectReplyOnWake;

impl Wake for InspectReplyOnWake {
    fn wake(self: Arc<Self>) {
        CHECKED_REPLY.with_borrow(|state| {
            let Some(state) = state else {
                panic!("missing state for wake control");
            };
            // A waker can synchronously inspect the ready response before returning.
            assert!(state.borrow_mut().answer.is_some());
        });
        REPLY_WAKE_COUNT.set(REPLY_WAKE_COUNT.get() + 1);
    }
}

#[test]
fn live_signature_queue_releases_reply_borrow_before_waking() {
    for complete in [true, false] {
        let state = Rc::new(RefCell::new(ReplyState {
            answer: None,
            waker: Some(Waker::from(Arc::new(InspectReplyOnWake))),
        }));
        CHECKED_REPLY.set(Some(Rc::clone(&state)));
        REPLY_WAKE_COUNT.set(0);
        let reply = SignatureReply {
            state: Rc::clone(&state),
            active: Rc::new(Cell::new(true)),
        };
        if complete {
            assert_eq!(reply.respond(Ok(7)), Ok(()));
            assert_eq!(state.borrow().answer, Some(Ok(7)));
        } else {
            drop(reply);
            assert_eq!(
                state.borrow().answer,
                Some(Err(SignatureQueueError::Cancelled))
            );
        }
        assert_eq!(REPLY_WAKE_COUNT.get(), 1);
        CHECKED_REPLY.set(None);
    }
}
