use crate::types::{
    Parameter, Parameters, Type,
    diagnostic::{PYTEST_TEST_ARGUMENT_WRONG_KIND, PYTEST_TEST_OPTIONAL_ARGUMENT},
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
pub(crate) struct Request<'db, 'ast> {
    name: &'ast ast::Identifier,
    expected_type: Type<'db>,
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
            self.build_requests_from_parameters(&fn_def.parameters, signature.parameters())
        } else {
            None
        }
    }

    fn build_requests_from_parameters(
        &self,
        ast_parameters: &'ast ast::Parameters,
        ty_parameters: &Parameters<'db>,
    ) -> Option<Vec<Request<'db, 'ast>>> {
        // Collect to display all errors.
        let checked_requests = ast_parameters
            .iter()
            .zip_eq(ty_parameters)
            .map(|(ast_parameter, ty_parameter)| {
                self.request_from_parameter(ast_parameter.as_parameter(), ty_parameter)
            })
            .collect_vec();
        Option::from_iter(checked_requests)
    }

    fn request_from_parameter(
        &self,
        ast_parameter: &'ast ast::Parameter,
        ty_parameter: &Parameter<'db>,
    ) -> Option<Request<'db, 'ast>> {
        self.check_parameter_kind(ty_parameter, ast_parameter.range())
            .then(move || Request {
                // It is a keyword argument, so the name will exist.
                name: ast_parameter.name(),
                expected_type: ty_parameter.annotated_type(),
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
                .report_lint(&PYTEST_TEST_ARGUMENT_WRONG_KIND, range)
            {
                // The display name exists because we are parsing a signature from a real function.
                builder.into_diagnostic(format!("Pytest tests only accept keyword arguments. `{}` is a {parameter_kind} argument.", parameter.display_name().unwrap()) );
            }
            false
        } else {
            // Optional arguments are ignored, so we can continue checking.
            self.check_optional_argument(parameter, range);
            true
        }
    }

    /// Check whether argument is optional.
    /// This generates a warning if it is.
    fn check_optional_argument(&self, parameter: &Parameter<'_>, range: TextRange) {
        if parameter.default_type().is_some() {
            if let Some(builder) = self
                .context
                .report_lint(&PYTEST_TEST_OPTIONAL_ARGUMENT, range)
            {
                builder.into_diagnostic(format!(
                    "Pytest tests ignore optional arguments. `{}` has a default value.",
                    parameter.display_name().unwrap()
                ));
            }
        }
    }
}
