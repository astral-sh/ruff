//! Controls canonical function identity, stored payload preservation, and independent resource
//! refusal for the controlled descriptor-kind update. Python-visible binding semantics live in
//! mdtests; these controls inspect identities and admission outcomes that mdtests cannot observe.

use ruff_python_ast::name::Name;

use super::function_decorators::{database, definition, prepared};
use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::function::descriptor::FunctionTypeDescriptorEffects;
use crate::types::signatures::{Parameter, Parameters};
use crate::types::{CallableSignature, CallableType, FunctionType, Signature, binding_type};

const SOURCE: &str = "def target(): ...\n";
const OVERRIDE: CallableTypeKind = CallableTypeKind::ClassMethodLike;

/// Observes entry and successful return of the full descriptor effect without changing its budget.
#[derive(Debug, Default)]
struct Observation {
    entered: Cell<bool>,
    completed: Cell<bool>,
}

/// Applies one descriptor kind to an existing function through the controlled effect.
#[derive(Clone, Copy, Debug)]
struct Request<'a, 'db> {
    function: FunctionType<'db>,
    kind: CallableTypeKind,
    observation: &'a Observation,
}

impl<'db> MemberOperation<'db> for Request<'_, 'db> {
    type Output = FunctionType<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        self.observation.entered.set(true);
        let value = FunctionTypeDescriptorEffects::with_kind(
            &SourceEffects::new(access, program),
            self.function,
            self.kind,
        )
        .await?;
        self.observation.completed.set(true);
        Ok(value)
    }
}

/// Obtains the fixture's declared function outside the controlled descriptor request.
fn declared_function<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> anyhow::Result<FunctionType<'db>> {
    binding_type(db, definition(prepared))
        .as_function_literal()
        .ok_or_else(|| anyhow::anyhow!("fixture definition is not a function"))
}

/// Gives a function two distinct stored overloads and two ordered implementation callables.
/// Defaults, source-overload indices and receiver constraints make loss of signature details visible.
fn modified_function<'db>(db: &'db TestDb, function: FunctionType<'db>) -> FunctionType<'db> {
    let first = Signature::new(
        Parameters::standard([Parameter::positional_or_keyword(Name::new("first"))
            .with_default_type(Type::bool_literal(true))]),
        Type::bool_literal(false),
    )
    .with_source_overload_index(Some(2))
    .with_probe_receiver_constraints(OwnedConstraintSet::always());
    let second = Signature::new(
        Parameters::standard([Parameter::positional_or_keyword(Name::new("second"))
            .with_default_type(Type::int_literal(7))]),
        Type::int_literal(11),
    )
    .with_source_overload_index(Some(5))
    .with_probe_receiver_constraints(OwnedConstraintSet::default());
    let first_callable = CallableType::single(db, first.clone());
    let second_callable = CallableType::single(db, second.clone());
    function.with_probe_updated_signatures(
        db,
        CallableSignature::from_overloads([first, second]),
        Box::from([second_callable, first_callable]),
    )
}

/// Runs one fully funded update and checks its explicit reads and invocation cleanup.
fn update<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    function: FunctionType<'db>,
    kind: CallableTypeKind,
) -> anyhow::Result<FunctionType<'db>> {
    let observation = Observation::default();
    observations::reset(None);
    let captured = capture(db, || {
        controlled_member_operation(
            prepared,
            Request {
                function,
                kind,
                observation: &observation,
            },
            &funded(),
        )
    })
    .map_err(|error| anyhow::anyhow!("descriptor read capture: {error:?}"))?;
    assert_eq!(
        captured.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    );
    let Ok(AnalysisOutcome::Complete(value)) = captured.value else {
        anyhow::bail!("funded descriptor update: {:?}", captured.value);
    };
    assert!(observation.entered.get());
    assert!(observation.completed.get());
    assert_no_active_attempt();
    Ok(value)
}

/// Applying a distinct kind creates its canonical override; restoring the declaration's kind
/// returns the original function identity with no stored override.
#[test]
fn descriptor_update_restores_declared_identity() -> anyhow::Result<()> {
    let db = database(SOURCE);
    let prepared = prepared(&db);
    let original = declared_function(&db, &prepared)?;
    let overridden = update(&db, &prepared, original, OVERRIDE)?;
    assert_eq!(
        overridden,
        original.probe_descriptor_kind_oracle(&db, OVERRIDE)
    );
    assert_ne!(overridden, original);
    assert_eq!(overridden.descriptor_kind(&db), Some(OVERRIDE));

    let restored = update(&db, &prepared, overridden, CallableTypeKind::FunctionLike)?;
    assert_eq!(
        restored,
        overridden.probe_descriptor_kind_oracle(&db, CallableTypeKind::FunctionLike),
    );
    assert_eq!(restored, original);
    assert_eq!(restored.descriptor_kind(&db), None);
    Ok(())
}

/// Both override and restoration retain the stored overload order, defaults, receiver constraints
/// and implementation-callable order instead of rebuilding signatures from the source declaration.
#[test]
fn descriptor_update_preserves_modified_payload() -> anyhow::Result<()> {
    let db = database(SOURCE);
    let prepared = prepared(&db);
    let original = declared_function(&db, &prepared)?;
    let modified = modified_function(&db, original);
    let expected_signature = modified.updated_signature(&db);
    let expected_callables = modified.updated_implementation_callables(&db);
    assert!(expected_signature.is_some());
    assert!(expected_callables.is_some());
    assert_ne!(modified, original);

    let overridden = update(&db, &prepared, modified, OVERRIDE)?;
    assert_eq!(
        overridden,
        modified.probe_descriptor_kind_oracle(&db, OVERRIDE)
    );
    assert_eq!(overridden.updated_signature(&db), expected_signature);
    assert_eq!(
        overridden.updated_implementation_callables(&db),
        expected_callables
    );

    let restored = update(&db, &prepared, overridden, CallableTypeKind::FunctionLike)?;
    assert_eq!(restored, modified);
    assert_eq!(restored.updated_signature(&db), expected_signature);
    assert_eq!(
        restored.updated_implementation_callables(&db),
        expected_callables
    );
    assert_eq!(restored.descriptor_kind(&db), None);
    Ok(())
}

/// Selects the only resource allowance reduced during a refusal control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Checks whether the descriptor effect returns successfully under one allowance.
/// Each request uses a fresh database and the `modified_function` fixture.
fn effect_returns(resource: Resource, limit: usize) -> anyhow::Result<bool> {
    let db = database(SOURCE);
    let prepared = prepared(&db);
    let function = modified_function(&db, declared_function(&db, &prepared)?);
    let observation = Observation::default();
    observations::reset(None);
    let result = controlled_member_operation(
        &prepared,
        Request {
            function,
            kind: OVERRIDE,
            observation: &observation,
        },
        &resource.policy(limit),
    );
    assert_no_active_attempt();
    match result {
        Ok(AnalysisOutcome::Complete(_)) => {
            assert!(observation.completed.get());
            Ok(true)
        }
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: (),
        }) => {
            assert_eq!(reason, resource.reason());
            Ok(observation.completed.get())
        }
        Err(error) => anyhow::bail!("descriptor threshold search: {error:?}"),
    }
}

/// Reducing work or bytes independently refuses a real descriptor request, drains its invocation,
/// and permits a fully funded retry with the same input and database revision. Fresh databases find
/// the effect-return threshold; the observation identifies the whole effect, not an internal mutation.
#[test_case::test_case(Resource::Work; "work")]
#[test_case::test_case(Resource::Bytes; "bytes")]
fn descriptor_update_refusal_allows_same_revision_retry(resource: Resource) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(effect_returns(resource, high)?);
    assert!(!effect_returns(resource, low)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if effect_returns(resource, middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }

    let db = database(SOURCE);
    let prepared = prepared(&db);
    let function = modified_function(&db, declared_function(&db, &prepared)?);
    let revision = salsa::plumbing::current_revision(&db);
    let observation = Observation::default();
    observations::reset(None);
    let result = controlled_member_operation(
        &prepared,
        Request {
            function,
            kind: OVERRIDE,
            observation: &observation,
        },
        &resource.policy(low),
    );
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: (),
        }),
    );
    assert!(observation.entered.get());
    assert!(!observation.completed.get());
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();

    let retried = update(&db, &prepared, function, OVERRIDE)?;
    assert_eq!(
        retried,
        function.probe_descriptor_kind_oracle(&db, OVERRIDE)
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}
