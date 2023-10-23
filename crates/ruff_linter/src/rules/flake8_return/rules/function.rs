use std::ops::Add;

use ruff_python_ast::{self as ast, ElifElseClause, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::helpers::{is_const_false, is_const_true};
use ruff_python_ast::stmt_if::elif_else_range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::whitespace::indentation;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::is_python_whitespace;

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::registry::{AsRule, Rule};
use crate::rules::flake8_return::helpers::end_of_last_statement;

use super::super::branch::Branch;
use super::super::helpers::result_exists;
use super::super::visitor::{ReturnVisitor, Stack};

/// ## What it does
/// Checks for the presence of a `return None` statement when `None` is the only
/// possible return value.
///
/// ## Why is this bad?
/// Python implicitly assumes `return None` if an explicit `return` value is
/// omitted. Therefore, explicitly returning `None` is redundant and should be
/// avoided when it is the only possible `return` value across all code paths
/// in a given function.
///
/// ## Example
/// ```python
/// def foo(bar):
///     if not bar:
///         return
///     return None
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     if not bar:
///         return
///     return
/// ```
#[violation]
pub struct UnnecessaryReturnNone;

impl AlwaysFixableViolation for UnnecessaryReturnNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not explicitly `return None` in function if it is the only possible return value"
        )
    }

    fn fix_title(&self) -> String {
        "Remove explicit `return None`".to_string()
    }
}

/// ## What it does
/// Checks for the presence of a `return` statement with no explicit value,
/// for functions that return non-`None` values elsewhere.
///
/// ## Why is this bad?
/// Including a `return` statement with no explicit value can cause confusion
/// when other `return` statements in the function return non-`None` values.
/// Python implicitly assumes return `None` if no other return value is present.
/// Adding an explicit `return None` can make the code more readable by clarifying
/// intent.
///
/// ## Example
/// ```python
/// def foo(bar):
///     if not bar:
///         return
///     return 1
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     if not bar:
///         return None
///     return 1
/// ```
#[violation]
pub struct ImplicitReturnValue;

impl AlwaysFixableViolation for ImplicitReturnValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not implicitly `return None` in function able to return non-`None` value")
    }

    fn fix_title(&self) -> String {
        "Add explicit `None` return value".to_string()
    }
}

/// ## What it does
/// Checks for missing explicit `return` statements at the end of functions
/// that can return non-`None` values.
///
/// ## Why is this bad?
/// The lack of an explicit `return` statement at the end of a function that
/// can return non-`None` values can cause confusion. Python implicitly returns
/// `None` if no other return value is present. Adding an explicit
/// `return None` can make the code more readable by clarifying intent.
///
/// ## Example
/// ```python
/// def foo(bar):
///     if not bar:
///         return 1
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     if not bar:
///         return 1
///     return None
/// ```
#[violation]
pub struct ImplicitReturn;

impl AlwaysFixableViolation for ImplicitReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing explicit `return` at the end of function able to return non-`None` value")
    }

    fn fix_title(&self) -> String {
        "Add explicit `return` statement".to_string()
    }
}

/// ## What it does
/// Checks for variable assignments that immediately precede a `return` of the
/// assigned variable.
///
/// ## Why is this bad?
/// The variable assignment is not necessary as the value can be returned
/// directly.
///
/// ## Example
/// ```python
/// def foo():
///     bar = 1
///     return bar
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     return 1
/// ```
#[violation]
pub struct UnnecessaryAssign {
    name: String,
}

impl AlwaysFixableViolation for UnnecessaryAssign {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryAssign { name } = self;
        format!("Unnecessary assignment to `{name}` before `return` statement")
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary assignment".to_string()
    }
}

/// ## What it does
/// Checks for `else` statements with a `return` statement in the preceding
/// `if` block.
///
/// ## Why is this bad?
/// The `else` statement is not needed as the `return` statement will always
/// break out of the enclosing function. Removing the `else` will reduce
/// nesting and make the code more readable.
///
/// ## Example
/// ```python
/// def foo(bar, baz):
///     if bar:
///         return 1
///     else:
///         return baz
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar, baz):
///     if bar:
///         return 1
///     return baz
/// ```
#[violation]
pub struct SuperfluousElseReturn {
    branch: Branch,
}

impl Violation for SuperfluousElseReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseReturn { branch } = self;
        format!("Unnecessary `{branch}` after `return` statement")
    }
}

/// ## What it does
/// Checks for `else` statements with a `raise` statement in the preceding `if`
/// block.
///
/// ## Why is this bad?
/// The `else` statement is not needed as the `raise` statement will always
/// break out of the current scope. Removing the `else` will reduce nesting
/// and make the code more readable.
///
/// ## Example
/// ```python
/// def foo(bar, baz):
///     if bar == "Specific Error":
///         raise Exception(bar)
///     else:
///         raise Exception(baz)
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar, baz):
///     if bar == "Specific Error":
///         raise Exception(bar)
///     raise Exception(baz)
/// ```
#[violation]
pub struct SuperfluousElseRaise {
    branch: Branch,
}

impl Violation for SuperfluousElseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseRaise { branch } = self;
        format!("Unnecessary `{branch}` after `raise` statement")
    }
}

/// ## What it does
/// Checks for `else` statements with a `continue` statement in the preceding
/// `if` block.
///
/// ## Why is this bad?
/// The `else` statement is not needed, as the `continue` statement will always
/// continue onto the next iteration of a loop. Removing the `else` will reduce
/// nesting and make the code more readable.
///
/// ## Example
/// ```python
/// def foo(bar, baz):
///     for i in bar:
///         if i < baz:
///             continue
///         else:
///             x = 0
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar, baz):
///     for i in bar:
///         if i < baz:
///             continue
///         x = 0
/// ```
#[violation]
pub struct SuperfluousElseContinue {
    branch: Branch,
}

impl Violation for SuperfluousElseContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseContinue { branch } = self;
        format!("Unnecessary `{branch}` after `continue` statement")
    }
}

/// ## What it does
/// Checks for `else` statements with a `break` statement in the preceding `if`
/// block.
///
/// ## Why is this bad?
/// The `else` statement is not needed, as the `break` statement will always
/// break out of the loop. Removing the `else` will reduce nesting and make the
/// code more readable.
///
/// ## Example
/// ```python
/// def foo(bar, baz):
///     for i in bar:
///         if i > baz:
///             break
///         else:
///             x = 0
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar, baz):
///     for i in bar:
///         if i > baz:
///             break
///         x = 0
/// ```
#[violation]
pub struct SuperfluousElseBreak {
    branch: Branch,
}

impl Violation for SuperfluousElseBreak {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseBreak { branch } = self;
        format!("Unnecessary `{branch}` after `break` statement")
    }
}

/// RET501
fn unnecessary_return_none(checker: &mut Checker, stack: &Stack) {
    for stmt in &stack.returns {
        let Some(expr) = stmt.value.as_deref() else {
            continue;
        };
        if !is_const_none(expr) {
            continue;
        }
        let mut diagnostic = Diagnostic::new(UnnecessaryReturnNone, stmt.range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "return".to_string(),
            stmt.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// RET502
fn implicit_return_value(checker: &mut Checker, stack: &Stack) {
    for stmt in &stack.returns {
        if stmt.value.is_some() {
            continue;
        }
        let mut diagnostic = Diagnostic::new(ImplicitReturnValue, stmt.range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "return None".to_string(),
            stmt.range,
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// Return `true` if the `func` is a known function that never returns.
fn is_noreturn_func(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).is_some_and(|call_path| {
        matches!(
            call_path.as_slice(),
            ["" | "builtins" | "sys" | "_thread" | "pytest", "exit"]
                | ["" | "builtins", "quit"]
                | ["os" | "posix", "_exit" | "abort"]
                | ["_winapi", "ExitProcess"]
                | ["pytest", "fail" | "skip" | "xfail"]
        ) || semantic.match_typing_call_path(&call_path, "assert_never")
    })
}

/// RET503
fn implicit_return(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            if let Some(last_stmt) = body.last() {
                implicit_return(checker, last_stmt);
            }
            for clause in elif_else_clauses {
                if let Some(last_stmt) = clause.body.last() {
                    implicit_return(checker, last_stmt);
                }
            }

            // Check if we don't have an else clause
            if matches!(
                elif_else_clauses.last(),
                None | Some(ast::ElifElseClause { test: Some(_), .. })
            ) {
                let mut diagnostic = Diagnostic::new(ImplicitReturn, stmt.range());
                if let Some(indent) = indentation(checker.locator(), stmt) {
                    let mut content = String::new();
                    content.push_str(checker.stylist().line_ending().as_str());
                    content.push_str(indent);
                    content.push_str("return None");
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                        content,
                        end_of_last_statement(stmt, checker.locator()),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        Stmt::Assert(ast::StmtAssert { test, .. }) if is_const_false(test) => {}
        Stmt::While(ast::StmtWhile { test, .. }) if is_const_true(test) => {}
        Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
            if let Some(last_stmt) = orelse.last() {
                implicit_return(checker, last_stmt);
            } else {
                let mut diagnostic = Diagnostic::new(ImplicitReturn, stmt.range());
                if let Some(indent) = indentation(checker.locator(), stmt) {
                    let mut content = String::new();
                    content.push_str(checker.stylist().line_ending().as_str());
                    content.push_str(indent);
                    content.push_str("return None");
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                        content,
                        end_of_last_statement(stmt, checker.locator()),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => {
            for case in cases {
                if let Some(last_stmt) = case.body.last() {
                    implicit_return(checker, last_stmt);
                }
            }
        }
        Stmt::With(ast::StmtWith { body, .. }) => {
            if let Some(last_stmt) = body.last() {
                implicit_return(checker, last_stmt);
            }
        }
        Stmt::Return(_) | Stmt::Raise(_) | Stmt::Try(_) => {}
        Stmt::Expr(ast::StmtExpr { value, .. })
            if matches!(
                value.as_ref(),
                Expr::Call(ast::ExprCall { func, ..  })
                    if is_noreturn_func(func, checker.semantic())
            ) => {}
        _ => {
            let mut diagnostic = Diagnostic::new(ImplicitReturn, stmt.range());
            if let Some(indent) = indentation(checker.locator(), stmt) {
                let mut content = String::new();
                content.push_str(checker.stylist().line_ending().as_str());
                content.push_str(indent);
                content.push_str("return None");
                diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                    content,
                    end_of_last_statement(stmt, checker.locator()),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// RET504
fn unnecessary_assign(checker: &mut Checker, stack: &Stack) {
    for (assign, return_, stmt) in &stack.assignment_return {
        // Identify, e.g., `return x`.
        let Some(value) = return_.value.as_ref() else {
            continue;
        };

        let Expr::Name(ast::ExprName {
            id: returned_id, ..
        }) = value.as_ref()
        else {
            continue;
        };

        // Identify, e.g., `x = 1`.
        if assign.targets.len() > 1 {
            continue;
        }

        let Some(target) = assign.targets.first() else {
            continue;
        };

        let Expr::Name(ast::ExprName {
            id: assigned_id, ..
        }) = target
        else {
            continue;
        };

        if returned_id != assigned_id {
            continue;
        }

        if stack.non_locals.contains(assigned_id.as_str()) {
            continue;
        }

        let mut diagnostic = Diagnostic::new(
            UnnecessaryAssign {
                name: assigned_id.to_string(),
            },
            value.range(),
        );
        diagnostic.try_set_fix(|| {
            // Delete the `return` statement. There's no need to treat this as an isolated
            // edit, since we're editing the preceding statement, so no conflicting edit would
            // be allowed to remove that preceding statement.
            let delete_return =
                edits::delete_stmt(stmt, None, checker.locator(), checker.indexer());

            // Replace the `x = 1` statement with `return 1`.
            let content = checker.locator().slice(assign);
            let equals_index = content
                .find('=')
                .ok_or(anyhow::anyhow!("expected '=' in assignment statement"))?;
            let after_equals = equals_index + 1;

            let replace_assign = Edit::range_replacement(
                // If necessary, add whitespace after the `return` keyword.
                // Ex) Convert `x=y` to `return y` (instead of `returny`).
                if content[after_equals..]
                    .chars()
                    .next()
                    .is_some_and(is_python_whitespace)
                {
                    "return".to_string()
                } else {
                    "return ".to_string()
                },
                // Replace from the start of the assignment statement to the end of the equals
                // sign.
                TextRange::new(
                    assign.start(),
                    assign
                        .range()
                        .start()
                        .add(TextSize::try_from(after_equals)?),
                ),
            );

            Ok(Fix::unsafe_edits(replace_assign, [delete_return]))
        });
        checker.diagnostics.push(diagnostic);
    }
}

/// RET505, RET506, RET507, RET508
fn superfluous_else_node(
    checker: &mut Checker,
    if_elif_body: &[Stmt],
    elif_else: &ElifElseClause,
) -> bool {
    let branch = if elif_else.test.is_some() {
        Branch::Elif
    } else {
        Branch::Else
    };
    for child in if_elif_body {
        if child.is_return_stmt() {
            let diagnostic = Diagnostic::new(
                SuperfluousElseReturn { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_break_stmt() {
            let diagnostic = Diagnostic::new(
                SuperfluousElseBreak { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_raise_stmt() {
            let diagnostic = Diagnostic::new(
                SuperfluousElseRaise { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_continue_stmt() {
            let diagnostic = Diagnostic::new(
                SuperfluousElseContinue { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                checker.diagnostics.push(diagnostic);
            }
            return true;
        }
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_elif_else(checker: &mut Checker, stack: &Stack) {
    for (if_elif_body, elif_else) in &stack.elifs_elses {
        superfluous_else_node(checker, if_elif_body, elif_else);
    }
}

/// Run all checks from the `flake8-return` plugin.
pub(crate) fn function(checker: &mut Checker, body: &[Stmt], returns: Option<&Expr>) {
    // Find the last statement in the function.
    let Some(last_stmt) = body.last() else {
        // Skip empty functions.
        return;
    };

    // Skip functions that consist of a single return statement.
    if body.len() == 1 && matches!(last_stmt, Stmt::Return(_)) {
        return;
    }

    // Traverse the function body, to collect the stack.
    let stack = {
        let mut visitor = ReturnVisitor::default();
        for stmt in body {
            visitor.visit_stmt(stmt);
        }
        visitor.stack
    };

    // Avoid false positives for generators.
    if stack.is_generator {
        return;
    }

    if checker.any_enabled(&[
        Rule::SuperfluousElseReturn,
        Rule::SuperfluousElseRaise,
        Rule::SuperfluousElseContinue,
        Rule::SuperfluousElseBreak,
    ]) {
        superfluous_elif_else(checker, &stack);
    }

    // Skip any functions without return statements.
    if stack.returns.is_empty() {
        return;
    }

    // If we have at least one non-`None` return...
    if result_exists(&stack.returns) {
        if checker.enabled(Rule::ImplicitReturnValue) {
            implicit_return_value(checker, &stack);
        }
        if checker.enabled(Rule::ImplicitReturn) {
            implicit_return(checker, last_stmt);
        }

        if checker.enabled(Rule::UnnecessaryAssign) {
            unnecessary_assign(checker, &stack);
        }
    } else {
        if checker.enabled(Rule::UnnecessaryReturnNone) {
            // Skip functions that have a return annotation that is not `None`.
            if returns.map_or(true, is_const_none) {
                unnecessary_return_none(checker, &stack);
            }
        }
    }
}
