//! Gradual callable tests exercise cold canonical inference and retained return continuations.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use salsa::Database;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, RunResult};
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::assert_no_active_attempt;
use ty_python_core::Program;

use super::super::*;
use crate::analysis::{
    AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
    expression_type_with_policy, prepare_file,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, observations,
};
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::infer::{
    InferExpression, expression_inference_ingredient, infer_expression_types,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    NestedReturn,
    CompletedReturn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Observe,
    Cancel,
}

#[derive(Debug, Default)]
struct Journal {
    events: RefCell<Vec<OwnershipEvent>>,
    builder: Cell<Option<usize>>,
    unpack: Cell<bool>,
    remaining: Cell<Option<usize>>,
    reached: Cell<bool>,
    retained: Cell<bool>,
}

thread_local! {
    static RECORDING: RefCell<Option<(Rc<Journal>, Stop, Action)>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct Recording(Rc<Journal>);

impl Recording {
    /// Records ownership in one root invocation and optionally cancels at its selected boundary.
    fn new(stop: Stop, action: Action) -> Self {
        let journal = Rc::new(Journal::default());
        RECORDING.with_borrow_mut(|recording| {
            assert!(recording.replace((journal.clone(), stop, action)).is_none());
        });
        Self(journal)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with_borrow_mut(|recording| *recording = None);
    }
}

/// Records the tested root's owner events without changing nested canonical-query observers.
pub(in crate::types::infer::builder::local) fn observe_invocation<'root, 'db, 'ast, 'expr, B>(
    mut invocation: LocalInvocation<'root, 'db, 'ast, 'expr, B>,
) -> LocalInvocation<'root, 'db, 'ast, 'expr, B> {
    RECORDING.with_borrow(|recording| {
        if let Some((journal, ..)) = recording
            && journal.builder.get().is_none()
        {
            journal
                .builder
                .set(Some(invocation.builders.root as *const _ as usize));
            journal.unpack.set(
                invocation
                    .builders
                    .root
                    .context
                    .inference_flags
                    .contains(InferenceFlags::IN_VALID_UNPACK_CONTEXT),
            );
            let journal = journal.clone();
            invocation.observe_ownership(Rc::new(move |event| {
                journal.events.borrow_mut().push(event)
            }));
        }
    });
    invocation
}

/// Observes a nested return callable while its parent's gradual parameters remain owned.
pub(in crate::types::infer::builder::local) fn before_step<B>(
    builders: &BuilderStore<'_, '_, '_>,
    owners: &LocalOwners<'_, '_, B>,
    owner: &callable_annotation::Active,
) {
    RECORDING.with_borrow(|recording| {
        let Some((journal, stop, action)) = recording else { return; };
        let Some(OwnerSlot::CallableAnnotation(payload)) = owners.slots.get(owner.0) else { return; };
        let builder = builders.builder(payload.builder);
        if journal.builder.get() != Some(builder as *const _ as usize) { return; }
        assert_eq!(builder.context.inference_flags.contains(InferenceFlags::IN_VALID_UNPACK_CONTEXT), journal.unpack.get());
        let retained = owners.slots[..owner.0].iter().any(|slot| {
            matches!(slot, OwnerSlot::CallableAnnotation(callable_annotation::Payload {
                phase: callable_annotation::Phase::Pending(callable_annotation::Pending::Return { parameters, .. }), ..
            }) if parameters.is_gradual())
        });
        if !retained { return; }
        journal.retained.set(true);
        let boundary = match &payload.phase {
            callable_annotation::Phase::Active(callable_annotation::State::Gradual { .. }) => Stop::NestedReturn,
            callable_annotation::Phase::Active(callable_annotation::State::Complete(_)) => Stop::CompletedReturn,
            _ => return,
        };
        if journal.reached.get() || *stop != boundary { return; }
        journal.reached.set(true);
        journal.remaining.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(builder.db()));
        if *action == Action::Cancel {
            builder.db().cancellation_token().cancel();
            builder.db().unwind_if_revision_cancelled();
        }
    });
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

/// Builds a cold database whose final declaration uses the supplied callable annotation.
fn fixture(annotation: &str) -> anyhow::Result<TestDb> {
    let mut db = setup_db();
    db.write_file("src/main.pyi", format!("from typing import Callable\nfrom collections.abc import Callable as AbcCallable\nclass Leaf: ...\nvalue: {annotation}\n"))?;
    Ok(db)
}

fn prepare(db: &TestDb) -> anyhow::Result<PreparedAnalysisFile<'_>> {
    prepare_file(db, system_path_to_file(db, "src/main.pyi")?)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn annotation<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> anyhow::Result<&'ast ast::Expr> {
    let Some(ast::Stmt::AnnAssign(assignment)) = prepared.parsed_module().syntax().body.last()
    else {
        anyhow::bail!("fixture must end with an annotation");
    };
    Ok(&assignment.annotation)
}

fn cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Both Callable spellings complete cold, retain gradual parameters, and store the ordinary
/// callable type on the argument tuple and ellipsis before reusing the canonical expression memo.
#[test]
fn callable_ellipsis_cold_results_and_storage_match_ordinary() -> anyhow::Result<()> {
    for source in ["Callable[..., Leaf]", "AbcCallable[..., Leaf]"] {
        let db = fixture(source)?;
        let prepared = prepare(&db)?;
        let node = annotation(&prepared)?;
        let expression = prepared.semantic_index().expression(node);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                expression_inference_ingredient(&db),
                InferExpression::Bare(expression).as_id()
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        observations::reset(None);
        let actual = expression_type_with_policy(&prepared, node.into(), &funded())
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let AnalysisOutcome::Complete(ty @ Type::Callable(callable)) = actual else {
            anyhow::bail!("{source}: {actual:?}");
        };
        cleanup();
        assert_eq!(callable.signatures(&db).iter().count(), 1);
        assert!(
            callable
                .signatures(&db)
                .iter()
                .all(|signature| signature.parameters().is_gradual())
        );
        let canonical = infer_expression_types(&db, expression, TypeContext::default());
        let ast::Expr::Subscript(subscript) = node else {
            anyhow::bail!("expected callable subscript");
        };
        let ast::Expr::Tuple(arguments) = &*subscript.slice else {
            anyhow::bail!("expected callable arguments");
        };
        assert_eq!(canonical.expression_type(node), ty);
        assert_eq!(
            canonical.try_expression_type(&subscript.slice),
            Some(ty)
        );
        assert_eq!(canonical.try_expression_type(&arguments.elts[0]), Some(ty));
        let file = prepared.program_file();
        let env = ProgramEnvironment::from_file(file);
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expression, TypeContext::default()),
            file.file(&db),
            file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_expression();
        assert_eq!(canonical, &ordinary);
        let mut events = db.clone();
        events.take_salsa_events();
        assert_eq!(
            expression_type_with_policy(&prepared, node.into(), &funded())
                .map_err(|error| anyhow::anyhow!("{error:?}"))?,
            actual
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            Some(InferExpression::Bare(expression).as_id()),
            &events.take_salsa_events(),
        );
        cleanup();
    }
    Ok(())
}

/// Runs the existing local type-expression entry point with a caller-owned builder.
struct Request<'builder, 'db, 'ast> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    expression: &'ast ast::Expr,
}

impl<'db> MemberOperation<'db> for Request<'_, 'db, '_> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Type<'db>>
    where
        'db: 'run,
    {
        source::type_expression(
            self.builder,
            self.expression,
            TypeExpressionMode::Scoped,
            &SourceEffects::new(access, program),
        )
        .await
    }
}

/// Keeps the root builder accessible after interruption so restoration is checked directly.
fn infer<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> anyhow::Result<Result<AnalysisOutcome<Type<'db>>, salsa::Cancelled>> {
    let node = annotation(prepared)?;
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let expression = prepared.semantic_index().expression(node);
    let mut builder = TypeInferenceBuilder::new(
        db,
        &env,
        InferenceRegion::Expression(expression, TypeContext::default()),
        file.file(db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    );
    let initial = function_annotation_state(&builder);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            prepared,
            Request {
                builder: &mut builder,
                expression: node,
            },
            policy,
        )
    }));
    assert_eq!(function_annotation_state(&builder), initial);
    assert!(builder.context.finish().is_empty());
    match result {
        Ok(result) => result.map(Ok).map_err(|error| anyhow::anyhow!("{error:?}")),
        Err(cancelled) => Ok(Err(cancelled)),
    }
}

/// Every observed callable payload retires once before the root builder is restored.
fn assert_retired(journal: &Journal) {
    let events = journal.events.borrow();
    let created: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            OwnershipEvent::Created {
                kind: OwnerKind::CallableAnnotation,
                identity,
                ..
            } => Some(*identity),
            _ => None,
        })
        .collect();
    assert!(created.len() >= 2);
    for identity in &created {
        assert_eq!(events.iter().filter(|event| matches!(event, OwnershipEvent::PayloadRetired { identity: retired } if retired == identity)).count(), 1);
    }
    let retired: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            OwnershipEvent::PayloadRetired { identity } if created.contains(identity) => {
                Some(*identity)
            }
            _ => None,
        })
        .collect();
    assert_eq!(retired, created.into_iter().rev().collect::<Vec<_>>());
    let last = events
        .iter()
        .rposition(|event| matches!(event, OwnershipEvent::PayloadRetired { .. }));
    let restored = events
        .iter()
        .rposition(|event| matches!(event, OwnershipEvent::RootRestored));
    assert!(matches!((last, restored), (Some(last), Some(restored)) if last < restored));
}

/// Work refusal and cancellation drain gradual parameters retained across a nested return,
/// restore the builder's flags, and permit a funded retry in the same revision.
#[test]
fn callable_ellipsis_retained_returns_drain_and_retry() -> anyhow::Result<()> {
    for stop in [Stop::NestedReturn, Stop::CompletedReturn] {
        let measured_db = fixture("Callable[..., Callable[..., Leaf]]")?;
        let measured = prepare(&measured_db)?;
        observations::reset(None);
        let recording = Recording::new(stop, Action::Observe);
        assert!(matches!(
            infer(&measured_db, &measured, &funded())?,
            Ok(AnalysisOutcome::Complete(_))
        ));
        let Some(remaining) = recording.0.remaining.get() else {
            anyhow::bail!("missing {stop:?} boundary");
        };
        let work = funded().semantic_work_limit - remaining;
        drop(recording);
        cleanup();
        for action in [Action::Observe, Action::Cancel] {
            let db = fixture("Callable[..., Callable[..., Leaf]]")?;
            let prepared = prepare(&db)?;
            let revision = salsa::plumbing::current_revision(&db);
            observations::reset(None);
            let recording = Recording::new(stop, action);
            let policy = match action {
                Action::Observe => AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                },
                Action::Cancel => funded(),
            };
            let actual = infer(&db, &prepared, &policy)?;
            match action {
                Action::Observe => assert_eq!(
                    actual.map_err(|error| anyhow::anyhow!("{error:?}"))?,
                    AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: ()
                    }
                ),
                Action::Cancel => {
                    assert!(matches!(actual, Err(salsa::Cancelled::Local)), "{actual:?}")
                }
            }
            assert!(recording.0.reached.get());
            assert!(recording.0.retained.get());
            assert_retired(&recording.0);
            drop(recording);
            cleanup();
            observations::reset(None);
            assert!(matches!(
                infer(&db, &prepared, &funded())?,
                Ok(AnalysisOutcome::Complete(_))
            ));
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            cleanup();
        }
    }
    Ok(())
}

/// Measures a real allocation boundary without completing earlier source inference first.
fn reaches_completed_return(bytes: usize) -> anyhow::Result<bool> {
    let db = fixture("Callable[..., Callable[..., Leaf]]")?;
    let prepared = prepare(&db)?;
    observations::reset(None);
    let recording = Recording::new(Stop::CompletedReturn, Action::Observe);
    let result = infer(
        &db,
        &prepared,
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    )?
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert!(
        matches!(
            result,
            AnalysisOutcome::Complete(_)
                | AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                }
        ),
        "{result:?}"
    );
    cleanup();
    Ok(recording.0.reached.get())
}

/// Byte refusal after a nested return completes drains both callable owners and allows retry.
#[test]
fn callable_ellipsis_byte_refusal_drains_and_retries() -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_completed_return(high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_completed_return(middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture("Callable[..., Callable[..., Leaf]]")?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::new(Stop::CompletedReturn, Action::Observe);
    assert_eq!(
        infer(
            &db,
            &prepared,
            &AnalysisPolicy {
                requested_bytes_limit: high,
                ..funded()
            }
        )?
        .map_err(|error| anyhow::anyhow!("{error:?}"))?,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }
    );
    assert!(recording.0.reached.get());
    assert!(recording.0.retained.get());
    assert_retired(&recording.0);
    drop(recording);
    cleanup();
    observations::reset(None);
    assert!(matches!(
        infer(&db, &prepared, &funded())?,
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}
