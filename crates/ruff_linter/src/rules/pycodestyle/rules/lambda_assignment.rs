use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{
    self as ast, Expr, ExprEllipsisLiteral, ExprLambda, Identifier, Parameter,
    ParameterWithDefault, Parameters, Stmt,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lambda expressions which are assigned to a variable.
///
/// ## Why is this bad?
/// Per PEP 8, you should "Always use a def statement instead of an assignment
/// statement that binds a lambda expression directly to an identifier."
///
/// Using a `def` statement leads to better tracebacks, and the assignment
/// itself negates the primary benefit of using a `lambda` expression (i.e.,
/// that it can be embedded inside another expression).
///
/// ## Example
/// ```python
/// f = lambda x: 2 * x
/// ```
///
/// Use instead:
/// ```python
/// def f(x):
///     return 2 * x
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
#[derive(ViolationMetadata)]
pub(crate) struct LambdaAssignment {
    name: String,
}

impl Violation for LambdaAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not assign a `lambda` expression, use a `def`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let LambdaAssignment { name } = self;
        Some(format!("Rewrite `{name}` as a `def`"))
    }
}

/// E731
pub(crate) fn lambda_assignment(
    checker: &Checker,
    target: &Expr,
    value: &Expr,
    annotation: Option<&Expr>,
    stmt: &Stmt,
) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    let Expr::Lambda(lambda) = value else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        LambdaAssignment {
            name: id.to_string(),
        },
        stmt.range(),
    );

    if !has_leading_content(stmt.start(), checker.source())
        && !has_trailing_content(stmt.end(), checker.source())
    {
        let first_line = checker.locator().line_str(stmt.start());
        let indentation = leading_indentation(first_line);
        let mut indented = String::new();
        for (idx, line) in function(id, lambda, annotation, checker)
            .universal_newlines()
            .enumerate()
        {
            if idx == 0 {
                indented.push_str(&line);
            } else {
                indented.push_str(checker.stylist().line_ending().as_str());
                indented.push_str(indentation);
                indented.push_str(&line);
            }
        }

        // If the assignment is a class attribute (with an annotation), ignore it.
        //
        // This is most common for, e.g., dataclasses and Pydantic models. Those libraries will
        // treat the lambda as an assignable field, and the use of a lambda is almost certainly
        // intentional.
        if annotation.is_some() && checker.semantic().current_scope().kind.is_class() {
            return;
        }

        // Otherwise, if the assignment is in a class body, flag it, but use a display-only fix.
        // Rewriting safely would require making this a static method.
        //
        // Similarly, if the lambda is shadowing a variable in the current scope,
        // rewriting it as a function declaration may break type-checking.
        // See: https://github.com/astral-sh/ruff/issues/5421
        if checker.semantic().current_scope().kind.is_class()
            || checker
                .semantic()
                .current_scope()
                .get_all(id)
                .any(|binding_id| checker.semantic().binding(binding_id).kind.is_annotation())
        {
            diagnostic.set_fix(Fix::display_only_edit(Edit::range_replacement(
                indented,
                stmt.range(),
            )));
        } else {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                indented,
                stmt.range(),
            )));
        }
    }

    checker.report_diagnostic(diagnostic);
}

/// Extract the argument types and return type from a `Callable` annotation.
/// The `Callable` import can be from either `collections.abc` or `typing`.
/// If an ellipsis is used for the argument types, an empty list is returned.
/// The returned values are cloned, so they can be used as-is.
fn extract_types(annotation: &Expr, semantic: &SemanticModel) -> Option<(Vec<Expr>, Expr)> {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = &annotation else {
        return None;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() else {
        return None;
    };
    let [param_types, return_type] = elts.as_slice() else {
        return None;
    };

    if !semantic
        .resolve_qualified_name(value)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["collections", "abc", "Callable"]
            ) || semantic.match_typing_qualified_name(&qualified_name, "Callable")
        })
    {
        return None;
    }

    // The first argument to `Callable` must be a list of types, parameter
    // specification, or ellipsis.
    let params = match param_types {
        Expr::List(ast::ExprList { elts, .. }) => elts.clone(),
        Expr::EllipsisLiteral(_) => vec![],
        _ => return None,
    };

    // The second argument to `Callable` must be a type.
    let return_type = return_type.clone();

    Some((params, return_type))
}

/// Generate a function definition from a `lambda` expression.
fn function(
    name: &str,
    lambda: &ExprLambda,
    annotation: Option<&Expr>,
    checker: &Checker,
) -> String {
    // Use a dummy body. It gets replaced at the end with the actual body.
    // This allows preserving the source formatting for the body.
    let body = Stmt::Return(ast::StmtReturn {
        value: Some(Box::new(Expr::EllipsisLiteral(
            ExprEllipsisLiteral::default(),
        ))),
        range: TextRange::default(),
    });
    let parameters = lambda.parameters.as_deref().cloned().unwrap_or_default();
    if let Some(annotation) = annotation {
        if let Some((arg_types, return_type)) = extract_types(annotation, checker.semantic()) {
            // A `lambda` expression can only have positional-only and positional-or-keyword
            // arguments. The order is always positional-only first, then positional-or-keyword.
            let new_posonlyargs = parameters
                .posonlyargs
                .iter()
                .enumerate()
                .map(|(idx, parameter)| ParameterWithDefault {
                    parameter: Parameter {
                        annotation: arg_types
                            .get(idx)
                            .map(|arg_type| Box::new(arg_type.clone())),
                        ..parameter.parameter.clone()
                    },
                    ..parameter.clone()
                })
                .collect::<Vec<_>>();
            let new_args = parameters
                .args
                .iter()
                .enumerate()
                .map(|(idx, parameter)| ParameterWithDefault {
                    parameter: Parameter {
                        annotation: arg_types
                            .get(idx + new_posonlyargs.len())
                            .map(|arg_type| Box::new(arg_type.clone())),
                        ..parameter.parameter.clone()
                    },
                    ..parameter.clone()
                })
                .collect::<Vec<_>>();
            let func = Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async: false,
                name: Identifier::new(name.to_string(), TextRange::default()),
                parameters: Box::new(Parameters {
                    posonlyargs: new_posonlyargs,
                    args: new_args,
                    ..parameters
                }),
                body: vec![body],
                decorator_list: vec![],
                returns: Some(Box::new(return_type)),
                type_params: None,
                range: TextRange::default(),
            });
            let generated = checker.generator().stmt(&func);

            return replace_trailing_ellipsis_with_original_expr(generated, lambda, checker);
        }
    }
    let function = Stmt::FunctionDef(ast::StmtFunctionDef {
        is_async: false,
        name: Identifier::new(name.to_string(), TextRange::default()),
        parameters: Box::new(parameters),
        body: vec![body],
        decorator_list: vec![],
        returns: None,
        type_params: None,
        range: TextRange::default(),
    });
    let generated = checker.generator().stmt(&function);

    replace_trailing_ellipsis_with_original_expr(generated, lambda, checker)
}

fn replace_trailing_ellipsis_with_original_expr(
    mut generated: String,
    lambda: &ExprLambda,
    checker: &Checker,
) -> String {
    let original_expr_range = parenthesized_range(
        (&lambda.body).into(),
        lambda.into(),
        checker.comment_ranges(),
        checker.source(),
    )
    .unwrap_or(lambda.body.range());

    let original_expr_in_source = checker.locator().slice(original_expr_range);

    let placeholder_ellipsis_start = generated.rfind("...").unwrap();
    let placeholder_ellipsis_end = placeholder_ellipsis_start + "...".len();

    generated.replace_range(
        placeholder_ellipsis_start..placeholder_ellipsis_end,
        original_expr_in_source,
    );
    generated
}
