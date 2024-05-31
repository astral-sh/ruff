use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFunctionDef};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return x` statements in functions, that also contain yield
/// statements.
///
/// ## Why is this bad?
/// Using `return x` in a generator function used to be syntactically invalid
/// in Python 2. In Python 3 `return x` can be used in a generator as a return
/// value in conjunction with yield from. Users coming from Python 2 may expect
/// the old behavior which might lead to bugs. Use native async def coroutines
/// or mark intentional return x usage with # noqa on the same line.
///
/// ## Example
/// ```python
/// def broken():
///     if True:
///         return [1, 2, 3]
///
///     yield 3
///     yield 2
///     yield 1
/// ```
#[violation]
pub struct ReturnInGenerator;

impl Violation for ReturnInGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `yield` together with `return`. Use native `async def` coroutines or put a `# noqa` comment on this line if this was intentional.")
    }
}

/// B901
pub(crate) fn return_in_generator(checker: &mut Checker, function_def: &StmtFunctionDef) {
    if function_def.name.id == "__await__" {
        return;
    }

    let mut visitor = ReturnInGeneratorVisitor::default();
    visitor.visit_body(&function_def.body);

    if visitor.has_yield {
        if let Some(return_) = visitor.return_ {
            checker
                .diagnostics
                .push(Diagnostic::new(ReturnInGenerator, return_));
        }
    }
}

#[derive(Default)]
struct ReturnInGeneratorVisitor {
    return_: Option<TextRange>,
    has_yield: bool,
}

impl Visitor<'_> for ReturnInGeneratorVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(ast::StmtExpr { value, .. }) => match **value {
                Expr::Yield(_) | Expr::YieldFrom(_) => {
                    self.has_yield = true;
                }
                _ => {}
            },
            Stmt::FunctionDef(_) => {
                // do not recurse into nested functions, as they are evaluated
                // individually
            }
            Stmt::Return(ast::StmtReturn { value, range }) => {
                if Option::is_some(value) {
                    self.return_ = Some(*range);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}
