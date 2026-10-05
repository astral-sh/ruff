//! Partial substitution maps complete callable values while refusing unsupported signature effects.

use super::*;
use crate::types::mapping::source::observations::RootCacheState;
use crate::types::signatures::ConcatenateTail;
use crate::types::{Parameter, Parameters, Signature};

/// Fixed callable parameters use the retained argument prefix and retain their ParamSpec-value metadata.
#[test]
fn partial_callable_default_preserves_parameter_metadata() {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let variable = variable(&db, &env, "T");
    let context = GenericContext::from_typevar_instances(&db, &env, [variable]);
    let prefix = [Type::int_literal(1)];
    let input = Type::paramspec_value_callable(
        &db,
        Parameters::standard([
            Parameter::positional_only(None).with_annotated_type(Type::TypeVar(variable))
        ]),
    );
    let action = Action::Map {
        ty: input,
        context,
        prefix: &prefix,
        skip: None,
    };
    reset();
    let result = controlled(&prepared, action, &funded());
    let expected = ordinary_mapping(&db, &env, action);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Complete(Output::Mapped(expected)))
    );
    assert_eq!(
        expected,
        Type::paramspec_value_callable(
            &db,
            Parameters::standard(
                [Parameter::positional_only(None).with_annotated_type(prefix[0]),]
            )
        )
    );
    let caches = mapping_observations::root_cache_snapshot();
    assert_eq!(caches.count, 1);
    assert_eq!(
        caches.observations[0].unwrap().state,
        RootCacheState::Present
    );
    assert_mapping(context, 1, None, 1);
    assert_cleanup();
}

/// Selects a signature effect that is not implemented by partial callable substitution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnsupportedSignature {
    ParamSpec,
    Concatenate,
    GenericContext,
}

/// Unsupported signature effects refuse precisely, publish no partial mapping, and remain retryable.
#[test_case::test_case(UnsupportedSignature::ParamSpec; "paramspec tail")]
#[test_case::test_case(UnsupportedSignature::Concatenate; "concatenate tail")]
#[test_case::test_case(UnsupportedSignature::GenericContext; "generic context")]
fn partial_callable_effect_refusal_leaves_no_cached_result(unsupported: UnsupportedSignature) {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let raw = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(
            &db,
            Name::new_static("P"),
            None,
            TypeVarKind::LegacyParamSpec,
        ),
        None,
        None,
        None,
    );
    let variable = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    );
    let context = GenericContext::from_typevar_instances(&db, &env, [variable]);
    let prefix = [Type::paramspec_value_callable(
        &db,
        Parameters::standard([]),
    )];
    let (signature, operation, state) = match unsupported {
        UnsupportedSignature::ParamSpec => (
            Signature::new(Parameters::paramspec(&db, variable), Type::unknown()),
            MaterializationOperation::Leaf(MappingOperation::ParamSpec),
            RootCacheState::NoTransformer,
        ),
        UnsupportedSignature::Concatenate => (
            Signature::new(
                Parameters::concatenate(
                    &db,
                    vec![
                        Parameter::positional_only(None).with_annotated_type(Type::int_literal(1)),
                    ],
                    ConcatenateTail::ParamSpec(variable),
                ),
                Type::unknown(),
            ),
            MaterializationOperation::Leaf(MappingOperation::ParamSpec),
            RootCacheState::NoTransformer,
        ),
        UnsupportedSignature::GenericContext => (
            Signature::new_generic(Some(context), Parameters::standard([]), Type::unknown()),
            MaterializationOperation::Mode,
            RootCacheState::Absent,
        ),
    };
    let input = Type::single_callable(&db, signature);
    let action = Action::Map {
        ty: input,
        context,
        prefix: &prefix,
        skip: None,
    };
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        reset();
        assert_eq!(
            controlled(&prepared, action, &funded()),
            Ok(unavailable(OperationId::Specialization(operation)))
        );
        let caches = mapping_observations::root_cache_snapshot();
        assert_eq!(caches.count, 1);
        let root = caches.observations[0].unwrap();
        assert_eq!(
            root.callable,
            input.as_callable().map(|callable| callable.as_id())
        );
        assert_eq!(root.state, state);
        assert_mapping(context, 1, None, 0);
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
