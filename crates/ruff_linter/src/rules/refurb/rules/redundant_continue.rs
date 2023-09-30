use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest, registry::AsRule};

/// ## What it does
/// TODO
///
/// ## Why is this bad?
/// TODO
/// 
///
/// ## Example
/// ```python
/// def func():
///     while True:
///         pass
///
///         continue
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     while True:
///         pass
/// ```
///
/// ## References
/// - [Python documentation: `continue`](https://docs.python.org/3/reference/simple_stmts.html#continue)

#[violation]
pub struct ImplicitCwd;

impl Violation for ImplicitCwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't explicitly continue if you are already at the end of the control flow")
    }
}


/// FURB133
pub(crate) fn no_redundant_continue(checker: &mut Checker, continue_stmt: &ast::Continue) {}