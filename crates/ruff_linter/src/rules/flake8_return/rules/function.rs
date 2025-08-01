use anyhow::Result;

use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{is_const_false, is_const_true};
use ruff_python_ast::stmt_if::elif_else_range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{self as ast, Decorator, ElifElseClause, Expr, Stmt};
use ruff_python_parser::TokenKind;
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::visibility::is_property;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer, is_python_whitespace};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::fix::edits::adjust_indentation;
use crate::registry::Rule;
use crate::rules::flake8_return::helpers::end_of_last_statement;
use crate::{AlwaysFixableViolation, FixAvailability, Violation};
use crate::{Edit, Fix};

use crate::rules::flake8_return::branch::Branch;
use crate::rules::flake8_return::helpers::result_exists;
use crate::rules::flake8_return::visitor::{ReturnVisitor, Stack};

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
///
/// ## Fix safety
/// This rule's fix is marked as unsafe for cases in which comments would be
/// dropped from the `return` statement.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryReturnNone;

impl AlwaysFixableViolation for UnnecessaryReturnNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not explicitly `return None` in function if it is the only possible return value"
            .to_string()
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
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitReturnValue;

impl AlwaysFixableViolation for ImplicitReturnValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not implicitly `return None` in function able to return non-`None` value".to_string()
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
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitReturn;

impl AlwaysFixableViolation for ImplicitReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing explicit `return` at the end of function able to return non-`None` value"
            .to_string()
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
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryAssign {
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
#[derive(ViolationMetadata)]
pub(crate) struct SuperfluousElseReturn {
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
#[derive(ViolationMetadata)]
pub(crate) struct SuperfluousElseRaise {
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
#[derive(ViolationMetadata)]
pub(crate) struct SuperfluousElseContinue {
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
#[derive(ViolationMetadata)]
pub(crate) struct SuperfluousElseBreak {
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
fn unnecessary_return_none(checker: &Checker, decorator_list: &[Decorator], stack: &Stack) {
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
            checker.settings().pydocstyle.property_decorators(),
            checker.semantic(),
        ) {
            return;
        }

        let mut diagnostic = checker.report_diagnostic(UnnecessaryReturnNone, stmt.range());
        let edit = Edit::range_replacement("return".to_string(), stmt.range());
        diagnostic.set_fix(Fix::applicable_edit(
            edit,
            if checker.comment_ranges().intersects(stmt.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }
}

/// RET502
fn implicit_return_value(checker: &Checker, stack: &Stack) {
    for stmt in &stack.returns {
        if stmt.value.is_some() {
            continue;
        }
        let mut diagnostic = checker.report_diagnostic(ImplicitReturnValue, stmt.range());
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "return None".to_string(),
            stmt.range(),
        )));
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
        || semantic.match_typing_qualified_name(&qualified_name, "Never")
}

fn add_return_none(checker: &Checker, stmt: &Stmt, range: TextRange) {
    let mut diagnostic = checker.report_diagnostic(ImplicitReturn, range);
    if let Some(indent) = indentation(checker.source(), stmt) {
        let mut content = String::new();
        content.push_str(checker.stylist().line_ending().as_str());
        content.push_str(indent);
        content.push_str("return None");
        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
            content,
            end_of_last_statement(stmt, checker.locator()),
        )));
    }
}

fn has_implicit_return(checker: &Checker, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            if body
                .last()
                .is_some_and(|last| has_implicit_return(checker, last))
            {
                return true;
            }

            if elif_else_clauses.iter().any(|clause| {
                clause
                    .body
                    .last()
                    .is_some_and(|last| has_implicit_return(checker, last))
            }) {
                return true;
            }

            // Check if we don't have an else clause
            matches!(
                elif_else_clauses.last(),
                None | Some(ast::ElifElseClause { test: Some(_), .. })
            )
        }
        Stmt::Assert(ast::StmtAssert { test, .. }) if is_const_false(test) => false,
        Stmt::While(ast::StmtWhile { test, .. }) if is_const_true(test) => false,
        Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
            if let Some(last_stmt) = orelse.last() {
                has_implicit_return(checker, last_stmt)
            } else {
                true
            }
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases.iter().any(|case| {
            case.body
                .last()
                .is_some_and(|last| has_implicit_return(checker, last))
        }),
        Stmt::With(ast::StmtWith { body, .. }) => body
            .last()
            .is_some_and(|last_stmt| has_implicit_return(checker, last_stmt)),
        Stmt::Return(_) | Stmt::Raise(_) | Stmt::Try(_) => false,
        Stmt::Expr(ast::StmtExpr { value, .. })
            if matches!(
                value.as_ref(),
                Expr::Call(ast::ExprCall { func, ..  })
                    if is_noreturn_func(func, checker.semantic())
            ) =>
        {
            false
        }
        _ => true,
    }
}

/// RET503
fn implicit_return(checker: &Checker, function_def: &ast::StmtFunctionDef, stmt: &Stmt) {
    if has_implicit_return(checker, stmt) {
        add_return_none(checker, stmt, function_def.range());
    }
}

/// RET504
pub(crate) fn unnecessary_assign(checker: &Checker, function_stmt: &Stmt) {
    let Stmt::FunctionDef(function_def) = function_stmt else {
        return;
    };
    let Some(stack) = create_stack(checker, function_def) else {
        return;
    };

    if !result_exists(&stack.returns) {
        return;
    }

    let Some(function_scope) = checker.semantic().function_scope(function_def) else {
        return;
    };
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

        let Some(assigned_binding) = function_scope
            .get(assigned_id)
            .map(|binding_id| checker.semantic().binding(binding_id))
        else {
            continue;
        };
        // Check if there's any reference made to `assigned_binding` in another scope, e.g, nested
        // functions. If there is, ignore them.
        if assigned_binding
            .references()
            .map(|reference_id| checker.semantic().reference(reference_id))
            .any(|reference| reference.scope_id() != assigned_binding.scope)
        {
            continue;
        }

        let mut diagnostic = checker.report_diagnostic(
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

            let eq_token = checker
                .tokens()
                .before(assign.value.start())
                .iter()
                .rfind(|token| token.kind() == TokenKind::Equal)
                .unwrap();

            let content = checker.source();
            // Replace the `x = 1` statement with `return 1`.
            let replace_assign = Edit::range_replacement(
                if content[eq_token.end().to_usize()..]
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
                TextRange::new(assign.start(), eq_token.range().end()),
            );

            Ok(Fix::unsafe_edits(replace_assign, [delete_return]))
        });
    }
}

/// RET505, RET506, RET507, RET508
fn superfluous_else_node(
    checker: &Checker,
    if_elif_body: &[Stmt],
    elif_else: &ElifElseClause,
) -> bool {
    let branch = if elif_else.test.is_some() {
        Branch::Elif
    } else {
        Branch::Else
    };
    let range = elif_else_range(elif_else, checker.locator().contents())
        .unwrap_or_else(|| elif_else.range());
    for child in if_elif_body {
        let diagnostic = if child.is_return_stmt() {
            checker.report_diagnostic_if_enabled(SuperfluousElseReturn { branch }, range)
        } else if child.is_break_stmt() {
            checker.report_diagnostic_if_enabled(SuperfluousElseBreak { branch }, range)
        } else if child.is_raise_stmt() {
            checker.report_diagnostic_if_enabled(SuperfluousElseRaise { branch }, range)
        } else if child.is_continue_stmt() {
            checker.report_diagnostic_if_enabled(SuperfluousElseContinue { branch }, range)
        } else {
            continue;
        };
        if let Some(mut d) = diagnostic {
            d.try_set_fix(|| remove_else(checker, elif_else));
        }
        return true;
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_elif_else(checker: &Checker, stack: &Stack) {
    for (if_elif_body, elif_else) in &stack.elifs_elses {
        superfluous_else_node(checker, if_elif_body, elif_else);
    }
}

fn create_stack<'a>(
    checker: &'a Checker,
    function_def: &'a ast::StmtFunctionDef,
) -> Option<Stack<'a>> {
    let ast::StmtFunctionDef { body, .. } = function_def;

    // Find the last statement in the function.
    let Some(last_stmt) = body.last() else {
        // Skip empty functions.
        return None;
    };

    // Skip functions that consist of a single return statement.
    if body.len() == 1 && matches!(last_stmt, Stmt::Return(_)) {
        return None;
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
        return None;
    }

    Some(stack)
}

/// Run all checks from the `flake8-return` plugin, but `RET504` which is ran
/// after the semantic model is fully built.
pub(crate) fn function(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    let ast::StmtFunctionDef {
        decorator_list,
        returns,
        body,
        ..
    } = function_def;

    let Some(stack) = create_stack(checker, function_def) else {
        return;
    };
    let Some(last_stmt) = body.last() else {
        return;
    };

    if checker.any_rule_enabled(&[
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
        if checker.is_rule_enabled(Rule::ImplicitReturnValue) {
            implicit_return_value(checker, &stack);
        }
        if checker.is_rule_enabled(Rule::ImplicitReturn) {
            implicit_return(checker, function_def, last_stmt);
        }
    } else {
        if checker.is_rule_enabled(Rule::UnnecessaryReturnNone) {
            // Skip functions that have a return annotation that is not `None`.
            if returns.as_deref().is_none_or(Expr::is_none_literal_expr) {
                unnecessary_return_none(checker, decorator_list, &stack);
            }
        }
    }
}

/// Generate a [`Fix`] to remove an `else` or `elif` clause.
fn remove_else(checker: &Checker, elif_else: &ElifElseClause) -> Result<Fix> {
    let locator = checker.locator();
    let indexer = checker.indexer();
    let stylist = checker.stylist();

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
        let Some(desired_indentation) = indentation(locator.contents(), elif_else) else {
            return Err(anyhow::anyhow!("Compound statement cannot be inlined"));
        };

        // If the statement is on the same line as the `else`, just remove the `else: `.
        // Ex) `else: return True` -> `return True`
        if let Some(first) = elif_else.body.first() {
            if indexer.preceded_by_multi_statement_line(first, locator.contents()) {
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
