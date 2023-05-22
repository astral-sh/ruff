use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Arg, Arguments, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_leading_content, has_trailing_content};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Generator;
use ruff_python_ast::whitespace::leading_space;
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;

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
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#programming-recommendations)
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
    if let Expr::Name(ast::ExprName { id, .. }) = target {
        if let Expr::Lambda(ast::ExprLambda { args, body, .. }) = value {
            // If the assignment is in a class body, it might not be safe
            // to replace it because the assignment might be
            // carrying a type annotation that will be used by some
            // package like dataclasses, which wouldn't consider the
            // rewritten function definition to be equivalent.
            // See https://github.com/charliermarsh/ruff/issues/3046
            let fixable = !matches!(checker.semantic_model().scope().kind, ScopeKind::Class(_));

            let mut diagnostic = Diagnostic::new(
                LambdaAssignment {
                    name: id.to_string(),
                },
                stmt.range(),
            );

            if checker.patch(diagnostic.kind.rule())
                && fixable
                && !has_leading_content(stmt, checker.locator)
                && !has_trailing_content(stmt, checker.locator)
            {
                let first_line = checker.locator.line(stmt.start());
                let indentation = &leading_space(first_line);
                let mut indented = String::new();
                for (idx, line) in function(
                    checker.semantic_model(),
                    id,
                    args,
                    body,
                    annotation,
                    checker.generator(),
                )
                .universal_newlines()
                .enumerate()
                {
                    if idx == 0 {
                        indented.push_str(&line);
                    } else {
                        indented.push_str(checker.stylist.line_ending().as_str());
                        indented.push_str(indentation);
                        indented.push_str(&line);
                    }
                }
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    indented,
                    stmt.range(),
                )));
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Extract the argument types and return type from a `Callable` annotation.
/// The `Callable` import can be from either `collections.abc` or `typing`.
/// If an ellipsis is used for the argument types, an empty list is returned.
/// The returned values are cloned, so they can be used as-is.
fn extract_types(model: &SemanticModel, annotation: &Expr) -> Option<(Vec<Expr>, Expr)> {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = &annotation else {
        return None;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() else {
        return None;
    };
    if elts.len() != 2 {
        return None;
    }

    if !model.resolve_call_path(value).map_or(false, |call_path| {
        call_path.as_slice() == ["collections", "abc", "Callable"]
            || model.match_typing_call_path(&call_path, "Callable")
    }) {
        return None;
    }

    // The first argument to `Callable` must be a list of types, parameter
    // specification, or ellipsis.
    let args = match &elts[0] {
        Expr::List(ast::ExprList { elts, .. }) => elts.clone(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Ellipsis,
            ..
        }) => vec![],
        _ => return None,
    };

    // The second argument to `Callable` must be a type.
    let return_type = elts[1].clone();

    Some((args, return_type))
}

fn function(
    model: &SemanticModel,
    name: &str,
    args: &Arguments,
    body: &Expr,
    annotation: Option<&Expr>,
    generator: Generator,
) -> String {
    let body = Stmt::Return(ast::StmtReturn {
        value: Some(Box::new(body.clone())),
        range: TextRange::default(),
    });
    if let Some(annotation) = annotation {
        if let Some((arg_types, return_type)) = extract_types(model, annotation) {
            // A `lambda` expression can only have positional and positional-only
            // arguments. The order is always positional-only first, then positional.
            let new_posonlyargs = args
                .posonlyargs
                .iter()
                .enumerate()
                .map(|(idx, arg)| Arg {
                    annotation: arg_types
                        .get(idx)
                        .map(|arg_type| Box::new(arg_type.clone())),
                    ..arg.clone()
                })
                .collect::<Vec<_>>();
            let new_args = args
                .args
                .iter()
                .enumerate()
                .map(|(idx, arg)| Arg {
                    annotation: arg_types
                        .get(idx + new_posonlyargs.len())
                        .map(|arg_type| Box::new(arg_type.clone())),
                    ..arg.clone()
                })
                .collect::<Vec<_>>();
            let func = Stmt::FunctionDef(ast::StmtFunctionDef {
                name: name.into(),
                args: Box::new(Arguments {
                    posonlyargs: new_posonlyargs,
                    args: new_args,
                    ..args.clone()
                }),
                body: vec![body],
                decorator_list: vec![],
                returns: Some(Box::new(return_type)),
                type_comment: None,
                range: TextRange::default(),
            });
            return generator.stmt(&func);
        }
    }
    let func = Stmt::FunctionDef(ast::StmtFunctionDef {
        name: name.into(),
        args: Box::new(args.clone()),
        body: vec![body],
        decorator_list: vec![],
        returns: None,
        type_comment: None,
        range: TextRange::default(),
    });
    generator.stmt(&func)
}
