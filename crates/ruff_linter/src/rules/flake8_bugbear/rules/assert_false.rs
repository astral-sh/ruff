use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_const_false;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.67")]
pub(crate) struct AssertFalse;

impl AlwaysFixableViolation for AssertFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not `assert False` (`python -O` removes these calls), raise `AssertionError()`"
            .to_string()
    }

    fn fix_title(&self) -> String {
        "Replace `assert False`".to_string()
    }
}

fn assertion_error<'alloc, 'ast>(
    msg: Option<&Expr<'ast>>,
    checker: &'alloc Checker<'ast>,
) -> Stmt<'alloc>
where
    'ast: 'alloc,
{
    Stmt::Raise(ast::StmtRaise {
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        exc: Some(checker.alloc_expr(Expr::Call(ast::ExprCall {
            func: checker.alloc_expr(Expr::Name(ast::ExprName {
                id: ast::name::AstName::new_static("AssertionError"),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            })),
            arguments: Arguments {
                args: if let Some(msg) = msg {
                    checker.alloc_vec(vec![msg.clone()])
                } else {
                    checker.alloc_vec(vec![])
                },
                keywords: checker.alloc_vec(vec![]),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            },
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        }))),
        cause: None,
    })
}

/// B011
pub(crate) fn assert_false<'ast>(
    checker: &Checker<'ast>,
    stmt: &Stmt<'ast>,
    test: &Expr<'ast>,
    msg: Option<&Expr<'ast>>,
) {
    if !is_const_false(test) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(AssertFalse, test.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().stmt(&assertion_error(msg, checker)),
        stmt.range(),
    )));
}
