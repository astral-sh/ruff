use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::{self as ast, Expr, Int, Number, Operator, Stmt};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops with explicit loop-index variables that can be replaced
/// with `enumerate()`.
///
/// ## Why is this bad?
/// When iterating over a sequence, it's often desirable to keep track of the
/// index of each element alongside the element itself. Prefer the `enumerate`
/// builtin over manually incrementing a counter variable within the loop, as
/// `enumerate` is more concise and idiomatic.
///
/// ## Example
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for fruit in fruits:
///     print(f"{i + 1}. {fruit}")
///     i += 1
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for i, fruit in enumerate(fruits):
///     print(f"{i + 1}. {fruit}")
/// ```
///
/// ## References
/// - [Python documentation: `enumerate`](https://docs.python.org/3/library/functions.html#enumerate)
#[derive(ViolationMetadata)]
pub(crate) struct EnumerateForLoop {
    index: String,
}

impl Violation for EnumerateForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EnumerateForLoop { index } = self;
        format!("Use `enumerate()` for index variable `{index}` in `for` loop")
    }
}

/// SIM113
pub(crate) fn enumerate_for_loop(checker: &Checker, for_stmt: &ast::StmtFor) {
    // If the loop is async, abort.
    if for_stmt.is_async {
        return;
    }

    // If the loop contains a `continue`, abort.
    let mut visitor = LoopControlFlowVisitor::default();
    visitor.visit_body(&for_stmt.body);
    if visitor.has_continue {
        return;
    }

    for stmt in &for_stmt.body {
        // Find the augmented assignment expression (e.g., `i += 1`).
        if let Some(index) = match_index_increment(stmt) {
            // Find the binding corresponding to the initialization (e.g., `i = 1`).
            let Some(id) = checker.semantic().resolve_name(index) else {
                continue;
            };

            // If it's not an assignment (e.g., it's a function argument), ignore it.
            let binding = checker.semantic().binding(id);
            if !binding.kind.is_assignment() {
                continue;
            }

            // If the variable is global or nonlocal, ignore it.
            if binding.is_global() || binding.is_nonlocal() {
                continue;
            }

            // Ensure that the index variable was initialized to 0.
            let Some(value) = typing::find_binding_value(binding, checker.semantic()) else {
                continue;
            };
            if !matches!(
                value,
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Int(Int::ZERO),
                    ..
                })
            ) {
                continue;
            }

            // If the binding is not at the same level as the `for` loop (e.g., it's in an `if`),
            // ignore it.
            let Some(for_loop_id) = checker.semantic().current_statement_id() else {
                continue;
            };
            let Some(assignment_id) = binding.source else {
                continue;
            };
            if checker.semantic().parent_statement_id(for_loop_id)
                != checker.semantic().parent_statement_id(assignment_id)
            {
                continue;
            }

            // Identify the binding created by the augmented assignment.
            // TODO(charlie): There should be a way to go from `ExprName` to `BindingId` (like
            // `resolve_name`, but for bindings rather than references).
            let binding = {
                let mut bindings = checker
                    .semantic()
                    .current_scope()
                    .get_all(&index.id)
                    .map(|id| checker.semantic().binding(id))
                    .filter(|binding| for_stmt.range().contains_range(binding.range()));

                let Some(binding) = bindings.next() else {
                    continue;
                };

                // If there are multiple assignments to this variable _within_ the loop, ignore it.
                if bindings.next().is_some() {
                    continue;
                }

                binding
            };

            // If the variable is used outside the loop, ignore it.
            if binding.references.iter().any(|id| {
                let reference = checker.semantic().reference(*id);
                !for_stmt.range().contains_range(reference.range())
            }) {
                continue;
            }

            let diagnostic = Diagnostic::new(
                EnumerateForLoop {
                    index: index.id.to_string(),
                },
                stmt.range(),
            );
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// If the statement is an index increment statement (e.g., `i += 1`), return
/// the name of the index variable.
fn match_index_increment(stmt: &Stmt) -> Option<&ast::ExprName> {
    let Stmt::AugAssign(ast::StmtAugAssign {
        target,
        op: Operator::Add,
        value,
        ..
    }) = stmt
    else {
        return None;
    };

    let name = target.as_name_expr()?;

    if matches!(
        value.as_ref(),
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            ..
        })
    ) {
        return Some(name);
    }

    None
}

#[derive(Debug, Default)]
struct LoopControlFlowVisitor {
    has_continue: bool,
}

impl StatementVisitor<'_> for LoopControlFlowVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Continue(_) => self.has_continue = true,
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
