//! Canonical ParamSpec defaults preserve callable metadata across completion and interruption.

use super::*;
use crate::Program;
use crate::analysis::DeferredInferenceOperation;
use crate::types::infer::deferred_definition_inference_ingredient;
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::infer::type_parameter_header::{
    TypeParameterHeaderInput, infer_type_parameter_header,
};
use crate::types::typevar::default::lazy::LazyDefaultEffects;
use crate::types::typevar::{BindingContext, TypeVarNonce};
use crate::types::{KnownClass, KnownInstanceType, Parameter, Parameters, todo_type};

/// Identifies the owned value present when cancellation is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    Parameter,
    Parameters,
}

/// Observes converter progress without changing an admission or a semantic result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Progress {
    appended: usize,
    constructed: usize,
    remaining_work: Option<usize>,
}

thread_local! {
    static PROGRESS: Cell<Progress> = const { Cell::new(Progress { appended: 0, constructed: 0, remaining_work: None }) };
    static CANCEL_CONVERSION: Cell<Option<Boundary>> = const { Cell::new(None) };
}

/// Records a completed push while the converter still owns its parameter vector.
pub(in crate::types::infer) fn parameter_appended(db: &dyn Db) {
    let mut progress = PROGRESS.get();
    progress.appended += 1;
    PROGRESS.set(progress);
    cancel_at(db, Boundary::Parameter);
}

/// Records the completed Parameters owner before constructing and interning its callable.
pub(in crate::types::infer) fn parameters_constructed(db: &dyn Db) {
    let mut progress = PROGRESS.get();
    progress.constructed += 1;
    progress.remaining_work = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
    PROGRESS.set(progress);
    cancel_at(db, Boundary::Parameters);
}

/// Requests ordinary local cancellation at the selected real converter boundary, once.
fn cancel_at(db: &dyn Db, boundary: Boundary) {
    if CANCEL_CONVERSION.get() == Some(boundary) {
        CANCEL_CONVERSION.set(None);
        db.cancellation_token().cancel();
    }
}

/// Clears observations before an attempt and optionally arms one cancellation request.
fn reset_conversion(cancel: Option<Boundary>) {
    reset(None);
    PROGRESS.set(Progress::default());
    CANCEL_CONVERSION.set(cancel);
}

/// Selects a complete declaration header without evaluating its default expression.
fn default_header<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> (Definition<'db>, TypeVarInstance<'db>) {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .last()
        .unwrap()
        .as_class_def_stmt()
        .unwrap();
    let parameter = class
        .type_params
        .as_deref()
        .unwrap()
        .type_params
        .last()
        .unwrap();
    let ast::TypeParam::ParamSpec(parameter) = parameter else {
        panic!("ParamSpec fixture")
    };
    let definition = prepared
        .semantic_index()
        .expect_single_definition(parameter);
    let header =
        infer_type_parameter_header(db, definition, TypeParameterHeaderInput::from(parameter));
    assert_eq!(header.deferred, Some(definition));
    (definition, header.variable)
}

/// Specifies a valid default expression and its expected complete converted value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultCase {
    List,
    Empty,
    Ellipsis,
    Reference,
}

impl DefaultCase {
    const fn source(self) -> &'static str {
        match self {
            Self::List => "class Marker[**P = [int, str]]: ...\n",
            Self::Empty => "class Marker[**P = []]: ...\n",
            Self::Ellipsis => "class Marker[**P = ...]: ...\n",
            Self::Reference => "class Marker[**P, **Q = P]: ...\n",
        }
    }

    /// Compares every interned signature field with ordinary constructors, or preserves a referenced ParamSpec exactly.
    fn assert_value<'db>(
        self,
        db: &'db TestDb,
        prepared: &PreparedAnalysisFile<'db>,
        value: Type<'db>,
    ) {
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let parameters = match self {
            Self::List => Parameters::standard([
                Parameter::positional_only(None)
                    .with_annotated_type(KnownClass::Int.to_instance(db, &env)),
                Parameter::positional_only(None)
                    .with_annotated_type(KnownClass::Str.to_instance(db, &env)),
            ]),
            Self::Empty => Parameters::standard([]),
            Self::Ellipsis => Parameters::gradual_form(),
            Self::Reference => {
                let class = prepared
                    .parsed_module()
                    .syntax()
                    .body
                    .last()
                    .unwrap()
                    .as_class_def_stmt()
                    .unwrap();
                let ast::TypeParam::ParamSpec(parameter) = class
                    .type_params
                    .as_deref()
                    .unwrap()
                    .type_params
                    .last()
                    .unwrap()
                else {
                    panic!("ParamSpec reference fixture")
                };
                let definition = prepared
                    .semantic_index()
                    .expect_single_definition(parameter);
                let stored = infer_deferred_types(db, definition)
                    .expression_type(parameter.default.as_deref().unwrap());
                assert!(matches!(
                    stored,
                    Type::TypeVar(_) | Type::KnownInstance(KnownInstanceType::TypeVar(_))
                ));
                assert_eq!(value, stored);
                return;
            }
        };
        assert_eq!(value, Type::paramspec_value_callable(db, parameters));
    }
}

/// Cold lazy-default queries preserve callable metadata for constructed callables and the identity
/// of referenced ParamSpecs, and publish reusable canonical results. The ordinary comparison
/// runs on a separate database so it cannot supply cached results to the controlled attempt.
#[test_case::test_case(DefaultCase::List; "list")]
#[test_case::test_case(DefaultCase::Empty; "empty list")]
#[test_case::test_case(DefaultCase::Ellipsis; "ellipsis")]
#[test_case::test_case(DefaultCase::Reference; "paramspec reference")]
fn cold_paramspec_defaults_preserve_canonical_values(case: DefaultCase) {
    let db = fixture(case.source());
    let prepared = prepare(&db);
    let (_, variable) = default_header(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_missing(&db, variable);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset_conversion(None);
    let result = controlled(&prepared, variable, Operation::Raw, &funded());
    let Ok(AnalysisOutcome::Complete(Some(value))) = result else {
        panic!("{result:?}")
    };
    assert_raw_query_ran(&db, variable, &events_db.take_salsa_events());
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            lazy_typevar_default_ingredient(&db),
            variable.as_id()
        )
        .is_ok()
    );
    case.assert_value(&db, &prepared, value);
    assert_cleanup();

    events_db.take_salsa_events();
    reset_conversion(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Raw, &funded()),
        Ok(AnalysisOutcome::Complete(Some(value)))
    );
    assert_eq!(QUERIES.get(), 0);
    assert_eq!(PROGRESS.get(), Progress::default());
    assert_function_query_was_not_run_by_name(
        &db,
        "lazy_default_unchecked",
        Some(variable.as_id()),
        &events_db.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();

    let ordinary_db = fixture(case.source());
    let ordinary_prepared = prepare(&ordinary_db);
    let (_, ordinary_variable) = default_header(&ordinary_db, &ordinary_prepared);
    let ordinary = raw_ordinary(&ordinary_db, ordinary_variable).unwrap();
    case.assert_value(&ordinary_db, &ordinary_prepared, ordinary);
}

/// Invalid non-list defaults reach an unsupported diagnostic before conversion can run.
/// Refusal leaves the raw default and its expression result unpublished for same-revision retry.
#[test]
fn invalid_paramspec_default_preserves_its_diagnostic_refusal() {
    let db = fixture("class Marker[**P = int]: ...\n");
    let prepared = prepare(&db);
    let (definition, variable) = default_header(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        reset_conversion(None);
        assert_eq!(
            controlled(&prepared, variable, Operation::Raw, &funded()),
            Ok(unavailable(OperationId::Deferred(
                DeferredInferenceOperation::ParamSpecDefaultDiagnostic,
            )))
        );
        assert_eq!(PROGRESS.get(), Progress::default());
        assert_missing(&db, variable);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                deferred_definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Requires the default-expression child to have published before testing conversion's failure or retry.
fn assert_deferred_complete(db: &TestDb, definition: Definition<'_>) {
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            deferred_definition_inference_ingredient(db),
            definition.as_id()
        )
        .is_ok()
    );
}

/// Retries on the refused database and verifies reuse of the completed default-expression child.
fn retry_conversion<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    variable: TypeVarInstance<'db>,
) {
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset_conversion(None);
    let result = controlled(prepared, variable, Operation::Raw, &funded());
    let Ok(AnalysisOutcome::Complete(Some(value))) = result else {
        panic!("{result:?}")
    };
    let child = deferred_definition_inference_ingredient(db).database_key_index(definition.as_id());
    assert!(!events_db.take_salsa_events().iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == child)));
    DefaultCase::List.assert_value(db, prepared, value);
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            lazy_typevar_default_ingredient(db),
            variable.as_id()
        )
        .is_ok()
    );
    assert_cleanup();
}

/// Measures work through completed parameter construction on a separate cold database.
fn constructed_work() -> usize {
    let db = fixture(DefaultCase::List.source());
    let prepared = prepare(&db);
    let (_, variable) = default_header(&db, &prepared);
    reset_conversion(None);
    assert!(matches!(
        controlled(&prepared, variable, Operation::Raw, &funded()),
        Ok(AnalysisOutcome::Complete(Some(_)))
    ));
    let progress = PROGRESS.get();
    assert_eq!((progress.appended, progress.constructed), (2, 1));
    assert_cleanup();
    funded().semantic_work_limit - progress.remaining_work.unwrap()
}

/// Observes whether a fresh cold conversion constructs its parameters within a byte allowance.
fn reaches_parameters(bytes: usize) -> bool {
    let db = fixture(DefaultCase::List.source());
    let prepared = prepare(&db);
    let (_, variable) = default_header(&db, &prepared);
    reset_conversion(None);
    let result = controlled(
        &prepared,
        variable,
        Operation::Raw,
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    );
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(_))
                | Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                })
        ),
        "{result:?}"
    );
    assert_cleanup();
    PROGRESS.get().constructed == 1
}

/// Finds the first byte allowance that reaches constructed parameters, using separate databases.
fn constructed_bytes() -> usize {
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_parameters(high));
    while low < high {
        let middle = low + (high - low) / 2;
        if reaches_parameters(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    high
}

/// Selects the independently limited resource while leaving the other funded limit unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

/// Refusal after parameter construction keeps the raw default unpublished and permits same-revision retry.
/// The inferred default expression remains a completed canonical dependency of that retry.
#[test_case::test_case(Resource::Work; "work")]
#[test_case::test_case(Resource::Bytes; "bytes")]
fn constructed_paramspec_refusal_preserves_child_and_retries(resource: Resource) {
    let (policy, reason) = match resource {
        Resource::Work => (
            AnalysisPolicy {
                semantic_work_limit: constructed_work(),
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        Resource::Bytes => (
            AnalysisPolicy {
                requested_bytes_limit: constructed_bytes(),
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    };
    let db = fixture(DefaultCase::List.source());
    let prepared = prepare(&db);
    let (definition, variable) = default_header(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    reset_conversion(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Raw, &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: ()
        })
    );
    assert_eq!(
        (PROGRESS.get().appended, PROGRESS.get().constructed),
        (2, 1)
    );
    assert_deferred_complete(&db, definition);
    assert_missing(&db, variable);
    assert_cleanup();
    retry_conversion(&db, &prepared, definition, variable);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Cancellation with a populated parameter owner drains the attempt and allows same-revision retry.
/// Salsa may publish the completed raw default before delivering cancellation; any surviving memo must be complete.
#[test_case::test_case(Boundary::Parameter; "parameter vector")]
#[test_case::test_case(Boundary::Parameters; "parameters owner")]
fn paramspec_conversion_cancellation_retries(boundary: Boundary) {
    let db = fixture(DefaultCase::List.source());
    let prepared = prepare(&db);
    let (definition, variable) = default_header(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    reset_conversion(Some(boundary));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, variable, Operation::Raw, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(PROGRESS.get().appended >= 1);
    if boundary == Boundary::Parameters {
        assert_eq!(PROGRESS.get().constructed, 1);
    }
    assert_eq!(CANCEL_CONVERSION.get(), None);
    assert_deferred_complete(&db, definition);
    match FinalSourceMemo::certify(
        &db as &dyn Db,
        lazy_typevar_default_ingredient(&db),
        variable.as_id(),
    ) {
        Ok(_) => {
            DefaultCase::List.assert_value(&db, &prepared, raw_ordinary(&db, variable).unwrap())
        }
        Err(error) => assert_eq!(error, FinalSourceError::MissingMemo),
    }
    assert_cleanup();
    retry_conversion(&db, &prepared, definition, variable);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Converts an already inferred default through the production controlled conversion effect.
#[derive(Clone, Copy, Debug)]
struct ConversionRequest<'db>(Type<'db>);

impl<'db> MemberOperation<'db> for ConversionRequest<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        LazyDefaultEffects::paramspec_value(&SourceEffects::new(access, program), self.0).await
    }
}

/// Selects inferred forms whose conversion is independent of default-expression syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InferredDefault {
    Any,
    Unknown,
    Todo,
    VariableTuple,
    Nominal,
    Bound(TypeVarKind),
    Unbound(TypeVarKind),
}

/// ParamSpecs retain their identities; other inferred forms recover as unknown parameters, with Todo preserved.
/// These calls exercise the production conversion effect directly, including forms that invalid syntax can produce.
#[test_case::test_case(InferredDefault::Any; "any")]
#[test_case::test_case(InferredDefault::Unknown; "unknown")]
#[test_case::test_case(InferredDefault::Todo; "todo")]
#[test_case::test_case(InferredDefault::VariableTuple; "variable tuple")]
#[test_case::test_case(InferredDefault::Nominal; "nominal non-list")]
#[test_case::test_case(InferredDefault::Bound(TypeVarKind::LegacyParamSpec); "bound paramspec")]
#[test_case::test_case(InferredDefault::Unbound(TypeVarKind::LegacyParamSpec); "unbound paramspec")]
#[test_case::test_case(InferredDefault::Bound(TypeVarKind::LegacyTypeVar); "bound typevar")]
#[test_case::test_case(InferredDefault::Unbound(TypeVarKind::LegacyTypeVar); "unbound typevar")]
fn inferred_default_conversion_preserves_identity_or_recovers(case: InferredDefault) {
    let db = fixture("class Marker: ...\n");
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = match case {
        InferredDefault::Any => Type::any(),
        InferredDefault::Unknown => Type::unknown(),
        InferredDefault::Todo => todo_type!("ParamSpec default"),
        InferredDefault::VariableTuple => Type::homogeneous_tuple(&db, &env, Type::int_literal(1)),
        InferredDefault::Nominal => KnownClass::Int.to_instance(&db, &env),
        InferredDefault::Bound(kind) | InferredDefault::Unbound(kind) => {
            let variable = TypeVarInstance::new(
                &db,
                TypeVarIdentity::new(&db, Name::new_static("P"), None, kind),
                None,
                None,
                None,
            );
            if matches!(case, InferredDefault::Bound(_)) {
                Type::TypeVar(BoundTypeVarInstance::new(
                    &db,
                    variable,
                    BindingContext::Synthetic(env.program(&db)),
                    None,
                    TypeVarNonce::NONE,
                ))
            } else {
                Type::KnownInstance(KnownInstanceType::TypeVar(variable))
            }
        }
    };
    reset_conversion(None);
    let actual = controlled_member_operation(&prepared, ConversionRequest(input), &funded());
    assert_cleanup();
    let expected = match case {
        InferredDefault::Bound(kind) | InferredDefault::Unbound(kind) if kind.is_paramspec() => {
            input
        }
        InferredDefault::Todo => Type::paramspec_value_callable(&db, Parameters::todo()),
        InferredDefault::Any
        | InferredDefault::Unknown
        | InferredDefault::VariableTuple
        | InferredDefault::Nominal
        | InferredDefault::Bound(_)
        | InferredDefault::Unbound(_) => Type::paramspec_value_callable(&db, Parameters::unknown()),
    };
    assert_eq!(actual, Ok(AnalysisOutcome::Complete(expected)));
}
