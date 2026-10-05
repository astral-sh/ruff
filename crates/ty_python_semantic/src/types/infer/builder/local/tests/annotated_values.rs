//! Annotated-value controls run the real source adapter with a caller-owned builder.

use std::cell::{Cell, RefCell};
use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use salsa::execution_probe::RunResult;
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::assert_no_active_attempt;
use ty_python_core::Program;
use ty_python_core::definition::{AnnotatedAssignmentDefinitionKind, DefinitionKind};

use super::super::*;
use crate::analysis::{
    AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, AssignmentValidationOperation,
    OperationId, PreparedAnalysisFile, prepare_file,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::infer::builder::annotated_assignment::{
    AnnotatedAssignmentEffects, AnnotatedAssignmentOperation,
};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, observations,
};
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::infer::source_runtime::tests::signature_annotations::State;

/// Observable stages of the source adapter's annotated-value transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer::builder) enum Event {
    RhsEntered,
    RhsCompleted,
    BeforeBinding,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    scalar: State,
    cache: Option<usize>,
    cache_owners: Option<usize>,
    fields_len: usize,
    fields_capacity: usize,
    fields_spilled: bool,
    fields_pointer: usize,
}

impl Snapshot {
    /// Captures scalar state and owner identities without cloning either backing allocation.
    fn new(builder: &TypeInferenceBuilder<'_, '_>) -> Self {
        let fields = &builder.dataclass_field_specifiers;
        Self {
            scalar: function_annotation_state(builder),
            cache: builder
                .expression_cache
                .as_ref()
                .map(|cache| Rc::as_ptr(cache) as usize),
            cache_owners: builder.expression_cache.as_ref().map(Rc::strong_count),
            fields_len: fields.len(),
            fields_capacity: fields.capacity(),
            fields_spilled: fields.spilled(),
            fields_pointer: fields.as_ptr() as usize,
        }
    }
}

#[derive(Debug, Default)]
struct Journal {
    boundaries: RefCell<Vec<(Event, Snapshot, Option<usize>)>>,
    rhs_retired: Cell<bool>,
    restored: Cell<Option<Snapshot>>,
    restored_after_rhs: Cell<bool>,
}

thread_local! {
    static RECORDING: RefCell<Option<(usize, Rc<Journal>)>> = const { RefCell::new(None) };
    static CANCEL_AT_RHS: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug)]
struct Recording(Rc<Journal>);

impl Recording {
    /// Records only the borrowed builder selected by this control, excluding canonical children.
    fn new(builder: &TypeInferenceBuilder<'_, '_>) -> Self {
        let journal = Rc::new(Journal::default());
        RECORDING.with_borrow_mut(|recording| {
            assert!(
                recording
                    .replace((builder as *const _ as usize, journal.clone()))
                    .is_none()
            );
        });
        Self(journal)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with_borrow_mut(|recording| *recording = None);
        CANCEL_AT_RHS.set(false);
    }
}

/// Records the actual adapter's boundaries and the work remaining before its next operation.
/// A selected RHS entry requests cancellation after recording its temporary state.
pub(in crate::types::infer::builder) fn value_boundary(
    builder: &TypeInferenceBuilder<'_, '_>,
    event: Event,
) {
    RECORDING.with_borrow(|recording| {
        if let Some((pointer, journal)) = recording
            && *pointer == builder as *const _ as usize
        {
            journal.boundaries.borrow_mut().push((
                event,
                Snapshot::new(builder),
                salsa::attempt_probe::remaining_allowance_for_diagnostics(builder.db()),
            ));
            if event == Event::RhsEntered && CANCEL_AT_RHS.replace(false) {
                builder.db().cancellation_token().cancel();
                builder.db().unwind_if_revision_cancelled();
            }
        }
    });
}

/// Records outer rollback after the saved field-specifier owner has returned to the builder.
pub(in crate::types::infer::builder) fn restored(builder: &TypeInferenceBuilder<'_, '_>) {
    RECORDING.with_borrow(|recording| {
        if let Some((pointer, journal)) = recording
            && *pointer == builder as *const _ as usize
        {
            journal.restored.set(Some(Snapshot::new(builder)));
            journal.restored_after_rhs.set(journal.rhs_retired.get());
        }
    });
}

/// Records that the RHS local invocation has dropped its continuations and owned payloads.
pub(in crate::types::infer::builder) fn rhs_retired(builder: &TypeInferenceBuilder<'_, '_>) {
    RECORDING.with_borrow(|recording| {
        if let Some((pointer, journal)) = recording
            && *pointer == builder as *const _ as usize
        {
            journal.rhs_retired.set(true);
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cache {
    Absent,
    Existing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fields {
    EmptySpilled,
    NonemptyInline,
    NonemptySpilled,
}

/// Invokes the value transaction on a builder that remains owned by the test.
struct ValueRequest<'builder, 'db, 'ast> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    assignment: &'db AnnotatedAssignmentDefinitionKind,
    definition: Definition<'db>,
    declared: TypeAndQualifiers<'db>,
    value: &'ast ast::Expr,
}

impl<'db> MemberOperation<'db> for ValueRequest<'_, 'db, '_> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<()>
    where
        'db: 'run,
    {
        AnnotatedAssignmentEffects::value_assignment(
            &SourceEffects::new(access, program),
            self.builder,
            self.assignment,
            self.definition,
            self.declared,
            false,
            self.value,
        )
        .await
    }
}

#[derive(Debug)]
struct ValueRun<'db> {
    result: Option<AnalysisOutcome<()>>,
    definition: salsa::Id,
    initial: Snapshot,
    final_state: Snapshot,
    initial_fields: Vec<Type<'db>>,
    final_fields: Vec<Type<'db>>,
    target: Option<Type<'db>>,
    journal: Rc<Journal>,
}

impl ValueRun<'_> {
    fn boundary(&self, event: Event) -> Option<(Snapshot, Option<usize>)> {
        self.journal
            .boundaries
            .borrow()
            .iter()
            .find_map(|(observed, state, remaining)| {
                (*observed == event).then_some((*state, *remaining))
            })
    }

    /// Checks exact incoming state after a refused value transaction has drained its RHS.
    fn assert_restored(&self) {
        assert_eq!(self.initial, self.final_state);
        assert_eq!(self.initial_fields, self.final_fields);
        assert_eq!(self.journal.restored.get(), Some(self.initial));
        assert!(self.journal.restored_after_rhs.get());
        assert!(self.boundary(Event::Complete).is_none());
    }
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

/// Defines a separate incoming binding context and a valued target that bypasses enum-name checks.
/// The method gives the RHS its ordinary standalone expression entry.
fn fixture() -> anyhow::Result<TestDb> {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "marker: bool\nclass Holder:\n    def method(self):\n        __value: bool = True\n",
    )?;
    Ok(db)
}

fn prepare(db: &TestDb) -> anyhow::Result<PreparedAnalysisFile<'_>> {
    prepare_file(db, system_path_to_file(db, "src/main.py")?)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

/// Runs the real value adapter with supplied declaration input and inspects its borrowed builder.
/// Declaration inference is outside this control so incoming field specifiers reach the value entry.
/// A caught local cancellation sets `ValueRun::result` to `None` while retaining the observed state.
fn infer<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    fields: Fields,
    cache: Cache,
    declared: Type<'db>,
    policy: &AnalysisPolicy,
) -> anyhow::Result<ValueRun<'db>> {
    let [ast::Stmt::AnnAssign(marker), ast::Stmt::ClassDef(class)] =
        prepared.parsed_module().suite().as_slice()
    else {
        anyhow::bail!("fixture must contain the marker and holder class");
    };
    let [ast::Stmt::FunctionDef(method)] = class.body.as_slice() else {
        anyhow::bail!("fixture class must contain one method");
    };
    let [ast::Stmt::AnnAssign(node)] = method.body.as_slice() else {
        anyhow::bail!("fixture method must contain the valued assignment");
    };
    let index = prepared.semantic_index();
    let definition = index.expect_single_definition(node);
    let DefinitionKind::AnnotatedAssignment(assignment) = definition.kind(db) else {
        anyhow::bail!("fixture definition must be an annotated assignment");
    };
    let Some(value) = node.value.as_deref() else {
        anyhow::bail!("fixture assignment must have a value");
    };
    anyhow::ensure!(
        index.try_expression(value).is_some(),
        "method fixture RHS must have a canonical expression entry"
    );
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let mut builder = TypeInferenceBuilder::new(
        db,
        &env,
        InferenceRegion::Definition(definition),
        file.file(db),
        file,
        index,
        prepared.parsed_module(),
    );
    builder.typevar_binding_context = Some(index.expect_single_definition(marker));
    builder
        .context
        .inference_flags
        .remove(InferenceFlags::CHECK_UNBOUND_TYPEVARS | InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR);
    builder
        .context
        .inference_flags
        .insert(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT);
    builder.deferred_state =
        DeferredExpressionState::InStringAnnotation(NodeKey::from_node(&*node.annotation));
    if let Cache::Existing = cache {
        builder.setup_expression_cache();
    }
    match fields {
        Fields::EmptySpilled => builder.dataclass_field_specifiers.reserve(8),
        Fields::NonemptyInline => builder.dataclass_field_specifiers.push(Type::unknown()),
        Fields::NonemptySpilled => {
            builder.dataclass_field_specifiers.reserve(8);
            builder
                .dataclass_field_specifiers
                .extend([Type::unknown(), Type::bool_literal(false)]);
        }
    }
    let initial = Snapshot::new(&builder);
    let initial_fields = builder.dataclass_field_specifiers.to_vec();
    observations::reset(None);
    let recording = Recording::new(&builder);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            prepared,
            ValueRequest {
                builder: &mut builder,
                assignment,
                definition,
                declared: TypeAndQualifiers::declared(declared),
                value,
            },
            policy,
        )
    }));
    let final_state = Snapshot::new(&builder);
    let final_fields = builder.dataclass_field_specifiers.to_vec();
    let target = builder.try_expression_type(&node.target);
    assert!(builder.context.finish().is_empty());
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let result = match result {
        Ok(result) => Some(result.map_err(|error| anyhow::anyhow!("{error:?}"))?),
        Err(salsa::Cancelled::Local) => None,
        Err(error) => anyhow::bail!("unexpected cancellation: {error:?}"),
    };
    Ok(ValueRun {
        result,
        definition: definition.as_id(),
        initial,
        final_state,
        initial_fields,
        final_fields,
        target,
        journal: recording.0.clone(),
    })
}

/// Successful inference restores incoming scalar context before binding validation, preserves cache
/// identity, and commits an empty working field list instead of retaining the incoming spilled owner.
#[test]
fn annotated_value_restores_context_before_binding_and_commits_fields() -> anyhow::Result<()> {
    for cache in [Cache::Absent, Cache::Existing] {
        let db = fixture()?;
        let prepared = prepare(&db)?;
        let actual = infer(
            &db,
            &prepared,
            Fields::EmptySpilled,
            cache,
            Type::unknown(),
            &funded(),
        )?;
        assert_eq!(actual.result, Some(AnalysisOutcome::Complete(())));
        assert_eq!(actual.target, Some(Type::bool_literal(true)));
        let Some((rhs, _)) = actual.boundary(Event::RhsEntered) else {
            anyhow::bail!("value did not enter RHS inference");
        };
        assert_eq!(rhs.scalar.binding, Some(actual.definition));
        assert_ne!(rhs.scalar.binding, actual.initial.scalar.binding);
        let Some((binding, _)) = actual.boundary(Event::BeforeBinding) else {
            anyhow::bail!("value did not reach binding validation");
        };
        assert_eq!(binding.scalar, actual.initial.scalar);
        assert_eq!(binding.cache, actual.initial.cache);
        assert_eq!(binding.cache_owners, actual.initial.cache_owners);
        assert_eq!(binding.fields_len, 0);
        assert!(actual.initial.fields_spilled);
        assert!(!actual.final_state.fields_spilled);
        assert_eq!(actual.final_state.scalar, actual.initial.scalar);
        assert_eq!(actual.final_state.cache, actual.initial.cache);
        assert!(actual.final_fields.is_empty());
        assert!(actual.journal.rhs_retired.get());
        assert!(actual.journal.restored.get().is_none());
        assert!(actual.boundary(Event::Complete).is_some());
    }
    Ok(())
}

/// Nonempty incoming field lists refuse before the RHS and preserve their contents and allocation.
#[test]
fn annotated_value_nonempty_fields_refuse_without_changing_state() -> anyhow::Result<()> {
    for fields in [Fields::NonemptyInline, Fields::NonemptySpilled] {
        let db = fixture()?;
        let prepared = prepare(&db)?;
        let actual = infer(
            &db,
            &prepared,
            fields,
            Cache::Existing,
            Type::unknown(),
            &funded(),
        )?;
        assert_eq!(
            actual.result,
            Some(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::AnnotatedAssignment(
                    AnnotatedAssignmentOperation::FieldSpecifiers,
                )),
                completed: (),
            })
        );
        assert_eq!(actual.initial, actual.final_state);
        assert_eq!(actual.initial_fields, actual.final_fields);
        assert!(actual.boundary(Event::RhsEntered).is_none());
        assert!(actual.journal.restored.get().is_none());
    }
    Ok(())
}

/// A diagnostic refusal after RHS completion restores the original empty spilled buffer and context.
#[test]
fn annotated_value_validation_refusal_restores_after_rhs() -> anyhow::Result<()> {
    for cache in [Cache::Absent, Cache::Existing] {
        let db = fixture()?;
        let prepared = prepare(&db)?;
        let actual = infer(
            &db,
            &prepared,
            Fields::EmptySpilled,
            cache,
            Type::Never,
            &funded(),
        )?;
        assert_eq!(
            actual.result,
            Some(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(
                    OperationId::AssignmentValidation(
                        AssignmentValidationOperation::InvalidDiagnostic,
                    )
                ),
                completed: (),
            })
        );
        assert!(actual.boundary(Event::RhsCompleted).is_some());
        assert!(actual.boundary(Event::BeforeBinding).is_some());
        actual.assert_restored();
    }
    Ok(())
}

/// Finds whether a cold value run reaches binding validation under the given byte allowance.
fn reaches_binding(bytes: usize) -> anyhow::Result<bool> {
    let db = fixture()?;
    let prepared = prepare(&db)?;
    let actual = infer(
        &db,
        &prepared,
        Fields::EmptySpilled,
        Cache::Existing,
        Type::unknown(),
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    )?;
    assert!(matches!(
        actual.result,
        Some(AnalysisOutcome::Complete(()) | AnalysisOutcome::Incomplete { .. })
    ));
    Ok(actual.boundary(Event::BeforeBinding).is_some())
}

/// Work refusal inside the RHS and work/byte refusal before binding storage restore incoming owners;
/// a funded retry with a fresh parent builder completes in the same database revision.
#[test]
fn annotated_value_budget_refusals_restore_and_retry() -> anyhow::Result<()> {
    let measured_db = fixture()?;
    let measured = prepare(&measured_db)?;
    let baseline = infer(
        &measured_db,
        &measured,
        Fields::EmptySpilled,
        Cache::Existing,
        Type::unknown(),
        &funded(),
    )?;
    assert_eq!(baseline.result, Some(AnalysisOutcome::Complete(())));
    let Some((_, Some(remaining))) = baseline.boundary(Event::BeforeBinding) else {
        anyhow::bail!("missing work observation before binding");
    };
    let work = funded().semantic_work_limit - remaining;
    let Some((_, Some(rhs_remaining))) = baseline.boundary(Event::RhsCompleted) else {
        anyhow::bail!("missing work observation after RHS inference");
    };
    let rhs_work = (funded().semantic_work_limit - rhs_remaining)
        .checked_sub(1)
        .ok_or_else(|| anyhow::anyhow!("RHS inference must consume work"))?;
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_binding(high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_binding(middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    for (policy, reason, reached) in [
        (
            AnalysisPolicy {
                semantic_work_limit: rhs_work,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
            Event::RhsEntered,
        ),
        (
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
            Event::BeforeBinding,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: high,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
            Event::BeforeBinding,
        ),
    ] {
        let db = fixture()?;
        let prepared = prepare(&db)?;
        let revision = salsa::plumbing::current_revision(&db);
        let actual = infer(
            &db,
            &prepared,
            Fields::EmptySpilled,
            Cache::Existing,
            Type::unknown(),
            &policy,
        )?;
        assert_eq!(
            actual.result,
            Some(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert!(actual.boundary(reached).is_some());
        if reached == Event::RhsEntered {
            assert!(actual.boundary(Event::RhsCompleted).is_none());
        }
        actual.assert_restored();
        let retry = infer(
            &db,
            &prepared,
            Fields::EmptySpilled,
            Cache::Existing,
            Type::unknown(),
            &funded(),
        )?;
        assert_eq!(retry.result, Some(AnalysisOutcome::Complete(())));
        assert_eq!(retry.target, Some(Type::bool_literal(true)));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

/// Cancellation after temporary context installation restores the incoming scalar state, cache,
/// and spilled field buffer; a funded retry completes in the same database revision.
#[test]
fn annotated_value_cancellation_after_context_installation_restores_and_retries()
-> anyhow::Result<()> {
    let db = fixture()?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    CANCEL_AT_RHS.set(true);
    let actual = infer(
        &db,
        &prepared,
        Fields::EmptySpilled,
        Cache::Existing,
        Type::unknown(),
        &funded(),
    )?;
    assert_eq!(actual.result, None);
    let Some((rhs, _)) = actual.boundary(Event::RhsEntered) else {
        anyhow::bail!("value did not reach the selected cancellation boundary");
    };
    assert_eq!(rhs.scalar.binding, Some(actual.definition));
    assert_ne!(rhs.scalar.binding, actual.initial.scalar.binding);
    assert_eq!(actual.initial, actual.final_state);
    assert_eq!(actual.initial_fields, actual.final_fields);
    assert_eq!(actual.journal.restored.get(), Some(actual.initial));
    assert!(actual.boundary(Event::Complete).is_none());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let retry = infer(
        &db,
        &prepared,
        Fields::EmptySpilled,
        Cache::Existing,
        Type::unknown(),
        &funded(),
    )?;
    assert_eq!(retry.result, Some(AnalysisOutcome::Complete(())));
    assert_eq!(retry.target, Some(Type::bool_literal(true)));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}
