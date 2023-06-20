use itertools::Itertools;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{self, Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::ScopeId;

use crate::autofix::edits::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for the presence of unused variables in function scopes.
///
/// ## Why is this bad?
/// A variable that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// def foo():
///     x = 1
///     y = 2
///     return x
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     x = 1
///     return x
/// ```
///
/// ## Options
/// - `dummy-variable-rgx`
#[violation]
pub struct UnusedVariable {
    pub name: String,
}

impl Violation for UnusedVariable {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedVariable { name } = self;
        format!("Local variable `{name}` is assigned to but never used")
    }

    fn autofix_title(&self) -> Option<String> {
        let UnusedVariable { name } = self;
        Some(format!("Remove assignment to unused variable `{name}`"))
    }
}

/// Return the [`TextRange`] of the token before the next match of the predicate
fn match_token_before<F>(location: TextSize, locator: &Locator, f: F) -> Option<TextRange>
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.after(location);
    for ((_, range), (tok, _)) in lexer::lex_starts_at(contents, Mode::Module, location)
        .flatten()
        .tuple_windows()
    {
        if f(tok) {
            return Some(range);
        }
    }
    None
}

/// Return the [`TextRange`] of the token after the next match of the predicate, skipping over
/// any bracketed expressions.
fn match_token_after<F>(location: TextSize, locator: &Locator, f: F) -> Option<TextRange>
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.after(location);

    // Track the bracket depth.
    let mut par_count = 0u32;
    let mut sqb_count = 0u32;
    let mut brace_count = 0u32;

    for ((tok, _), (_, range)) in lexer::lex_starts_at(contents, Mode::Module, location)
        .flatten()
        .tuple_windows()
    {
        match tok {
            Tok::Lpar => {
                par_count = par_count.saturating_add(1);
            }
            Tok::Lsqb => {
                sqb_count = sqb_count.saturating_add(1);
            }
            Tok::Lbrace => {
                brace_count = brace_count.saturating_add(1);
            }
            Tok::Rpar => {
                par_count = par_count.saturating_sub(1);
            }
            Tok::Rsqb => {
                sqb_count = sqb_count.saturating_sub(1);
            }
            Tok::Rbrace => {
                brace_count = brace_count.saturating_sub(1);
            }
            _ => {}
        }

        // If we're in nested brackets, continue.
        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        if f(tok) {
            return Some(range);
        }
    }
    None
}

/// Return the [`TextRange`] of the token matching the predicate or the first mismatched
/// bracket, skipping over any bracketed expressions.
fn match_token_or_closing_brace<F>(location: TextSize, locator: &Locator, f: F) -> Option<TextRange>
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.after(location);

    // Track the bracket depth.
    let mut par_count = 0u32;
    let mut sqb_count = 0u32;
    let mut brace_count = 0u32;

    for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, location).flatten() {
        match tok {
            Tok::Lpar => {
                par_count = par_count.saturating_add(1);
            }
            Tok::Lsqb => {
                sqb_count = sqb_count.saturating_add(1);
            }
            Tok::Lbrace => {
                brace_count = brace_count.saturating_add(1);
            }
            Tok::Rpar => {
                if par_count == 0 {
                    return Some(range);
                }
                par_count = par_count.saturating_sub(1);
            }
            Tok::Rsqb => {
                if sqb_count == 0 {
                    return Some(range);
                }
                sqb_count = sqb_count.saturating_sub(1);
            }
            Tok::Rbrace => {
                if brace_count == 0 {
                    return Some(range);
                }
                brace_count = brace_count.saturating_sub(1);
            }
            _ => {}
        }

        // If we're in nested brackets, continue.
        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        if f(tok) {
            return Some(range);
        }
    }
    None
}

/// Generate a [`Edit`] to remove an unused variable assignment, given the
/// enclosing [`Stmt`] and the [`TextRange`] of the variable binding.
fn remove_unused_variable(
    stmt: &Stmt,
    parent: Option<&Stmt>,
    range: TextRange,
    checker: &Checker,
) -> Option<Fix> {
    // First case: simple assignment (`x = 1`)
    if let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
        if let Some(target) = targets.iter().find(|target| range == target.range()) {
            if target.is_name_expr() {
                return if targets.len() > 1
                    || contains_effect(value, |id| checker.semantic().is_builtin(id))
                {
                    // If the expression is complex (`x = foo()`), remove the assignment,
                    // but preserve the right-hand side.
                    let start = target.start();
                    let end =
                        match_token_after(start, checker.locator, |tok| tok == Tok::Equal)?.start();
                    let edit = Edit::deletion(start, end);
                    Some(Fix::suggested(edit))
                } else {
                    // If (e.g.) assigning to a constant (`x = 1`), delete the entire statement.
                    let edit = delete_stmt(stmt, parent, checker.locator, checker.indexer);
                    Some(Fix::suggested(edit).isolate(checker.isolation(parent)))
                };
            }
        }
    }

    // Second case: simple annotated assignment (`x: int = 1`)
    if let Stmt::AnnAssign(ast::StmtAnnAssign {
        target,
        value: Some(value),
        ..
    }) = stmt
    {
        if target.is_name_expr() {
            return if contains_effect(value, |id| checker.semantic().is_builtin(id)) {
                // If the expression is complex (`x = foo()`), remove the assignment,
                // but preserve the right-hand side.
                let start = stmt.start();
                let end =
                    match_token_after(start, checker.locator, |tok| tok == Tok::Equal)?.start();
                let edit = Edit::deletion(start, end);
                Some(Fix::suggested(edit))
            } else {
                // If (e.g.) assigning to a constant (`x = 1`), delete the entire statement.
                let edit = delete_stmt(stmt, parent, checker.locator, checker.indexer);
                Some(Fix::suggested(edit).isolate(checker.isolation(parent)))
            };
        }
    }

    // Third case: with_item (`with foo() as x:`)
    if let Stmt::With(ast::StmtWith { items, .. }) = stmt {
        // Find the binding that matches the given `Range`.
        // TODO(charlie): Store the `WithItem` in the `Binding`.
        for item in items {
            if let Some(optional_vars) = &item.optional_vars {
                if optional_vars.range() == range {
                    // Find the first token before the `as` keyword.
                    let start =
                        match_token_before(item.context_expr.start(), checker.locator, |tok| {
                            tok == Tok::As
                        })?
                        .end();

                    // Find the first colon, comma, or closing bracket after the `as` keyword.
                    let end = match_token_or_closing_brace(start, checker.locator, |tok| {
                        tok == Tok::Colon || tok == Tok::Comma
                    })?
                    .start();

                    let edit = Edit::deletion(start, end);
                    return Some(Fix::suggested(edit));
                }
            }
        }
    }

    None
}

/// F841
pub(crate) fn unused_variable(checker: &mut Checker, scope: ScopeId) {
    let scope = &checker.semantic().scopes[scope];
    if scope.uses_locals() && scope.kind.is_any_function() {
        return;
    }

    let bindings: Vec<_> = scope
        .bindings()
        .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)))
        .filter_map(|(name, binding)| {
            if (binding.kind.is_assignment() || binding.kind.is_named_expr_assignment())
                && !binding.is_nonlocal()
                && !binding.is_global()
                && !binding.is_used()
                && !checker.settings.dummy_variable_rgx.is_match(name)
                && name != "__tracebackhide__"
                && name != "__traceback_info__"
                && name != "__traceback_supplement__"
                && name != "__debuggerskip__"
            {
                return Some((name.to_string(), binding.range, binding.source));
            }

            None
        })
        .collect();

    for (name, range, source) in bindings {
        let mut diagnostic = Diagnostic::new(UnusedVariable { name }, range);
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(source) = source {
                let stmt = checker.semantic().stmts[source];
                let parent = checker.semantic().stmts.parent(stmt);
                if let Some(fix) = remove_unused_variable(stmt, parent, range, checker) {
                    diagnostic.set_fix(fix);
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
