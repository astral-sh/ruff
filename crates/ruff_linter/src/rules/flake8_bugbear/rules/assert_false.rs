use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_false;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `assert False`.
///
/// ## Why is this bad?
/// Python removes `assert` statements when running in optimized mode
/// (`python -O`), making `assert False` an unreliable means of
/// raising an `AssertionError`.
///
/// Instead, raise an `AssertionError` directly.
///
/// ## Example
/// ```python
/// assert False
/// ```
///
/// Use instead:
/// ```python
/// raise AssertionError
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as changing an `assert` to a
/// `raise` will change the behavior of your program when running in
/// optimized mode (`python -O`).
///
/// ## References
/// - [Python documentation: `assert`](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
#[violation]
pub struct AssertFalse;

impl AlwaysFixableViolation for AssertFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not `assert False` (`python -O` removes these calls), raise `AssertionError()`")
    }

    fn fix_title(&self) -> String {
        "Replace `assert False`".to_string()
    }
}

fn assertion_error(msg: Option<&Expr>) -> Stmt {
    Stmt::Raise(ast::StmtRaise {
        range: TextRange::default(),
        exc: Some(Box::new(Expr::Call(ast::ExprCall {
            func: Box::new(Expr::Name(ast::ExprName {
                id: "AssertionError".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            })),
            arguments: Arguments {
                args: if let Some(msg) = msg {
                    Box::from([msg.clone()])
                } else {
                    Box::from([])
                },
                keywords: Box::from([]),
                range: TextRange::default(),
            },
            range: TextRange::default(),
        }))),
        cause: None,
    })
}

/// B011
pub(crate) fn assert_false(checker: &mut Checker, stmt: &Stmt, test: &Expr, msg: Option<&Expr>) {
    if !is_const_false(test) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssertFalse, test.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().stmt(&assertion_error(msg)),
        stmt.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
