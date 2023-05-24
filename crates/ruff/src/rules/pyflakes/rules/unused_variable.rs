use itertools::Itertools;
use log::error;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::RefEquality;
use ruff_python_semantic::scope::{ScopeId, ScopeKind};

use crate::autofix::actions::delete_stmt;
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
/// ## Options
/// - `dummy-variable-rgx`
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

/// Return the [`TextRange`] of the token after the next match of
/// the predicate, skipping over any bracketed expressions.
fn match_token_after<F, T>(located: &T, locator: &Locator, f: F) -> TextRange
where
    F: Fn(Tok) -> bool,
    T: Ranged,
{
    let contents = locator.after(located.start());

    // Track the bracket depth.
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for ((tok, _), (_, range)) in lexer::lex_starts_at(contents, Mode::Module, located.start())
        .flatten()
        .tuple_windows()
    {
        match tok {
            Tok::Lpar => {
                par_count += 1;
            }
            Tok::Lsqb => {
                sqb_count += 1;
            }
            Tok::Lbrace => {
                brace_count += 1;
            }
            Tok::Rpar => {
                par_count -= 1;
                // If this is a closing bracket, continue.
                if par_count == 0 {
                    continue;
                }
            }
            Tok::Rsqb => {
                sqb_count -= 1;
                // If this is a closing bracket, continue.
                if sqb_count == 0 {
                    continue;
                }
            }
            Tok::Rbrace => {
                brace_count -= 1;
                // If this is a closing bracket, continue.
                if brace_count == 0 {
                    continue;
                }
            }
            _ => {}
        }
        // If we're in nested brackets, continue.
        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        if f(tok) {
            return range;
        }
    }
    unreachable!("No token after matched");
}

/// Return the [`TextRange`] of the token matching the predicate,
/// skipping over any bracketed expressions.
fn match_token<F, T>(located: &T, locator: &Locator, f: F) -> TextRange
where
    F: Fn(Tok) -> bool,
    T: Ranged,
{
    let contents = locator.after(located.start());

    // Track the bracket depth.
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, located.start()).flatten() {
        match tok {
            Tok::Lpar => {
                par_count += 1;
            }
            Tok::Lsqb => {
                sqb_count += 1;
            }
            Tok::Lbrace => {
                brace_count += 1;
            }
            Tok::Rpar => {
                par_count -= 1;
                // If this is a closing bracket, continue.
                if par_count == 0 {
                    continue;
                }
            }
            Tok::Rsqb => {
                sqb_count -= 1;
                // If this is a closing bracket, continue.
                if sqb_count == 0 {
                    continue;
                }
            }
            Tok::Rbrace => {
                brace_count -= 1;
                // If this is a closing bracket, continue.
                if brace_count == 0 {
                    continue;
                }
            }
            _ => {}
        }
        // If we're in nested brackets, continue.
        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        if f(tok) {
            return range;
        }
    }
    unreachable!("No token after matched");
}

#[derive(Copy, Clone)]
enum DeletionKind {
    Whole,
    Partial,
}

/// Generate a [`Edit`] to remove an unused variable assignment, given the
/// enclosing [`Stmt`] and the [`TextRange`] of the variable binding.
fn remove_unused_variable(
    stmt: &Stmt,
    range: TextRange,
    checker: &Checker,
) -> Option<(DeletionKind, Fix)> {
    // First case: simple assignment (`x = 1`)
    if let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
        if let Some(target) = targets.iter().find(|target| range == target.range()) {
            if target.is_name_expr() {
                return if targets.len() > 1
                    || contains_effect(value, |id| checker.semantic_model().is_builtin(id))
                {
                    // If the expression is complex (`x = foo()`), remove the assignment,
                    // but preserve the right-hand side.
                    #[allow(deprecated)]
                    Some((
                        DeletionKind::Partial,
                        Fix::unspecified(Edit::deletion(
                            target.start(),
                            match_token_after(target, checker.locator, |tok| tok == Tok::Equal)
                                .start(),
                        )),
                    ))
                } else {
                    // If (e.g.) assigning to a constant (`x = 1`), delete the entire statement.
                    let parent = checker.semantic_model().stmts.parent(stmt);
                    let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                    match delete_stmt(
                        stmt,
                        parent,
                        &deleted,
                        checker.locator,
                        checker.indexer,
                        checker.stylist,
                    ) {
                        #[allow(deprecated)]
                        Ok(fix) => Some((DeletionKind::Whole, Fix::unspecified(fix))),
                        Err(err) => {
                            error!("Failed to delete unused variable: {}", err);
                            None
                        }
                    }
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
            return if contains_effect(value, |id| checker.semantic_model().is_builtin(id)) {
                // If the expression is complex (`x = foo()`), remove the assignment,
                // but preserve the right-hand side.
                #[allow(deprecated)]
                Some((
                    DeletionKind::Partial,
                    Fix::unspecified(Edit::deletion(
                        stmt.start(),
                        match_token_after(stmt, checker.locator, |tok| tok == Tok::Equal).start(),
                    )),
                ))
            } else {
                // If assigning to a constant (`x = 1`), delete the entire statement.
                let parent = checker.semantic_model().stmts.parent(stmt);
                let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                match delete_stmt(
                    stmt,
                    parent,
                    &deleted,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                ) {
                    #[allow(deprecated)]
                    Ok(edit) => Some((DeletionKind::Whole, Fix::unspecified(edit))),
                    Err(err) => {
                        error!("Failed to delete unused variable: {}", err);
                        None
                    }
                }
            };
        }
    }

    // Third case: withitem (`with foo() as x:`)
    if let Stmt::With(ast::StmtWith { items, .. }) = stmt {
        // Find the binding that matches the given `Range`.
        // TODO(charlie): Store the `Withitem` in the `Binding`.
        for item in items {
            if let Some(optional_vars) = &item.optional_vars {
                if optional_vars.range() == range {
                    #[allow(deprecated)]
                    return Some((
                        DeletionKind::Partial,
                        Fix::unspecified(Edit::deletion(
                            item.context_expr.end(),
                            // The end of the `Withitem` is the colon, comma, or closing
                            // parenthesis following the `optional_vars`.
                            match_token(&item.context_expr, checker.locator, |tok| {
                                tok == Tok::Colon || tok == Tok::Comma || tok == Tok::Rpar
                            })
                            .start(),
                        )),
                    ));
                }
            }
        }
    }

    None
}

/// F841
pub(crate) fn unused_variable(checker: &mut Checker, scope: ScopeId) {
    let scope = &checker.semantic_model().scopes[scope];
    if scope.uses_locals && matches!(scope.kind, ScopeKind::Function(..)) {
        return;
    }

    let bindings: Vec<_> = scope
        .bindings()
        .map(|(name, binding_id)| (*name, &checker.semantic_model().bindings[*binding_id]))
        .filter_map(|(name, binding)| {
            if (binding.kind.is_assignment() || binding.kind.is_named_expr_assignment())
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
                let stmt = checker.semantic_model().stmts[source];
                if let Some((kind, fix)) = remove_unused_variable(stmt, range, checker) {
                    if matches!(kind, DeletionKind::Whole) {
                        checker.deletions.insert(RefEquality(stmt));
                    }
                    diagnostic.set_fix(fix);
                }
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}
