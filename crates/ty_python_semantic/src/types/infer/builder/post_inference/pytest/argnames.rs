use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};

use crate::types::{diagnostic::PYTEST_INVALID_ARGNAMES_LITERAL, infer::TypeInferenceBuilder};

impl TypeInferenceBuilder<'_, '_> {
    pub(crate) fn parse_argnames_expression(&self, argnames_argument: &ast::Expr) {
        if let Some(literal) = self.expression_type(argnames_argument).as_string_literal() {
            self.parse_argnames_string(literal.value(self.db()), argnames_argument.range());
        }
    }

    fn parse_argnames_string(&self, value: &str, range: TextRange) {
        if value.is_empty() {
            if let Some(builder) = self
                .context
                .report_lint(&PYTEST_INVALID_ARGNAMES_LITERAL, range)
            {
                builder.into_diagnostic("Argnames is an empty string.");
            }
        }
    }
}
