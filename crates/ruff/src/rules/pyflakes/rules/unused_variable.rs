use itertools::Itertools;
use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Location, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::helpers::contains_effect;
use crate::ast::types::{BindingKind, Range, RefEquality, ScopeKind};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
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

fn match_token_after<F>(stmt: &Stmt, locator: &Locator, f: F) -> Location
where
    F: Fn(Tok) -> bool,
{
    let contents = locator.slice_source_code_range(&Range::from_located(stmt));
    for ((_, tok, _), (start, ..)) in lexer::make_tokenizer_located(contents, stmt.location)
        .flatten()
        .tuple_windows()
    {
        if f(tok) {
            return start;
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
        if targets.len() == 1 && matches!(targets[0].node, ExprKind::Name { .. }) {
            return if contains_effect(checker, value) {
                // If the expression is complex (`x = foo()`), remove the assignment,
                // but preserve the right-hand side.
                Some((
                    DeletionKind::Partial,
                    Fix::deletion(
                        stmt.location,
                        match_token_after(stmt, checker.locator, |tok| tok == Tok::Equal),
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
                        match_token_after(stmt, checker.locator, |tok| tok == Tok::Equal),
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
                            optional_vars.end_location.unwrap(),
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
