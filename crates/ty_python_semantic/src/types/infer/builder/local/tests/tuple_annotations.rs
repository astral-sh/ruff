//! Tuple annotation tests exercise canonical results and cancellation with retained local state.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use salsa::Database;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::assert_no_active_attempt;

use super::super::*;
use crate::analysis::{
    AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
    expression_type_with_policy, prepare_file,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::infer::builder::source_definition::controlled::observations;
use crate::types::infer::{
    InferExpression, expression_inference_ingredient, infer_expression_types,
};
use crate::types::tuple::TupleSpecBuilder;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    RetainedElement,
    NestedChild,
}

#[derive(Default)]
struct Journal {
    events: RefCell<Vec<OwnershipEvent>>,
    remaining: Cell<Option<usize>>,
    reached: Cell<bool>,
    retained: Cell<bool>,
}

thread_local! {
    static RECORDING: RefCell<Option<(Rc<Journal>, Stop, bool)>> = const { RefCell::new(None) };
}

struct Recording(Rc<Journal>);

impl Recording {
    fn new(stop: Stop, cancel: bool) -> Self {
        let journal = Rc::new(Journal::default());
        RECORDING.with_borrow_mut(|recording| {
            assert!(recording.replace((journal.clone(), stop, cancel)).is_none());
        });
        Self(journal)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with_borrow_mut(|recording| *recording = None);
    }
}

pub(in crate::types::infer::builder::local) fn observe_invocation<'root, 'db, 'ast, 'expr, B>(
    mut invocation: LocalInvocation<'root, 'db, 'ast, 'expr, B>,
) -> LocalInvocation<'root, 'db, 'ast, 'expr, B> {
    RECORDING.with_borrow(|recording| {
        if let Some((journal, ..)) = recording {
            let journal = journal.clone();
            invocation.observe_ownership(Rc::new(move |event| {
                journal.events.borrow_mut().push(event)
            }));
        }
    });
    invocation
}

/// Observes a real retained tuple before the next transition or while its nested child starts.
pub(in crate::types::infer::builder::local) fn before_step<B>(
    builders: &BuilderStore<'_, '_, '_>,
    owners: &LocalOwners<'_, '_, B>,
    owner: &tuple_annotation::Active,
) {
    RECORDING.with_borrow(|recording| {
        let Some((journal, stop, cancel)) = recording else { return; };
        if journal.reached.get() { return; }
        let Some(OwnerSlot::TupleAnnotation(payload)) = owners.slots.get(owner.0) else { return; };
        let db = builders.builder(payload.builder).db();
        if matches!(&payload.phase, tuple_annotation::Phase::Active(tuple_annotation::State::Elements(elements)) if matches!(&elements.types, TupleSpecBuilder::Fixed(types) if !types.is_empty())) {
            journal.retained.set(true);
        }
        let reached = match (stop, &payload.phase) {
            (Stop::RetainedElement, tuple_annotation::Phase::Active(tuple_annotation::State::Elements(elements))) => {
                matches!(&elements.types, TupleSpecBuilder::Fixed(types) if !types.is_empty()) && !elements.remaining.is_empty()
            }
            (Stop::NestedChild, tuple_annotation::Phase::Active(tuple_annotation::State::Start(_))) => {
                owners.slots[..owner.0].iter().any(|slot| {
                    matches!(slot, OwnerSlot::TupleAnnotation(tuple_annotation::Payload {
                        phase: tuple_annotation::Phase::Pending(tuple_annotation::Pending::Element { target: tuple_annotation::Target::Elements(elements), .. }), ..
                    }) if matches!(&elements.types, TupleSpecBuilder::Fixed(types) if !types.is_empty()))
                })
            }
            _ => false,
        };
        if reached {
            journal.reached.set(true);
            journal.remaining.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(db));
            if *cancel {
                db.cancellation_token().cancel();
                db.unwind_if_revision_cancelled();
            }
        }
    });
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

fn fixture(annotation: &str) -> anyhow::Result<TestDb> {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        format!(
            "from typing import Tuple\nclass First: ...\nclass Second: ...\nvalue: {annotation}\n"
        ),
    )?;
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

fn infer<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> anyhow::Result<AnalysisOutcome<Type<'db>>> {
    expression_type_with_policy(prepared, annotation(prepared)?.into(), policy)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Fixed, empty, homogeneous, and nested tuple annotations share exact ordinary query results.
/// The inner slice is stored only when it is a tuple expression; a single child's type is retained.
#[test]
fn tuple_annotations_complete_cold_and_reuse_canonical_results() -> anyhow::Result<()> {
    for (source, expected) in [
        ("tuple[()]", "tuple[()]"),
        ("tuple[First]", "tuple[First]"),
        ("tuple[First, Second]", "tuple[First, Second]"),
        ("tuple[First, ...]", "tuple[First, ...]"),
        (
            "tuple[First, tuple[Second, ...]]",
            "tuple[First, tuple[Second, ...]]",
        ),
        ("Tuple[First, Second]", "tuple[First, Second]"),
    ] {
        let db = fixture(source)?;
        let prepared = prepare(&db)?;
        let node = annotation(&prepared)?;
        let expression = prepared.semantic_index().expression(node);
        let ingredient = expression_inference_ingredient(&db);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                ingredient,
                InferExpression::Bare(expression).as_id()
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        let revision = salsa::plumbing::current_revision(&db);
        let mut events = db.clone();
        events.take_salsa_events();
        observations::reset(None);
        let result = infer(&prepared, &funded())?;
        let AnalysisOutcome::Complete(ty) = result else {
            anyhow::bail!("{source}: {result:?}");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(ty.display(&db, &env).to_string(), expected);
        cleanup();
        let canonical = infer_expression_types(&db, expression, TypeContext::default());
        assert_eq!(canonical.expression_type(node), ty);
        let ast::Expr::Subscript(subscript) = node else {
            anyhow::bail!("tuple fixture is a subscript");
        };
        let slice_type = canonical.try_expression_type(subscript.slice.as_ref());
        if matches!(subscript.slice.as_ref(), ast::Expr::Tuple(_)) {
            assert_eq!(slice_type, Some(ty));
        } else {
            assert_eq!(
                slice_type.map(|slice| slice.display(&db, &env).to_string()),
                Some("First".to_owned())
            );
        }
        let file = prepared.program_file();
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
        events.take_salsa_events();
        assert_eq!(infer(&prepared, &funded())?, result);
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            Some(InferExpression::Bare(expression).as_id()),
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        cleanup();
    }
    Ok(())
}

fn assert_retired(journal: &Journal) {
    let events = journal.events.borrow();
    let created: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            OwnershipEvent::Created {
                kind: OwnerKind::TupleAnnotation,
                identity,
                ..
            } => Some(*identity),
            _ => None,
        })
        .collect();
    assert!(!created.is_empty());
    for identity in &created {
        assert_eq!(events.iter().filter(|event| matches!(event, OwnershipEvent::PayloadRetired { identity: retired } if retired == identity)).count(), 1);
    }
    let last_payload = events
        .iter()
        .rposition(|event| matches!(event, OwnershipEvent::PayloadRetired { .. }));
    let restored = events
        .iter()
        .rposition(|event| matches!(event, OwnershipEvent::RootRestored));
    assert!(matches!((last_payload, restored), (Some(payload), Some(root)) if payload < root));
    if created.len() > 1 {
        let retirement: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                OwnershipEvent::PayloadRetired { identity } if created.contains(identity) => {
                    Some(*identity)
                }
                _ => None,
            })
            .collect();
        assert_eq!(retirement, created.into_iter().rev().collect::<Vec<_>>());
    }
}

/// Work refusal and cancellation retire a retained element, including when a nested tuple starts;
/// the same revision retries successfully and then reuses the completed expression memo.
#[test]
fn tuple_annotations_retained_state_drains_and_retries() -> anyhow::Result<()> {
    const SOURCE: &str = "tuple[First, tuple[Second, ...]]";
    for stop in [Stop::RetainedElement, Stop::NestedChild] {
        let measured_db = fixture(SOURCE)?;
        let measured = prepare(&measured_db)?;
        observations::reset(None);
        let recording = Recording::new(stop, false);
        assert!(matches!(
            infer(&measured, &funded())?,
            AnalysisOutcome::Complete(_)
        ));
        let Some(remaining) = recording.0.remaining.get() else {
            anyhow::bail!("missing {stop:?} boundary");
        };
        let work = funded().semantic_work_limit - remaining;
        drop(recording);
        cleanup();
        for cancel in [false, true] {
            let db = fixture(SOURCE)?;
            let prepared = prepare(&db)?;
            let expression = prepared.semantic_index().expression(annotation(&prepared)?);
            let revision = salsa::plumbing::current_revision(&db);
            observations::reset(None);
            let recording = Recording::new(stop, cancel);
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                }
            };
            let result = salsa::Cancelled::catch(AssertUnwindSafe(|| infer(&prepared, &policy)));
            if cancel {
                assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            } else {
                assert_eq!(
                    result.map_err(|error| anyhow::anyhow!("{error:?}"))??,
                    AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: ()
                    }
                );
            }
            assert!(recording.0.reached.get());
            assert_retired(&recording.0);
            if !cancel {
                assert_eq!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        expression_inference_ingredient(&db),
                        InferExpression::Bare(expression).as_id()
                    )
                    .map(|_| ()),
                    Err(FinalSourceError::MissingMemo)
                );
            }
            drop(recording);
            cleanup();
            observations::reset(None);
            let retried = infer(&prepared, &funded())?;
            assert!(
                matches!(retried, AnalysisOutcome::Complete(_)),
                "{retried:?}"
            );
            let mut events = db.clone();
            events.take_salsa_events();
            assert_eq!(infer(&prepared, &funded())?, retried);
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                Some(InferExpression::Bare(expression).as_id()),
                &events.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            cleanup();
        }
    }
    Ok(())
}

fn reaches_nested_child(byte_limit: usize) -> anyhow::Result<bool> {
    let db = fixture("tuple[First, tuple[Second, ...]]")?;
    let prepared = prepare(&db)?;
    observations::reset(None);
    let recording = Recording::new(Stop::NestedChild, false);
    let result = infer(
        &prepared,
        &AnalysisPolicy {
            requested_bytes_limit: byte_limit,
            ..funded()
        },
    )?;
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

/// A real byte limit can interrupt after a tuple retains an element and before its nested child
/// advances; its payload is retired and the unchanged expression succeeds on retry.
#[test]
fn tuple_annotation_byte_refusal_retires_retained_elements() -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_nested_child(high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_nested_child(middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture("tuple[First, tuple[Second, ...]]")?;
    let prepared = prepare(&db)?;
    let expression = prepared.semantic_index().expression(annotation(&prepared)?);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::new(Stop::NestedChild, false);
    let result = infer(
        &prepared,
        &AnalysisPolicy {
            requested_bytes_limit: low,
            ..funded()
        },
    )?;
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }
    );
    assert!(recording.0.retained.get());
    assert!(!recording.0.reached.get());
    assert_retired(&recording.0);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            InferExpression::Bare(expression).as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    drop(recording);
    cleanup();
    observations::reset(None);
    assert!(matches!(
        infer(&prepared, &funded())?,
        AnalysisOutcome::Complete(_)
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}

/// Unsupported child inference and invalid ellipses keep their named refusals; neither can publish
/// an approximate tuple result, while ordinary inference still handles those annotations.
#[test]
fn tuple_annotations_preserve_unavailable_children_and_diagnostics() -> anyhow::Result<()> {
    for (source, operation) in [
        (
            "tuple[..., First]",
            crate::analysis::OperationId::TypeExpressionInvalid,
        ),
        (
            "tuple[*tuple[First, Second]]",
            crate::analysis::OperationId::TypeExpressionLegacy,
        ),
    ] {
        let db = fixture(source)?;
        let prepared = prepare(&db)?;
        let expression = prepared.semantic_index().expression(annotation(&prepared)?);
        observations::reset(None);
        let result = infer(&prepared, &funded())?;
        assert_eq!(
            result,
            AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(operation),
                completed: ()
            }
        );
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                expression_inference_ingredient(&db),
                InferExpression::Bare(expression).as_id()
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        cleanup();
        let ordinary = infer_expression_types(&db, expression, TypeContext::default());
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(
            ordinary
                .expression_type(annotation(&prepared)?)
                .display(&db, &env)
                .to_string(),
            if source.contains("...") {
                "tuple[Unknown, First]"
            } else {
                "tuple[First, Second]"
            }
        );
    }
    Ok(())
}
