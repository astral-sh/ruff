use crate::types::{
    GenericContext, Parameter, Parameters, Type,
    diagnostic::{PYTEST_TEST_OPTIONAL_PARAMETER, PYTEST_TEST_PARAMETER_WRONG_KIND},
    infer::TypeInferenceBuilder,
};
use itertools::Itertools;
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};

/// A request can either come from an argument or a fixture.
/// For now, only argument requests are handled.
/// They are resolved by `pytest.mark.parametrize` only.
///
/// For example,
/// ```python
/// import pytest
/// from pathlib import Path
///
/// @pytest.fixture
/// def rng(seed: int) -> Rng:
///     """This fixture has one request: `seed`. The expected type is `int`."""
///     return Rng.from_seed(seed)
///
/// @pytest.mark.parametrize("seed", [0, 1, 2])
/// def test_rng_next(rng: Rng, tmp_path: Path) -> None:
///     """This test has two direct requests: `rng` and `tmp_path`.
///     However, it also makes an indirect request for `seed` via the `rng` fixture.
///     """
///     ...
/// ```
#[derive(Clone, Copy)]
pub(crate) struct Request<'db, 'ast> {
    name: &'ast ast::Identifier,
    expected_type: Type<'db>,
    generic_context: Option<GenericContext<'db>>,
}

impl<'db, 'ast> Request<'db, 'ast> {
    pub(crate) fn name(&self) -> &'ast ast::Identifier {
        self.name
    }

    pub(crate) fn ty(&self) -> Type<'db> {
        self.expected_type
    }

    pub(crate) fn generic_context(&self) -> Option<GenericContext<'db>> {
        self.generic_context
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn build_requests(
        &self,
        fn_def: &'ast ast::StmtFunctionDef,
        ty: &Type<'db>,
    ) -> Option<Vec<Request<'db, 'ast>>> {
        let db = self.db();
        if let Some(ty) = ty.as_function_literal() {
            let signature = ty.literal(db).last_definition.signature(db);
            Some(self.build_requests_from_parameters(
                &fn_def.parameters,
                signature.parameters(),
                signature.generic_context,
            ))
        } else {
            None
        }
    }

    fn build_requests_from_parameters(
        &self,
        ast_parameters: &'ast ast::Parameters,
        ty_parameters: &Parameters<'db>,
        generic_context: Option<GenericContext<'db>>,
    ) -> Vec<Request<'db, 'ast>> {
        // Collect to display all errors.
        ast_parameters
            .iter()
            .zip_eq(ty_parameters)
            .filter_map(|(ast_parameter, ty_parameter)| {
                self.request_from_parameter(ast_parameter, ty_parameter, generic_context)
            })
            .collect_vec()
    }

    fn request_from_parameter(
        &self,
        ast_parameter: ast::AnyParameterRef<'ast>,
        ty_parameter: &Parameter<'db>,
        generic_context: Option<GenericContext<'db>>,
    ) -> Option<Request<'db, 'ast>> {
        self.check_parameter_kind(ty_parameter, ast_parameter.range())
            .then(move || Request {
                // It is a keyword argument, so the name will exist.
                name: ast_parameter.name(),
                expected_type: ty_parameter.annotated_type(),
                generic_context,
            })
    }

    /// Check that a parameter can be used as a keyword argument.
    /// If not, generate an error.
    fn check_parameter_kind(&self, parameter: &Parameter, range: TextRange) -> bool {
        let parameter_error_kind = if parameter.is_positional_only() {
            Some("positional only")
        } else if parameter.is_variadic() {
            Some("variadic positional")
        } else if parameter.is_keyword_variadic() {
            Some("variadic keyword")
        } else {
            None
        };
        if let Some(parameter_kind) = parameter_error_kind {
            if let Some(builder) = self
                .context
                .report_lint(&PYTEST_TEST_PARAMETER_WRONG_KIND, range)
            {
                // The display name exists because we are parsing a signature from a real function.
                builder.into_diagnostic(format!("Pytest tests only accept keyword arguments. `{}` is a {parameter_kind} argument.", parameter.display_name().unwrap()) );
            }
            false
        } else {
            // Optional arguments are ignored.
            !self.check_optional_argument(parameter, range)
        }
    }

    /// Check whether argument is optional.
    /// This generates a warning if it is.
    /// Returns whether this is an optional argument.
    fn check_optional_argument(&self, parameter: &Parameter<'_>, range: TextRange) -> bool {
        let has_default_value = parameter.default_type().is_some();
        if has_default_value {
            if let Some(builder) = self
                .context
                .report_lint(&PYTEST_TEST_OPTIONAL_PARAMETER, range)
            {
                builder.into_diagnostic(format!(
                    "Pytest tests ignore optional arguments. `{}` has a default value.",
                    parameter.display_name().unwrap()
                ));
            }
        }
        has_default_value
    }
}
