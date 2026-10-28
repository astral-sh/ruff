use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Pattern, Stmt};
use ruff_python_semantic::{BindingId, BindingKind};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `case` clauses whose entire pattern is a bare name that is already bound to a
/// variable.
///
/// ## Why is this bad?
/// A bare name in a `case` clause is a capture pattern, not a value pattern. It matches any
/// subject and binds the subject to that name, so it does not compare the subject against the
/// variable's current value. Instead, it silently overwrites the variable.
///
/// This is a common mistake when matching against constants:
///
/// ```python
/// NOT_FOUND = 404
///
/// match status:
///     case NOT_FOUND:  # Matches every status and rebinds `NOT_FOUND`
///         ...
/// ```
///
/// To compare the subject against a value, use a literal or a dotted name, such as an enum
/// member. If the capture is intentional, use `case _ as name:`, which makes the binding
/// explicit and is not flagged by this rule.
///
/// ## Example
///
/// ```python
/// not_found = 404
///
///
/// def handle(status):
///     match status:
///         case 200:
///             return "ok"
///         case not_found:
///             return "not found"
/// ```
///
/// Use instead:
///
/// ```python
/// from http import HTTPStatus
///
///
/// def handle(status):
///     match status:
///         case 200:
///             return "ok"
///         case HTTPStatus.NOT_FOUND:
///             return "not found"
/// ```
///
/// ## References
/// - [Python documentation: Capture patterns](https://docs.python.org/3/reference/compound_stmts.html#capture-patterns)
/// - [Python documentation: Value patterns](https://docs.python.org/3/reference/compound_stmts.html#value-patterns)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct MatchCaptureShadowsVariable {
    name: String,
}

impl Violation for MatchCaptureShadowsVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MatchCaptureShadowsVariable { name } = self;
        format!("Capture pattern `{name}` shadows an existing variable")
    }

    fn fix_title(&self) -> Option<String> {
        let MatchCaptureShadowsVariable { name } = self;
        Some(format!(
            "Use a value pattern to compare against `{name}`, or `case _ as {name}:` to capture explicitly"
        ))
    }
}

/// RUF078
pub(crate) fn match_capture_shadows_variable(checker: &Checker, pattern: &Pattern) {
    let Pattern::MatchAs(ast::PatternMatchAs {
        pattern: None,
        name: Some(name),
        ..
    }) = pattern
    else {
        return;
    };

    let semantic = checker.semantic();

    // Only consider captures that form the whole `case` pattern, like `case name:`. Nested
    // captures, like `case Point(x=x)`, commonly rebind existing names on purpose.
    let Stmt::Match(ast::StmtMatch { cases, .. }) = semantic.current_statement() else {
        return;
    };
    if !cases
        .iter()
        .any(|case| case.pattern.range() == pattern.range())
    {
        return;
    }

    // The capture's own binding has already been added to the current scope.
    let scope = semantic.current_scope();
    let Some(capture_id) = scope.get(name.as_str()) else {
        return;
    };

    // Find the binding that the capture replaces: a binding in the same scope, or one that is
    // visible from an enclosing scope. Class scopes are skipped because their variables are not
    // visible from nested scopes.
    let shadowed_id: Option<BindingId> = scope.shadowed_binding(capture_id).or_else(|| {
        semantic
            .scopes
            .ancestors(semantic.scope_id)
            .skip(1)
            .filter(|ancestor| !ancestor.kind.is_class())
            .find_map(|ancestor| ancestor.get(name.as_str()))
    });
    let Some(shadowed_id) = shadowed_id else {
        return;
    };

    let shadowed = semantic.binding(shadowed_id);
    if matches!(
        shadowed.kind,
        BindingKind::Builtin
            | BindingKind::Annotation
            | BindingKind::Deletion
            | BindingKind::UnboundException(_)
    ) {
        return;
    }

    // Ignore names that were themselves bound by an earlier capture pattern, like reusing `x`
    // across several `match` statements.
    if shadowed
        .source
        .is_some_and(|source| semantic.statement(source).is_match_stmt())
    {
        return;
    }

    checker.report_diagnostic(
        MatchCaptureShadowsVariable {
            name: name.to_string(),
        },
        name.range(),
    );
}
