use ruff_python_ast::{
    self as ast, Constant, Expr, Identifier, Parameter, ParameterWithDefault, Parameters, Stmt,
};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
#[violation]
pub struct LambdaAssignment {
    name: String,
}

impl Violation for LambdaAssignment {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not assign a `lambda` expression, use a `def`")
    }

    fn autofix_title(&self) -> Option<String> {
        let LambdaAssignment { name } = self;
        Some(format!("Rewrite `{name}` as a `def`"))
    }
}

/// E731
pub(crate) fn lambda_assignment(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    annotation: Option<&Expr>,
    stmt: &Stmt,
) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    let Expr::Lambda(ast::ExprLambda {
        parameters, body, ..
    }) = value
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        LambdaAssignment {
            name: id.to_string(),
        },
        stmt.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        if !has_leading_content(stmt.start(), checker.locator())
            && !has_trailing_content(stmt.end(), checker.locator())
        {
            let first_line = checker.locator().line(stmt.start());
            let indentation = leading_indentation(first_line);
            let mut indented = String::new();
            for (idx, line) in function(
                id,
                parameters.as_deref(),
                body,
                annotation,
                checker.semantic(),
                checker.generator(),
            )
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

            // If the assignment is in a class body, it might not be safe to replace it because the
            // assignment might be carrying a type annotation that will be used by some package like
            // dataclasses, which wouldn't consider the rewritten function definition to be
            // equivalent. Even if it _doesn't_ have an annotation, rewriting safely would require
            // making this a static method.
            // See: https://github.com/astral-sh/ruff/issues/3046
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
                diagnostic.set_fix(Fix::manual(Edit::range_replacement(indented, stmt.range())));
            } else {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    indented,
                    stmt.range(),
                )));
            }
        }
    }

    checker.diagnostics.push(diagnostic);
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

    if !semantic.resolve_call_path(value).is_some_and(|call_path| {
        matches!(call_path.as_slice(), ["collections", "abc", "Callable"])
            || semantic.match_typing_call_path(&call_path, "Callable")
    }) {
        return None;
    }

    // The first argument to `Callable` must be a list of types, parameter
    // specification, or ellipsis.
    let params = match param_types {
        Expr::List(ast::ExprList { elts, .. }) => elts.clone(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Ellipsis,
            ..
        }) => vec![],
        _ => return None,
    };

    // The second argument to `Callable` must be a type.
    let return_type = return_type.clone();

    Some((params, return_type))
}

fn function(
    name: &str,
    parameters: Option<&Parameters>,
    body: &Expr,
    annotation: Option<&Expr>,
    semantic: &SemanticModel,
    generator: Generator,
) -> String {
    let body = Stmt::Return(ast::StmtReturn {
        value: Some(Box::new(body.clone())),
        range: TextRange::default(),
    });
    let parameters = parameters
        .cloned()
        .unwrap_or_else(|| Parameters::empty(TextRange::default()));
    if let Some(annotation) = annotation {
        if let Some((arg_types, return_type)) = extract_types(annotation, semantic) {
            // A `lambda` expression can only have positional and positional-only
            // arguments. The order is always positional-only first, then positional.
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
            return generator.stmt(&func);
        }
    }
    let func = Stmt::FunctionDef(ast::StmtFunctionDef {
        is_async: false,
        name: Identifier::new(name.to_string(), TextRange::default()),
        parameters: Box::new(parameters),
        body: vec![body],
        decorator_list: vec![],
        returns: None,
        type_params: None,
        range: TextRange::default(),
    });
    generator.stmt(&func)
}
