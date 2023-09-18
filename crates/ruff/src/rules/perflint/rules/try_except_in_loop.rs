use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for uses of except handling via `try`-`except` within `for` and
/// `while` loops.
///
/// ## Why is this bad?
/// Exception handling via `try`-`except` blocks incurs some performance
/// overhead, regardless of whether an exception is raised.
///
/// When possible, refactor your code to put the entire loop into the
/// `try`-`except` block, rather than wrapping each iteration in a separate
/// `try`-`except` block.
///
/// This rule is only enforced for Python versions prior to 3.11, which
/// introduced "zero cost" exception handling.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// string_numbers: list[str] = ["1", "2", "three", "4", "5"]
///
/// int_numbers: list[int] = []
/// for num in string_numbers:
///     try:
///         int_numbers.append(int(num))
///     except ValueError as e:
///         print(f"Couldn't convert to integer: {e}")
///         break
/// ```
///
/// Use instead:
/// ```python
/// string_numbers: list[str] = ["1", "2", "three", "4", "5"]
///
/// int_numbers: list[int] = []
/// try:
///     for num in string_numbers:
///         int_numbers.append(int(num))
/// except ValueError as e:
///     print(f"Couldn't convert to integer: {e}")
/// ```
///
/// ## Options
/// - `target-version`
#[violation]
pub struct TryExceptInLoop;

impl Violation for TryExceptInLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except` within a loop incurs performance overhead")
    }
}

/// PERF203
pub(crate) fn try_except_in_loop(checker: &mut Checker, body: &[Stmt]) {
    if checker.settings.target_version >= PythonVersion::Py311 {
        return;
    }

    let [Stmt::Try(ast::StmtTry { handlers, body, .. })] = body else {
        return;
    };

    let Some(handler) = handlers.first() else {
        return;
    };

    // Avoid flagging `try`-`except` blocks that contain `break` or `continue`,
    // which rely on the exception handling mechanism.
    if has_break_or_continue(body) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(TryExceptInLoop, handler.range()));
}

/// Returns `true` if a `break` or `continue` statement is present in `body`.
fn has_break_or_continue(body: &[Stmt]) -> bool {
    let mut visitor = LoopControlFlowVisitor::default();
    visitor.visit_body(body);
    visitor.has_break_or_continue
}

#[derive(Debug, Default)]
struct LoopControlFlowVisitor {
    has_break_or_continue: bool,
}

impl StatementVisitor<'_> for LoopControlFlowVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Break(_) | Stmt::Continue(_) => self.has_break_or_continue = true,
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
