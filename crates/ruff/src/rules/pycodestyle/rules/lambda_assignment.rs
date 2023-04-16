use ruff_python_semantic::context::Context;
use rustpython_parser::ast::{
    Arg, ArgData, Arguments, Constant, Expr, ExprKind, Location, Stmt, StmtKind,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{match_leading_content, match_trailing_content, unparse_stmt};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::leading_space;
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
/// f = lambda x: 2*x
/// ```
///
/// Use instead:
/// ```python
/// def f(x):
///    return 2 * x
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#programming-recommendations)
#[violation]
pub struct LambdaAssignment {
    pub name: String,
    pub fixable: bool,
}

impl Violation for LambdaAssignment {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not assign a `lambda` expression, use a `def`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|LambdaAssignment { name, .. }| format!("Rewrite `{name}` as a `def`"))
    }
}

/// E731
pub fn lambda_assignment(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    annotation: Option<&Expr>,
    stmt: &Stmt,
) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            // If the assignment is in a class body, it might not be safe
            // to replace it because the assignment might be
            // carrying a type annotation that will be used by some
            // package like dataclasses, which wouldn't consider the
            // rewritten function definition to be equivalent.
            // See https://github.com/charliermarsh/ruff/issues/3046
            let fixable = !matches!(checker.ctx.scope().kind, ScopeKind::Class(_));

            let mut diagnostic = Diagnostic::new(
                LambdaAssignment {
                    name: id.to_string(),
                    fixable,
                },
                Range::from(stmt),
            );

            if checker.patch(diagnostic.kind.rule())
                && fixable
                && !match_leading_content(stmt, checker.locator)
                && !match_trailing_content(stmt, checker.locator)
            {
                let first_line = checker.locator.slice(Range::new(
                    Location::new(stmt.location.row(), 0),
                    Location::new(stmt.location.row() + 1, 0),
                ));
                let indentation = &leading_space(first_line);
                let mut indented = String::new();
                for (idx, line) in
                    function(&checker.ctx, id, args, body, annotation, checker.stylist)
                        .universal_newlines()
                        .enumerate()
                {
                    if idx == 0 {
                        indented.push_str(line);
                    } else {
                        indented.push_str(checker.stylist.line_ending().as_str());
                        indented.push_str(indentation);
                        indented.push_str(line);
                    }
                }
                diagnostic.set_fix(Edit::replacement(
                    indented,
                    stmt.location,
                    stmt.end_location.unwrap(),
                ));
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Extract the argument types and return type from a `Callable` annotation.
/// The `Callable` import can be from either `collections.abc` or `typing`.
/// If an ellipsis is used for the argument types, an empty list is returned.
/// The returned values are cloned, so they can be used as-is.
fn extract_types(ctx: &Context, annotation: &Expr) -> Option<(Vec<Expr>, Expr)> {
    let ExprKind::Subscript { value, slice, .. } = &annotation.node else {
        return None;
    };
    let ExprKind::Tuple { elts, .. } = &slice.node else {
        return None;
    };
    if elts.len() != 2 {
        return None;
    }

    if !ctx.resolve_call_path(value).map_or(false, |call_path| {
        call_path.as_slice() == ["collections", "abc", "Callable"]
            || ctx.match_typing_call_path(&call_path, "Callable")
    }) {
        return None;
    }

    // The first argument to `Callable` must be a list of types, parameter
    // specification, or ellipsis.
    let args = match &elts[0].node {
        ExprKind::List { elts, .. } => elts.clone(),
        ExprKind::Constant {
            value: Constant::Ellipsis,
            ..
        } => vec![],
        _ => return None,
    };

    // The second argument to `Callable` must be a type.
    let return_type = elts[1].clone();

    Some((args, return_type))
}

fn function(
    ctx: &Context,
    name: &str,
    args: &Arguments,
    body: &Expr,
    annotation: Option<&Expr>,
    stylist: &Stylist,
) -> String {
    let body = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Return {
            value: Some(Box::new(body.clone())),
        },
    );
    if let Some(annotation) = annotation {
        if let Some((arg_types, return_type)) = extract_types(ctx, annotation) {
            // A `lambda` expression can only have positional and positional-only
            // arguments. The order is always positional-only first, then positional.
            let new_posonlyargs = args
                .posonlyargs
                .iter()
                .enumerate()
                .map(|(idx, arg)| {
                    Arg::new(
                        Location::default(),
                        Location::default(),
                        ArgData {
                            annotation: arg_types
                                .get(idx)
                                .map(|arg_type| Box::new(arg_type.clone())),
                            ..arg.node.clone()
                        },
                    )
                })
                .collect::<Vec<_>>();
            let new_args = args
                .args
                .iter()
                .enumerate()
                .map(|(idx, arg)| {
                    Arg::new(
                        Location::default(),
                        Location::default(),
                        ArgData {
                            annotation: arg_types
                                .get(idx + new_posonlyargs.len())
                                .map(|arg_type| Box::new(arg_type.clone())),
                            ..arg.node.clone()
                        },
                    )
                })
                .collect::<Vec<_>>();
            let func = Stmt::new(
                Location::default(),
                Location::default(),
                StmtKind::FunctionDef {
                    name: name.to_string(),
                    args: Box::new(Arguments {
                        posonlyargs: new_posonlyargs,
                        args: new_args,
                        ..args.clone()
                    }),
                    body: vec![body],
                    decorator_list: vec![],
                    returns: Some(Box::new(return_type)),
                    type_comment: None,
                },
            );
            return unparse_stmt(&func, stylist);
        }
    }
    let func = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::FunctionDef {
            name: name.to_string(),
            args: Box::new(args.clone()),
            body: vec![body],
            decorator_list: vec![],
            returns: None,
            type_comment: None,
        },
    );
    unparse_stmt(&func, stylist)
}
