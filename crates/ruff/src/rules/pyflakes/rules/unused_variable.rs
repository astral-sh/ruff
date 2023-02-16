use itertools::Itertools;
use log::error;
use rustpython_parser::ast::{ExprKind, Located, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::contains_effect;
use crate::ast::types::{BindingKind, Range, RefEquality, ScopeKind};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ## What it does
    /// Checks for the presence of unused variables in function scopes.
    ///
    /// ## Why is this bad?
    /// A variable that is defined but not used is likely a mistake, and should be
    /// removed to avoid confusion.
    ///
    /// If a variable is intentionally defined-but-not-used, it should be
    /// prefixed with an underscore, or some other value that adheres to the
    /// [`dummy-variable-rgx`] pattern.
    ///
    /// ## Options
    /// * `dummy-variable-rgx`
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
    pub struct UnusedVariable {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UnusedVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedVariable { name } = self;
        format!("Local variable `{name}` is assigned to but never used")
    }

    fn autofix_title(&self) -> String {
        let UnusedVariable { name } = self;
        format!("Remove assignment to unused variable `{name}`")
    }
}

/// Return the start and end [`Location`] of the token after the next match of the predicate,
/// skipping over any bracketed expressions.
fn match_token_after<F, T>(located: &Located<T>, locator: &Locator, f: F) -> Range
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.skip(located.location);

    // Track the bracket depth.
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for ((_, tok, _), (start, _, end)) in lexer::make_tokenizer_located(contents, located.location)
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
            return Range::new(start, end);
        }
    }
    unreachable!("No token after matched");
}

/// Return the start and end [`Location`] of the token matching the predicate, skipping over
/// any bracketed expressions.
fn match_token<F, T>(located: &Located<T>, locator: &Locator, f: F) -> Range
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.skip(located.location);

    // Track the bracket depth.
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for (start, tok, end) in lexer::make_tokenizer_located(contents, located.location).flatten() {
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
            return Range::new(start, end);
        }
    }
    unreachable!("No token after matched");
}

enum DeletionKind {
    Whole,
    Partial,
}

/// Generate a [`Fix`] to remove an unused variable assignment, given the
/// enclosing [`Stmt`] and the [`Range`] of the variable binding.
fn remove_unused_variable(
    stmt: &Stmt,
    range: &Range,
    checker: &Checker,
) -> Option<(DeletionKind, Fix)> {
    // First case: simple assignment (`x = 1`)
    if let StmtKind::Assign { targets, value, .. } = &stmt.node {
        if let Some(target) = targets.iter().find(|target| {
            range.location == target.location && range.end_location == target.end_location.unwrap()
        }) {
            if matches!(target.node, ExprKind::Name { .. }) {
                return if targets.len() > 1 || contains_effect(checker, value) {
                    // If the expression is complex (`x = foo()`), remove the assignment,
                    // but preserve the right-hand side.
                    Some((
                        DeletionKind::Partial,
                        Fix::deletion(
                            target.location,
                            match_token_after(target, checker.locator, |tok| tok == Tok::Equal)
                                .location,
                        ),
                    ))
                } else {
                    // If (e.g.) assigning to a constant (`x = 1`), delete the entire statement.
                    let parent = checker
                        .child_to_parent
                        .get(&RefEquality(stmt))
                        .map(std::convert::Into::into);
                    let deleted: Vec<&Stmt> = checker
                        .deletions
                        .iter()
                        .map(std::convert::Into::into)
                        .collect();
                    match delete_stmt(
                        stmt,
                        parent,
                        &deleted,
                        checker.locator,
                        checker.indexer,
                        checker.stylist,
                    ) {
                        Ok(fix) => Some((DeletionKind::Whole, fix)),
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
    if let StmtKind::AnnAssign {
        target,
        value: Some(value),
        ..
    } = &stmt.node
    {
        if matches!(target.node, ExprKind::Name { .. }) {
            return if contains_effect(checker, value) {
                // If the expression is complex (`x = foo()`), remove the assignment,
                // but preserve the right-hand side.
                Some((
                    DeletionKind::Partial,
                    Fix::deletion(
                        stmt.location,
                        match_token_after(stmt, checker.locator, |tok| tok == Tok::Equal).location,
                    ),
                ))
            } else {
                // If assigning to a constant (`x = 1`), delete the entire statement.
                let parent = checker
                    .child_to_parent
                    .get(&RefEquality(stmt))
                    .map(std::convert::Into::into);
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
                match delete_stmt(
                    stmt,
                    parent,
                    &deleted,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                ) {
                    Ok(fix) => Some((DeletionKind::Whole, fix)),
                    Err(err) => {
                        error!("Failed to delete unused variable: {}", err);
                        None
                    }
                }
            };
        }
    }

    // Third case: withitem (`with foo() as x:`)
    if let StmtKind::With { items, .. } = &stmt.node {
        // Find the binding that matches the given `Range`.
        // TODO(charlie): Store the `Withitem` in the `Binding`.
        for item in items {
            if let Some(optional_vars) = &item.optional_vars {
                if optional_vars.location == range.location
                    && optional_vars.end_location.unwrap() == range.end_location
                {
                    return Some((
                        DeletionKind::Partial,
                        Fix::deletion(
                            item.context_expr.end_location.unwrap(),
                            // The end of the `Withitem` is the colon, comma, or closing
                            // parenthesis following the `optional_vars`.
                            match_token(&item.context_expr, checker.locator, |tok| {
                                tok == Tok::Colon || tok == Tok::Comma || tok == Tok::Rpar
                            })
                            .location,
                        ),
                    ));
                }
            }
        }
    }

    None
}

/// F841
pub fn unused_variable(checker: &mut Checker, scope: usize) {
    let scope = &checker.scopes[scope];
    if scope.uses_locals && matches!(scope.kind, ScopeKind::Function(..)) {
        return;
    }

    for (name, binding) in scope
        .bindings
        .iter()
        .map(|(name, index)| (name, &checker.bindings[*index]))
    {
        if !binding.used()
            && matches!(binding.kind, BindingKind::Assignment)
            && !checker.settings.dummy_variable_rgx.is_match(name)
            && name != &"__tracebackhide__"
            && name != &"__traceback_info__"
            && name != &"__traceback_supplement__"
        {
            let mut diagnostic = Diagnostic::new(
                UnusedVariable {
                    name: (*name).to_string(),
                },
                binding.range,
            );
            if checker.patch(diagnostic.kind.rule()) {
                if let Some(stmt) = binding.source.as_ref().map(std::convert::Into::into) {
                    if let Some((kind, fix)) = remove_unused_variable(stmt, &binding.range, checker)
                    {
                        if matches!(kind, DeletionKind::Whole) {
                            checker.deletions.insert(RefEquality(stmt));
                        }
                        diagnostic.amend(fix);
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
