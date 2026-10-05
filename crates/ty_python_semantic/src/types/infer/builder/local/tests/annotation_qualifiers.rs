//! Qualifier tests exercise the local annotation driver inside the real source runtime.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, RunResult};
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::assert_no_active_attempt;
use ty_python_core::Program;

use super::super::*;
use crate::analysis::{
    AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, OperationId, PreparedAnalysisFile,
    expression_type_with_policy, prepare_file,
};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::types::infer::builder::annotation_expression::AnnotationEffects;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, observations,
};
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::infer::source_runtime::tests::signature_annotations::State;
use crate::types::infer::{
    InferenceRegion, definition_inference_ingredient, infer_definition_types,
};
use crate::types::typevar::{
    TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation,
    TypeVarIdentity, TypeVarInstance, TypeVarNonce,
};
use crate::types::{BindingContext, BoundTypeVarInstance, TypeVarKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    ParentSuspended,
    ChildCompleted,
}

#[derive(Debug, Default)]
struct Journal {
    builder: Cell<Option<usize>>,
    initial: Cell<Option<State>>,
    restored: Cell<Option<State>>,
    remaining: Cell<Option<usize>>,
    reached: Cell<bool>,
    suspended: Cell<bool>,
    drained: Cell<bool>,
}

thread_local! {
    static RECORDING: RefCell<Option<(Rc<Journal>, Stop, bool)>> = const { RefCell::new(None) };
}

#[derive(Debug)]
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

pub(in crate::types::infer::builder::local) fn invocation_started(builder: usize, state: State) {
    RECORDING.with_borrow(|recording| {
        if let Some((journal, ..)) = recording
            && journal.builder.get().is_none()
        {
            journal.builder.set(Some(builder));
            journal.initial.set(Some(state));
        }
    });
}

/// Records a real suspended annotation before its next admitted transition.
fn boundary(db: &dyn Db, builder: usize, depth: usize, boundary: Stop) {
    RECORDING.with_borrow(|recording| {
        let Some((journal, stop, cancel)) = recording else {
            return;
        };
        if journal.builder.get() != Some(builder) || depth == 0 {
            return;
        }
        journal.suspended.set(true);
        if journal.reached.get() || *stop != boundary {
            return;
        }
        // Two nested qualifiers ensure that the child completion still has a suspended parent.
        if depth < 2 {
            return;
        }
        journal.reached.set(true);
        journal
            .remaining
            .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                db,
            ));
        if *cancel {
            db.cancellation_token().cancel();
            db.unwind_if_revision_cancelled();
        }
    });
}

pub(in crate::types::infer::builder::local) fn parent_suspended(
    db: &dyn Db,
    builder: usize,
    depth: usize,
) {
    boundary(db, builder, depth, Stop::ParentSuspended);
}

pub(in crate::types::infer::builder::local) fn child_completed(
    db: &dyn Db,
    builder: usize,
    depth: usize,
) {
    boundary(db, builder, depth, Stop::ChildCompleted);
}

pub(in crate::types::infer::builder::local) fn continuations_retired(
    builder: usize,
    remaining: usize,
) {
    RECORDING.with_borrow(|recording| {
        if let Some((journal, ..)) = recording
            && journal.builder.get() == Some(builder)
        {
            assert_eq!(remaining, 0);
            journal.drained.set(true);
        }
    });
}

pub(in crate::types::infer::builder::local) fn restored(builder: usize, state: State) {
    RECORDING.with_borrow(|recording| {
        if let Some((journal, ..)) = recording
            && journal.builder.get() == Some(builder)
        {
            assert!(journal.drained.get());
            journal.restored.set(Some(state));
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
    let mut db = TestDbBuilder::new()
        .with_python_version(ast::PythonVersion::PY313)
        .build()?;
    db.write_file("src/main.pyi", format!("from typing import ClassVar, Final, Required, NotRequired\nfrom typing_extensions import ReadOnly\nfrom dataclasses import InitVar\nclass Leaf: ...\nvalue: {annotation}\n"))?;
    Ok(db)
}

fn prepare(db: &TestDb) -> anyhow::Result<PreparedAnalysisFile<'_>> {
    prepare_file(db, system_path_to_file(db, "src/main.pyi")?)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn assignment<'ast>(
    prepared: &'ast PreparedAnalysisFile<'_>,
) -> anyhow::Result<&'ast ast::StmtAnnAssign> {
    let Some(ast::Stmt::AnnAssign(assignment)) = prepared.parsed_module().syntax().body.last()
    else {
        anyhow::bail!("fixture must end with an annotated assignment");
    };
    Ok(assignment)
}

struct AnnotationRequest<'builder, 'db, 'ast> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    annotation: &'ast ast::Expr,
}

impl<'db> MemberOperation<'db> for AnnotationRequest<'_, 'db, '_> {
    type Output = TypeAndQualifiers<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        source::annotation(
            self.builder,
            self.annotation,
            DeferredExpressionState::None,
            PEP613Policy::Disallowed,
            &SourceEffects::new(access, program),
        )
        .await
    }
}

#[derive(Debug)]
struct AnnotationRun<'db> {
    result: AnalysisOutcome<TypeAndQualifiers<'db>>,
    expressions: FxHashMap<ty_python_core::ExpressionNodeKey, Type<'db>>,
    qualifiers: FxHashMap<ty_python_core::ExpressionNodeKey, TypeQualifiers>,
}

/// Runs annotation inference in the registered runtime, retaining its unpublished expression maps.
fn infer<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> anyhow::Result<AnnotationRun<'db>> {
    let assignment = assignment(prepared)?;
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(assignment);
    let mut builder = TypeInferenceBuilder::new(
        db,
        &env,
        InferenceRegion::Definition(definition),
        file.file(db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    );
    let result = controlled_member_operation(
        prepared,
        AnnotationRequest {
            builder: &mut builder,
            annotation: &assignment.annotation,
        },
        policy,
    );
    assert!(builder.context.finish().is_empty());
    let result = result.map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(AnnotationRun {
        result,
        expressions: builder.expressions,
        qualifiers: builder.qualifiers,
    })
}

fn cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// All qualifier kinds preserve the complete ordinary result and expression/qualifier storage.
/// A bare Final stores its special-form expression type while declaring qualified Unknown.
#[test]
fn annotation_qualifiers_match_ordinary_results_and_storage() -> anyhow::Result<()> {
    for source in [
        "ClassVar",
        "Final",
        "ClassVar[Leaf]",
        "Final[Leaf]",
        "Required[Leaf]",
        "NotRequired[Leaf]",
        "ReadOnly[Leaf]",
        "InitVar[Leaf]",
        "Required[ReadOnly[Leaf]]",
        "ReadOnly[NotRequired[Leaf]]",
        "Final[Leaf,]",
    ] {
        let db = fixture(source)?;
        let prepared = prepare(&db)?;
        observations::reset(None);
        let actual = infer(&db, &prepared, &funded())?;
        let AnalysisOutcome::Complete(actual_ty) = actual.result else {
            anyhow::bail!("{source}: {:?}", actual.result);
        };
        cleanup();
        let assignment = assignment(&prepared)?;
        let file = prepared.program_file();
        let env = ProgramEnvironment::from_file(file);
        let definition = prepared
            .semantic_index()
            .expect_single_definition(assignment);
        let mut ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            file.file(&db),
            file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        );
        let expected = annotation(
            &mut ordinary,
            &assignment.annotation,
            DeferredExpressionState::None,
            PEP613Policy::Disallowed,
        );
        assert!(ordinary.context.finish().is_empty());
        assert_eq!(actual_ty, expected, "{source}");
        assert_eq!(actual.expressions, ordinary.expressions, "{source}");
        assert_eq!(actual.qualifiers, ordinary.qualifiers, "{source}");
        assert_eq!(
            actual
                .qualifiers
                .get(&assignment.annotation.as_ref().into()),
            Some(&actual_ty.qualifiers())
        );
        if source == "Final" {
            assert_eq!(
                actual
                    .expressions
                    .get(&assignment.annotation.as_ref().into()),
                Some(&Type::SpecialForm(SpecialFormType::TypeQualifier(
                    TypeQualifier::Final
                )))
            );
            assert_eq!(actual_ty.inner_type(), Type::unknown());
        }
        if let ast::Expr::Subscript(subscript) = &*assignment.annotation
            && subscript.slice.is_tuple_expr()
        {
            assert_eq!(
                actual.expressions.get(&subscript.slice.as_ref().into()),
                Some(&actual_ty.inner_type())
            );
        }
    }
    Ok(())
}

struct NonSelfRequest<'builder, 'db, 'ast> {
    builder: &'builder TypeInferenceBuilder<'db, 'ast>,
    ty: Type<'db>,
}

impl<'db> MemberOperation<'db> for NonSelfRequest<'_, 'db, '_> {
    type Output = bool;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<bool>
    where
        'db: 'run,
    {
        AnnotationEffects::has_non_self_typevar(
            &SourceEffects::new(access, program),
            self.builder,
            self.ty,
        )
        .await
    }
}

/// Self is excluded by kind, while ordinary eager bounds/defaults still participate in the search.
#[test]
fn annotation_qualifiers_non_self_search_preserves_eager_traversal() -> anyhow::Result<()> {
    let db = fixture("Final[Leaf]")?;
    let prepared = prepare(&db)?;
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(assignment(&prepared)?);
    let builder = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Definition(definition),
        file.file(&db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    );
    let bind = |kind, bounds, default| {
        let variable = TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(&db, Name::new_static("T"), None, kind),
            bounds,
            None,
            default,
        );
        Type::TypeVar(BoundTypeVarInstance::new(
            &db,
            variable,
            BindingContext::Synthetic(file.program(&db)),
            None,
            TypeVarNonce::NONE,
        ))
    };
    let ordinary = bind(TypeVarKind::LegacyTypeVar, None, None);
    for (ty, expected) in [
        (ordinary, true),
        (bind(TypeVarKind::TypingSelf, None, None), false),
        (
            bind(
                TypeVarKind::TypingSelf,
                Some(TypeVarBoundOrConstraintsEvaluation::Eager(
                    TypeVarBoundOrConstraints::UpperBound(ordinary),
                )),
                None,
            ),
            true,
        ),
        (
            bind(
                TypeVarKind::TypingSelf,
                None,
                Some(TypeVarDefaultEvaluation::Eager(ordinary)),
            ),
            true,
        ),
    ] {
        observations::reset(None);
        let actual = controlled_member_operation(
            &prepared,
            NonSelfRequest {
                builder: &builder,
                ty,
            },
            &funded(),
        )
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(actual, AnalysisOutcome::Complete(expected));
        assert_eq!(ty.has_non_self_typevar(&db, &env), expected);
        cleanup();
    }
    assert!(builder.context.finish().is_empty());
    Ok(())
}

fn assert_restored(journal: &Journal) {
    assert!(journal.suspended.get());
    assert!(journal.drained.get());
    assert!(journal.initial.get().is_some());
    assert_eq!(journal.restored.get(), journal.initial.get());
}

/// Work refusal and cancellation drain suspended qualifier parents, restore scope, and retry.
/// A completed child leaves its canonical dependencies available to the same-revision retry.
#[test]
fn annotation_qualifiers_interrupted_continuations_restore_and_retry() -> anyhow::Result<()> {
    for stop in [Stop::ParentSuspended, Stop::ChildCompleted] {
        let measured_db = fixture("Required[ReadOnly[Leaf]]")?;
        let measured = prepare(&measured_db)?;
        observations::reset(None);
        let recording = Recording::new(stop, false);
        assert!(matches!(
            infer(&measured_db, &measured, &funded())?.result,
            AnalysisOutcome::Complete(_)
        ));
        let Some(remaining) = recording.0.remaining.get() else {
            anyhow::bail!("missing {stop:?} boundary");
        };
        let work = funded().semantic_work_limit - remaining;
        drop(recording);
        cleanup();
        for cancel in [false, true] {
            let db = fixture("Required[ReadOnly[Leaf]]")?;
            let prepared = prepare(&db)?;
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
            let result =
                salsa::Cancelled::catch(AssertUnwindSafe(|| infer(&db, &prepared, &policy)));
            if cancel {
                assert!(matches!(result, Err(salsa::Cancelled::Local)));
            } else {
                assert_eq!(
                    result
                        .map_err(|error| anyhow::anyhow!("{error:?}"))??
                        .result,
                    AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: ()
                    }
                );
            }
            assert!(recording.0.reached.get());
            assert_restored(&recording.0);
            drop(recording);
            cleanup();
            let mut events = db.clone();
            events.take_salsa_events();
            observations::reset(None);
            assert!(matches!(
                infer(&db, &prepared, &funded())?.result,
                AnalysisOutcome::Complete(_)
            ));
            if stop == Stop::ChildCompleted {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    None,
                    &events.take_salsa_events(),
                );
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            cleanup();
        }
    }
    Ok(())
}

fn reaches_completed_child(bytes: usize) -> anyhow::Result<bool> {
    let db = fixture("Required[ReadOnly[Leaf]]")?;
    let prepared = prepare(&db)?;
    observations::reset(None);
    let recording = Recording::new(Stop::ChildCompleted, false);
    let result = infer(
        &db,
        &prepared,
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    )?
    .result;
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

/// A byte refusal after child storage retains and drains the annotation parents before retry.
#[test]
fn annotation_qualifiers_byte_refusal_drains_and_retries() -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_completed_child(high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_completed_child(middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture("Required[ReadOnly[Leaf]]")?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::new(Stop::ChildCompleted, false);
    assert_eq!(
        infer(
            &db,
            &prepared,
            &AnalysisPolicy {
                requested_bytes_limit: high,
                ..funded()
            }
        )?
        .result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }
    );
    assert!(recording.0.reached.get());
    assert_restored(&recording.0);
    drop(recording);
    cleanup();
    observations::reset(None);
    assert!(matches!(
        infer(&db, &prepared, &funded())?.result,
        AnalysisOutcome::Complete(_)
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}

/// Unavailable qualifier diagnostics and enclosing-class checks cannot produce successful recovery.
#[test]
fn annotation_qualifiers_preserve_named_refusals() -> anyhow::Result<()> {
    for annotation in [
        "Required",
        "ClassVar[Final[Leaf]]",
        "Required[NotRequired[Leaf]]",
        "Final[Leaf, Leaf]",
    ] {
        let db = fixture(annotation)?;
        let prepared = prepare(&db)?;
        observations::reset(None);
        assert_eq!(
            infer(&db, &prepared, &funded())?.result,
            AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::AnnotationQualifier),
                completed: ()
            },
            "{annotation}"
        );
        cleanup();
    }
    Ok(())
}

/// A cold Final declaration publishes the ordinary canonical payload and reuses it on retry.
#[test]
fn annotation_qualifiers_final_definition_preserves_canonical_memos() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        "from typing import Final\nclass Leaf: ...\nvalue: Final[Leaf]\nleft = right = value\n",
    )?;
    let prepared = prepare(&db)?;
    let body = &prepared.parsed_module().syntax().body;
    let ast::Stmt::AnnAssign(assignment) = &body[2] else {
        anyhow::bail!("expected annotated value");
    };
    let Some(ast::Stmt::Assign(read)) = body.last() else {
        anyhow::bail!("expected final read");
    };
    let definition = prepared
        .semantic_index()
        .expect_single_definition(assignment);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let actual = expression_type_with_policy(&prepared, read.value.as_ref().into(), &funded())
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let AnalysisOutcome::Complete(ty) = actual else {
        anyhow::bail!("{actual:?}");
    };
    cleanup();
    let canonical = infer_definition_types(&db, definition);
    assert_eq!(
        canonical.completed_declaration(definition),
        Some(TypeAndQualifiers::declared(ty).with_qualifier(TypeQualifiers::FINAL))
    );
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Definition(definition),
        file.file(&db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_definition(definition);
    assert_eq!(canonical, &ordinary);
    let mut events = db.clone();
    events.take_salsa_events();
    assert_eq!(
        expression_type_with_policy(&prepared, read.value.as_ref().into(), &funded())
            .map_err(|error| anyhow::anyhow!("{error:?}"))?,
        actual
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(definition.as_id()),
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}

/// Interrupting a nested Final after its child completes cannot publish the parent definition;
/// retry reuses completed dependencies and publishes the complete declaration in the same revision.
#[test]
fn annotation_qualifiers_interrupted_definition_is_not_published() -> anyhow::Result<()> {
    fn fixture() -> anyhow::Result<TestDb> {
        let mut db = setup_db();
        db.write_file("src/main.pyi", "from typing import Final\nclass Leaf: ...\nvalue: Final[Final[Leaf]]\nleft = right = value\n")?;
        Ok(db)
    }

    fn read_value(
        prepared: &PreparedAnalysisFile<'_>,
    ) -> anyhow::Result<ty_python_core::ExpressionNodeKey> {
        let Some(ast::Stmt::Assign(read)) = prepared.parsed_module().syntax().body.last() else {
            anyhow::bail!("fixture must end with a value read");
        };
        Ok(read.value.as_ref().into())
    }

    let measured_db = fixture()?;
    let measured = prepare(&measured_db)?;
    observations::reset(None);
    let recording = Recording::new(Stop::ChildCompleted, false);
    let result = expression_type_with_policy(&measured, read_value(&measured)?, &funded())
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert!(matches!(result, AnalysisOutcome::Complete(_)), "{result:?}");
    let Some(remaining) = recording.0.remaining.get() else {
        anyhow::bail!("nested Final must complete its child");
    };
    let work = funded().semantic_work_limit - remaining;
    drop(recording);
    cleanup();

    let db = fixture()?;
    let prepared = prepare(&db)?;
    let ast::Stmt::AnnAssign(assignment) = &prepared.parsed_module().syntax().body[2] else {
        anyhow::bail!("expected annotated value");
    };
    let definition = prepared
        .semantic_index()
        .expect_single_definition(assignment);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::new(Stop::ChildCompleted, false);
    let result = expression_type_with_policy(
        &prepared,
        read_value(&prepared)?,
        &AnalysisPolicy {
            semantic_work_limit: work,
            ..funded()
        },
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        }
    );
    assert!(recording.0.reached.get());
    assert_restored(&recording.0);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    drop(recording);
    cleanup();
    let ast::Stmt::ClassDef(leaf) = &prepared.parsed_module().syntax().body[1] else {
        anyhow::bail!("expected the child class");
    };
    let leaf = prepared.semantic_index().expect_single_definition(leaf);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            leaf.as_id()
        )
        .is_ok()
    );
    let mut events = db.clone();
    events.take_salsa_events();
    observations::reset(None);
    let result = expression_type_with_policy(&prepared, read_value(&prepared)?, &funded())
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let AnalysisOutcome::Complete(ty) = result else {
        anyhow::bail!("{result:?}");
    };
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(leaf.as_id()),
        &events.take_salsa_events(),
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .is_ok()
    );
    assert_eq!(
        infer_definition_types(&db, definition).completed_declaration(definition),
        Some(TypeAndQualifiers::declared(ty).with_qualifier(TypeQualifiers::FINAL))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}
