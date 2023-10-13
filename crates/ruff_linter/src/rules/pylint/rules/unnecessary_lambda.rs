use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, visitor, Arguments, Expr, ExprLambda, Parameter, ParameterWithDefault,
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lambdas whose body is a function call on the same arguments as the lambda itself.
///
/// ## Why is this bad?
/// Such lambda expressions are in all but a few cases replaceable with the function being called
/// in the body of the lambda.
///
/// ## Example
/// ```python
/// df.apply(lambda x: str(x))
/// ```
///
/// Use instead:
/// ```python
/// df.apply(str)
/// ```

#[violation]
pub struct UnnecessaryLambda;

impl Violation for UnnecessaryLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Lambda may not be necessary")
    }
}

/// Identify all `Expr::Name` nodes in an AST.
struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    names: Vec<&'a ast::ExprName>,
}

impl<'a, 'b> Visitor<'b> for NameFinder<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(expr_name) = expr {
            self.names.push(expr_name);
        }
        visitor::walk_expr(self, expr);
    }
}

/// PLW0108
pub(crate) fn unnecessary_lambda(checker: &mut Checker, lambda: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
    } = lambda;
    // At least one the parameters of the lambda include a default value. We can't know if the
    // defaults provided by the lambda are the same as the defaults provided by the function
    // being called.
    if parameters.as_ref().map_or(false, |parameters| {
        parameters
            .args
            .iter()
            .any(|ParameterWithDefault { default, .. }| default.is_some())
    }) {
        return;
    }
    if let Expr::Call(ast::ExprCall {
        arguments, func, ..
    }) = body.as_ref()
    {
        let Arguments { args, keywords, .. } = arguments;

        // don't check chained calls
        if let Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() {
            if let Expr::Call(_) = value.as_ref() {
                return;
            }
        }

        let call_starargs: Vec<&Expr> = args
            .iter()
            .filter_map(|arg| {
                if let Expr::Starred(ast::ExprStarred { value, .. }) = arg {
                    Some(value.as_ref())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let call_kwargs: Vec<&Expr> = keywords.iter().map(|kw| &kw.value).collect::<Vec<_>>();

        let call_ordinary_args: Vec<&Expr> = args
            .iter()
            .filter(|arg| !matches!(arg, Expr::Starred(_)))
            .collect::<Vec<_>>();

        if let Some(parameters) = parameters.as_ref() {
            if let Some(kwarg) = parameters.kwarg.as_ref() {
                if call_kwargs.is_empty()
                    || call_kwargs.iter().any(|kw| {
                        if let Expr::Name(ast::ExprName { id, .. }) = kw {
                            id.as_str() != kwarg.name.as_str()
                        } else {
                            true
                        }
                    })
                {
                    return;
                }
            } else if !call_kwargs.is_empty() {
                return;
            }
            if let Some(vararg) = parameters.vararg.as_ref() {
                if call_starargs.is_empty()
                    || call_starargs.iter().any(|arg| {
                        if let Expr::Name(ast::ExprName { id, .. }) = arg {
                            id.as_str() != vararg.name.as_str()
                        } else {
                            true
                        }
                    })
                {
                    return;
                }
            } else if !call_starargs.is_empty() {
                return;
            }

            let lambda_ordinary_params: Vec<&Parameter> = parameters
                .args
                .iter()
                .map(|ParameterWithDefault { parameter, .. }| parameter)
                .collect::<Vec<_>>();

            if call_ordinary_args.len() != lambda_ordinary_params.len() {
                return;
            }

            let params_args = lambda_ordinary_params
                .iter()
                .zip(call_ordinary_args.iter())
                .collect::<Vec<_>>();

            for (param, arg) in params_args {
                if let Expr::Name(ast::ExprName { id, .. }) = arg {
                    if id.as_str() != param.name.as_str() {
                        return;
                    }
                } else {
                    return;
                }
            }
        } else if !call_starargs.is_empty()
            || !keywords.is_empty()
            || !call_ordinary_args.is_empty()
        {
            return;
        }

        //  The lambda is necessary if it uses its parameter in the function it is
        //  calling in the lambda's body
        //  e.g. lambda foo: (func1 if foo else func2)(foo)

        let mut finder = NameFinder { names: vec![] };
        finder.visit_expr(func);

        for expr_name in finder.names {
            if let Some(binding_id) = checker.semantic().resolve_name(expr_name) {
                let binding = checker.semantic().binding(binding_id);
                if checker.semantic().is_current_scope(binding.scope) {
                    return;
                }
            }
        }

        checker
            .diagnostics
            .push(Diagnostic::new(UnnecessaryLambda, lambda.range()));
    }
}
