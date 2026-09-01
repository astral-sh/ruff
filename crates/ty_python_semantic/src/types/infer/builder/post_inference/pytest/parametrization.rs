use crate::types::{
    KnownClass,
    infer::{TypeInferenceBuilder, builder::post_inference::pytest::argnames::KnownArgnames},
};
use itertools::Itertools;
use ruff_python_ast as ast;

/// Representation of a `pytest.mark.parametrize` call.
/// Only calls that will be checked are recorded.
#[expect(dead_code)]
pub(crate) struct Parametrization<'ast> {
    // If the argnames are unknown, no checking occurs, so we discard it.
    argnames: KnownArgnames,
    argvalues: &'ast ast::Expr,
}

impl Parametrization<'_> {
    pub(crate) fn argnames(&self) -> &KnownArgnames {
        &self.argnames
    }
}

impl<'ast> TypeInferenceBuilder<'_, 'ast> {
    pub(crate) fn build_parametrizations(
        &self,
        decorators: &'ast [ast::Decorator],
    ) -> Vec<Parametrization<'ast>> {
        let parametrizations = decorators
            .iter()
            .filter_map(|decorator| self.build_parametrization(decorator));
        parametrizations.collect_vec()
    }

    pub(crate) fn build_parametrization(
        &self,
        decorator: &'ast ast::Decorator,
    ) -> Option<Parametrization<'ast>> {
        if let Some(decorator_call) = decorator.expression.as_call_expr()
            && self.expression_type(&decorator_call.func)
                == KnownClass::PytestParametrizeMarkDecorator
                    .to_instance(self.db(), self.program_environment())
            && let Some(argnames) = decorator_call.arguments.find_argument("argnames", 0)
            && let Some(argnames) = self.parse_argnames_expression(argnames.value())
            && let Some(argvalues) = decorator_call.arguments.find_argument("argvalues", 1)
            // If there are extra arguments, they are ignored in case of edge cases like indirect fixtures.
            // These edge cases may be handled in the future.
            && decorator_call.arguments.len() == 2
        {
            let argvalues = argvalues.value();
            {
                Some(Parametrization {
                    argnames,
                    argvalues,
                })
            }
        } else {
            None
        }
    }
}
