use std::ops::Add;

use anyhow::Result;

use ruff_diagnostics::{AlwaysFixableViolation, FixAvailability, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_false, is_const_true};
use ruff_python_ast::stmt_if::elif_else_range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{self as ast, Decorator, ElifElseClause, Expr, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_semantic::analyze::visibility::is_property;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{is_python_whitespace, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::fix::edits::adjust_indentation;
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
/// The variable assignment is not necessary, as the value can be returned
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseReturn { branch } = self;
        format!("Unnecessary `{branch}` after `return` statement")
    }

    fn fix_title(&self) -> Option<String> {
        let SuperfluousElseReturn { branch } = self;
        Some(format!("Remove unnecessary `{branch}`"))
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseRaise { branch } = self;
        format!("Unnecessary `{branch}` after `raise` statement")
    }

    fn fix_title(&self) -> Option<String> {
        let SuperfluousElseRaise { branch } = self;
        Some(format!("Remove unnecessary `{branch}`"))
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseContinue { branch } = self;
        format!("Unnecessary `{branch}` after `continue` statement")
    }

    fn fix_title(&self) -> Option<String> {
        let SuperfluousElseContinue { branch } = self;
        Some(format!("Remove unnecessary `{branch}`"))
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseBreak { branch } = self;
        format!("Unnecessary `{branch}` after `break` statement")
    }

    fn fix_title(&self) -> Option<String> {
        let SuperfluousElseBreak { branch } = self;
        Some(format!("Remove unnecessary `{branch}`"))
    }
}

/// RET501
fn unnecessary_return_none(checker: &mut Checker, decorator_list: &[Decorator], stack: &Stack) {
    for stmt in &stack.returns {
        let Some(expr) = stmt.value.as_deref() else {
            continue;
        };
        if !expr.is_none_literal_expr() {
            continue;
        }

        // Skip property functions
        if is_property(
            decorator_list,
            checker.settings.pydocstyle.property_decorators(),
            checker.semantic(),
        ) {
            return;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryReturnNone, stmt.range());
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
        let mut diagnostic = Diagnostic::new(ImplicitReturnValue, stmt.range());
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "return None".to_string(),
            stmt.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// Return `true` if the `func` appears to be non-returning.
fn is_noreturn_func(func: &Expr, semantic: &SemanticModel) -> bool {
    // First, look for known functions that never return from the standard library and popular
    // libraries.
    if semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins" | "sys" | "_thread" | "pytest", "exit"]
                    | ["" | "builtins", "quit"]
                    | ["os" | "posix", "_exit" | "abort"]
                    | ["_winapi", "ExitProcess"]
                    | ["pytest", "fail" | "skip" | "xfail"]
            ) || semantic.match_typing_qualified_name(&qualified_name, "assert_never")
        })
    {
        return true;
    }

    // Second, look for `NoReturn` annotations on the return type.
    let Some(func_binding) = semantic.lookup_attribute(func) else {
        return false;
    };
    let Some(node_id) = semantic.binding(func_binding).source else {
        return false;
    };

    let Stmt::FunctionDef(ast::StmtFunctionDef { returns, .. }) = semantic.statement(node_id)
    else {
        return false;
    };

    let Some(returns) = returns.as_ref() else {
        return false;
    };

    let Some(qualified_name) = semantic.resolve_qualified_name(returns) else {
        return false;
    };

    semantic.match_typing_qualified_name(&qualified_name, "NoReturn")
}

fn add_return_none(checker: &mut Checker, stmt: &Stmt, range: TextRange) {
    let mut diagnostic = Diagnostic::new(ImplicitReturn, range);
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

/// Returns a list of all implicit returns in the given statement.
///
/// Note: The function should be refactored to `has_implicit_return` with an early return (when seeing the first implicit return)
/// when removing the preview gating.
fn implicit_returns<'a>(checker: &Checker, stmt: &'a Stmt) -> Vec<&'a Stmt> {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            let mut implicit_stmts = body
                .last()
                .map(|last| implicit_returns(checker, last))
                .unwrap_or_default();

            for clause in elif_else_clauses {
                implicit_stmts.extend(
                    clause
                        .body
                        .last()
                        .iter()
                        .flat_map(|last| implicit_returns(checker, last)),
                );
            }

            // Check if we don't have an else clause
            if matches!(
                elif_else_clauses.last(),
                None | Some(ast::ElifElseClause { test: Some(_), .. })
            ) {
                implicit_stmts.push(stmt);
            }
            implicit_stmts
        }
        Stmt::Assert(ast::StmtAssert { test, .. }) if is_const_false(test) => vec![],
        Stmt::While(ast::StmtWhile { test, .. }) if is_const_true(test) => vec![],
        Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
            if let Some(last_stmt) = orelse.last() {
                implicit_returns(checker, last_stmt)
            } else {
                vec![stmt]
            }
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => {
            let mut implicit_stmts = vec![];
            for case in cases {
                implicit_stmts.extend(
                    case.body
                        .last()
                        .into_iter()
                        .flat_map(|last_stmt| implicit_returns(checker, last_stmt)),
                );
            }
            implicit_stmts
        }
        Stmt::With(ast::StmtWith { body, .. }) => body
            .last()
            .map(|last_stmt| implicit_returns(checker, last_stmt))
            .unwrap_or_default(),
        Stmt::Return(_) | Stmt::Raise(_) | Stmt::Try(_) => vec![],
        Stmt::Expr(ast::StmtExpr { value, .. })
            if matches!(
                value.as_ref(),
                Expr::Call(ast::ExprCall { func, ..  })
                    if is_noreturn_func(func, checker.semantic())
            ) =>
        {
            vec![]
        }
        _ => {
            vec![stmt]
        }
    }
}

/// RET503
fn implicit_return(checker: &mut Checker, function_def: &ast::StmtFunctionDef, stmt: &Stmt) {
    let implicit_stmts = implicit_returns(checker, stmt);

    if implicit_stmts.is_empty() {
        return;
    }

    if checker.settings.preview.is_enabled() {
        add_return_none(checker, stmt, function_def.range());
    } else {
        for implicit_stmt in implicit_stmts {
            add_return_none(checker, implicit_stmt, implicit_stmt.range());
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

        // Ignore variables that have an annotation defined elsewhere.
        if stack.annotations.contains(assigned_id.as_str()) {
            continue;
        }

        // Ignore `nonlocal` and `global` variables.
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
            let mut diagnostic = Diagnostic::new(
                SuperfluousElseReturn { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    remove_else(
                        elif_else,
                        checker.locator(),
                        checker.indexer(),
                        checker.stylist(),
                    )
                });
                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_break_stmt() {
            let mut diagnostic = Diagnostic::new(
                SuperfluousElseBreak { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    remove_else(
                        elif_else,
                        checker.locator(),
                        checker.indexer(),
                        checker.stylist(),
                    )
                });

                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_raise_stmt() {
            let mut diagnostic = Diagnostic::new(
                SuperfluousElseRaise { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    remove_else(
                        elif_else,
                        checker.locator(),
                        checker.indexer(),
                        checker.stylist(),
                    )
                });

                checker.diagnostics.push(diagnostic);
            }
            return true;
        } else if child.is_continue_stmt() {
            let mut diagnostic = Diagnostic::new(
                SuperfluousElseContinue { branch },
                elif_else_range(elif_else, checker.locator().contents())
                    .unwrap_or_else(|| elif_else.range()),
            );
            if checker.enabled(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    remove_else(
                        elif_else,
                        checker.locator(),
                        checker.indexer(),
                        checker.stylist(),
                    )
                });

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
pub(crate) fn function(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    let ast::StmtFunctionDef {
        decorator_list,
        returns,
        body,
        ..
    } = function_def;

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
        let mut visitor = ReturnVisitor::new(checker.semantic());
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
            implicit_return(checker, function_def, last_stmt);
        }

        if checker.enabled(Rule::UnnecessaryAssign) {
            unnecessary_assign(checker, &stack);
        }
    } else {
        if checker.enabled(Rule::UnnecessaryReturnNone) {
            // Skip functions that have a return annotation that is not `None`.
            if returns.as_deref().map_or(true, Expr::is_none_literal_expr) {
                unnecessary_return_none(checker, decorator_list, &stack);
            }
        }
    }
}

/// Generate a [`Fix`] to remove an `else` or `elif` clause.
fn remove_else(
    elif_else: &ElifElseClause,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Fix> {
    if elif_else.test.is_some() {
        // Ex) `elif` -> `if`
        Ok(Fix::safe_edit(Edit::deletion(
            elif_else.start(),
            elif_else.start() + TextSize::from(2),
        )))
    } else {
        // the start of the line where the `else`` is
        let else_line_start = locator.line_start(elif_else.start());

        // making a tokenizer to find the Colon for the `else`, not always on the same line!
        let mut else_line_tokenizer =
            SimpleTokenizer::starts_at(elif_else.start(), locator.contents());

        // find the Colon for the `else`
        let Some(else_colon) =
            else_line_tokenizer.find(|token| token.kind == SimpleTokenKind::Colon)
        else {
            return Err(anyhow::anyhow!("Cannot find `:` in `else` statement"));
        };

        // get the indentation of the `else`, since that is the indent level we want to end with
        let Some(desired_indentation) = indentation(locator, elif_else) else {
            return Err(anyhow::anyhow!("Compound statement cannot be inlined"));
        };

        // If the statement is on the same line as the `else`, just remove the `else: `.
        // Ex) `else: return True` -> `return True`
        if let Some(first) = elif_else.body.first() {
            if indexer.preceded_by_multi_statement_line(first, locator) {
                return Ok(Fix::safe_edit(Edit::deletion(
                    elif_else.start(),
                    first.start(),
                )));
            }
        }

        // we're deleting the `else`, and it's Colon, and the rest of the line(s) they're on,
        // so here we get the last position of the line the Colon is on
        let else_colon_end = locator.full_line_end(else_colon.end());

        // if there is a comment on the same line as the Colon, let's keep it
        // and give it the proper indentation once we unindent it
        let else_comment_after_colon = else_line_tokenizer
            .find(|token| token.kind.is_comment())
            .and_then(|token| {
                if token.kind == SimpleTokenKind::Comment && token.start() < else_colon_end {
                    return Some(format!(
                        "{desired_indentation}{}{}",
                        locator.slice(token),
                        stylist.line_ending().as_str(),
                    ));
                }
                None
            })
            .unwrap_or(String::new());

        let indented = adjust_indentation(
            TextRange::new(else_colon_end, elif_else.end()),
            desired_indentation,
            locator,
            indexer,
            stylist,
        )?;

        Ok(Fix::safe_edit(Edit::replacement(
            format!("{else_comment_after_colon}{indented}"),
            else_line_start,
            elif_else.end(),
        )))
    }
}
