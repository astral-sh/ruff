use ruff_python_ast as ast;

use crate::types::{context::InferContext, diagnostic::INVALID_TYPED_DICT_STATEMENT};

pub(super) fn validate_typed_dict_class(
    context: &InferContext<'_, '_>,
    class_node: &ast::StmtClassDef,
) {
    // Check that a class-based `TypedDict` doesn't include any invalid statements:
    // https://typing.python.org/en/latest/spec/typeddict.html#class-based-syntax
    //
    //     The body of the class definition defines the items of the `TypedDict` type. It
    //     may also contain a docstring or pass statements (primarily to allow the creation
    //     of an empty `TypedDict`). No other statements are allowed, and type checkers
    //     should report an error if any are present.
    for stmt in &class_node.body {
        match stmt {
            // Annotated assignments are allowed (that's the whole point), but they're
            // not allowed to have a value.
            ast::Stmt::AnnAssign(ann_assign) => {
                if let Some(value) = &ann_assign.value
                    && let Some(builder) =
                        context.report_lint(&INVALID_TYPED_DICT_STATEMENT, &**value)
                {
                    builder.into_diagnostic("TypedDict item cannot have a value");
                }

                continue;
            }
            // Pass statements are allowed.
            ast::Stmt::Pass(_) => continue,
            ast::Stmt::Expr(expr) => {
                // Docstrings are allowed.
                if matches!(*expr.value, ast::Expr::StringLiteral(_)) {
                    continue;
                }
                // As a non-standard but common extension, we also interpret `...` as
                // equivalent to `pass`.
                if matches!(*expr.value, ast::Expr::EllipsisLiteral(_)) {
                    continue;
                }
            }
            // Everything else is forbidden.
            _ => {}
        }
        if let Some(builder) = context.report_lint(&INVALID_TYPED_DICT_STATEMENT, stmt) {
            if matches!(stmt, ast::Stmt::FunctionDef(_)) {
                builder.into_diagnostic(format_args!("TypedDict class cannot have methods"));
            } else {
                let mut diagnostic = builder
                    .into_diagnostic(format_args!("invalid statement in TypedDict class body"));
                diagnostic.info("Only annotated declarations (`<name>: <type>`) are allowed.");
            }
        }
    }
}
