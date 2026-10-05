//! Callable defaults use the ordinary binding algorithm; function literals retain their refusal.

use super::*;
use crate::Program;
use crate::types::function::source::FunctionSignatureEffects;
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::mapping::source::observations::RootCacheState;
use crate::types::{MappingOperation, Parameter, Parameters};

/// Selects a nominal or callable value supplied to the binding operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultInput {
    Nominal,
    Callable,
}

/// Binding a nominal default preserves it, and binding a callable visits its parameter annotations.
#[test_case::test_case(DefaultInput::Nominal; "nominal")]
#[test_case::test_case(DefaultInput::Callable; "callable")]
fn ordinary_default_binding_preserves_complete_values(
    input_kind: DefaultInput,
) -> anyhow::Result<()> {
    let db = fixture()?;
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let binding = BindingContext::Synthetic(prepared.program_file().program(&db));
    let variable = raw(&db, TypeVarKind::LegacyTypeVar, "T");
    let callable = input_kind == DefaultInput::Callable;
    let input = if callable {
        Type::paramspec_value_callable(
            &db,
            Parameters::standard([Parameter::positional_only(None)
                .with_annotated_type(Type::KnownInstance(KnownInstanceType::TypeVar(variable)))]),
        )
    } else {
        KnownClass::Int.to_instance(&db, &env)
    };
    reset(false);
    let actual = controlled_mapping(&prepared, input, binding, &funded());
    let expected = input.apply_type_mapping(
        &db,
        &env,
        &TypeMapping::BindLegacyTypevars(binding),
        TypeContext::default(),
    );
    assert_eq!(actual, Ok(AnalysisOutcome::Complete(expected)));
    if callable {
        let bound = BoundTypeVarInstance::new(&db, variable, binding, None, TypeVarNonce::NONE);
        assert_eq!(
            expected,
            Type::paramspec_value_callable(
                &db,
                Parameters::standard([
                    Parameter::positional_only(None).with_annotated_type(Type::TypeVar(bound)),
                ])
            )
        );
        let caches = mapping_observations::root_cache_snapshot();
        assert_eq!(caches.count, 1);
        assert_eq!(
            caches.observations[0].unwrap().state,
            RootCacheState::Present
        );
    } else {
        assert_eq!(expected, input);
    }
    assert_mapping(binding, usize::from(callable));
    assert_cleanup();
    Ok(())
}

/// Selects the mapping operation whose function-literal handling remains unsupported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FunctionMode {
    Binding,
    Partial,
}

/// Obtains a real function binding through the canonical provider before requesting its mapping.
#[derive(Clone, Copy, Debug)]
struct FunctionRequest<'db> {
    definition: Definition<'db>,
    context: GenericContext<'db>,
    mode: FunctionMode,
}

impl<'db> MemberOperation<'db> for FunctionRequest<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let ty =
            FunctionSignatureEffects::binding_type(&effects, access.db(), self.definition).await?;
        let env = ProgramEnvironment::from_program(program);
        match self.mode {
            FunctionMode::Binding => {
                effects
                    .bind_legacy_typevars(ty, &env, BindingContext::Synthetic(program))
                    .await
            }
            FunctionMode::Partial => {
                let buffer = access
                    .resources()
                    .default_arguments(access.endpoint(), 0)
                    .await?;
                let prefix = effects
                    .local_with_fixed_transfers(2, 0, || buffer.prefix())
                    .await?;
                effects
                    .apply_partial_specialization(ty, &env, self.context, prefix, None)
                    .await
            }
        }
    }
}

/// Enabling callable-default mappings does not enable function literals or cache a result for them.
/// Both attempts use the same database and revision; the first obtains the function from a cold provider.
#[test_case::test_case(FunctionMode::Binding; "binding")]
#[test_case::test_case(FunctionMode::Partial; "partial")]
fn function_literal_mapping_keeps_its_precise_refusal(mode: FunctionMode) -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("src/main.pyi", "def callback(value): ...\n")?;
    let prepared = bound_defaults::prepare_fixture(&db);
    let function = prepared.parsed_module().syntax().body[0]
        .as_function_def_stmt()
        .unwrap();
    let definition = prepared.semantic_index().expect_single_definition(function);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let context = GenericContext::from_typevar_instances(&db, &env, []);
    let request = FunctionRequest {
        definition,
        context,
        mode,
    };
    let operation = match mode {
        FunctionMode::Binding => OperationId::LegacyTypeVarBinding(MaterializationOperation::Leaf(
            MappingOperation::Function,
        )),
        FunctionMode::Partial => {
            OperationId::Specialization(MaterializationOperation::Leaf(MappingOperation::Function))
        }
    };
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        reset(false);
        assert_eq!(
            controlled_member_operation(&prepared, request, &funded()),
            Ok(unavailable(operation))
        );
        let caches = mapping_observations::root_cache_snapshot();
        assert_eq!(caches.count, 1);
        assert_eq!(
            caches.observations[0].unwrap().state,
            RootCacheState::NoTransformer
        );
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}
