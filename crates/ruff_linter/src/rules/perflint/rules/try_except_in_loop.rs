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
/// To optimize your code, two techniques are possible:
/// 1. Refactor your code to put the entire loop into the `try`-`except` block,
///    rather than wrapping each iteration in a separate `try`-`except` block.
/// 2. Use "Look Before You Leap" idioms that attempt to avoid exceptions
///    being raised in the first place, avoiding the need to use `try`-`except`
///    blocks in the first place.
///
/// This rule is only enforced for Python versions prior to 3.11, which
/// introduced "zero-cost" exception handling. However, note that even on
/// Python 3.11 and newer, refactoring your code to avoid exception handling in
/// tight loops can provide a significant speedup in some cases, as zero-cost
/// exception handling is only zero-cost in the "happy path" where no exception
/// is raised in the `try`-`except` block.
///
/// As with all `perflint` rules, this is only intended as a
/// micro-optimization. In many cases, it will have a negligible impact on
/// performance.
///
/// ## Example
/// ```python
/// string_numbers: list[str] = ["1", "2", "three", "4", "5"]
///
/// # `try`/`except` that could be moved out of the loop:
/// int_numbers: list[int] = []
/// for num in string_numbers:
///     try:
///         int_numbers.append(int(num))
///     except ValueError as e:
///         print(f"Couldn't convert to integer: {e}")
///         break
///
/// # `try`/`except` used when "look before you leap" idioms could be used:
/// number_names: dict[int, str] = {1: "one", 3: "three", 4: "four"}
/// for number in range(5):
///     try:
///         name = number_names[number]
///     except KeyError:
///         continue
///     else:
///         print(f"The name of {number} is {name}")
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
///
/// number_names: dict[int, str] = {1: "one", 3: "three", 4: "four"}
/// for number in range(5):
///     name = number_names.get(number)
///     if name is not None:
///         print(f"The name of {number} is {name}")
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
