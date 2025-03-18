use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::registry::Rule;

/// ## What it does
/// Checks for `try-except` blocks with duplicate exception handlers.
///
/// ## Why is this bad?
/// Duplicate exception handlers are redundant, as the first handler will catch
/// the exception, making the second handler unreachable.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[derive(ViolationMetadata)]
pub(crate) struct DuplicateTryBlockException {
    name: String,
    is_star: bool,
}

impl Violation for DuplicateTryBlockException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateTryBlockException { name, is_star } = self;
        if *is_star {
            format!("try-except* block with duplicate exception `{name}`")
        } else {
            format!("try-except block with duplicate exception `{name}`")
        }
    }
}

/// ## What it does
/// Checks for exception handlers that catch duplicate exceptions.
///
/// ## Why is this bad?
/// Including the same exception multiple times in the same handler is redundant,
/// as the first exception will catch the exception, making the second exception
/// unreachable. The same applies to exception hierarchies, as a handler for a
/// parent exception (like `Exception`) will also catch child exceptions (like
/// `ValueError`).
///
/// ## Example
/// ```python
/// try:
///     ...
/// except (Exception, ValueError):  # `Exception` includes `ValueError`.
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except Exception:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
#[derive(ViolationMetadata)]
pub(crate) struct DuplicateHandlerException {
    pub names: Vec<String>,
}

impl AlwaysFixableViolation for DuplicateHandlerException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateHandlerException { names } = self;
        if let [name] = names.as_slice() {
            format!("Exception handler with duplicate exception: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Exception handler with duplicate exceptions: {names}")
        }
    }

    fn fix_title(&self) -> String {
        "De-duplicate exceptions".to_string()
    }
}

fn type_pattern(elts: Vec<&Expr>) -> Expr {
    ast::ExprTuple {
        elts: elts.into_iter().cloned().collect(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
        parenthesized: true,
    }
    .into()
}

/// B014
fn duplicate_handler_exceptions<'a>(
    checker: &Checker,
    expr: &'a Expr,
    elts: &'a [Expr],
) -> FxHashMap<UnqualifiedName<'a>, &'a Expr> {
    let mut seen: FxHashMap<UnqualifiedName, &Expr> = FxHashMap::default();
    let mut duplicates: FxHashSet<UnqualifiedName> = FxHashSet::default();
    let mut unique_elts: Vec<&Expr> = Vec::default();
    for type_ in elts {
        if let Some(name) = UnqualifiedName::from_expr(type_) {
            if seen.contains_key(&name) {
                duplicates.insert(name);
            } else {
                seen.entry(name).or_insert(type_);
                unique_elts.push(type_);
            }
        }
    }

    if checker.enabled(Rule::DuplicateHandlerException) {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut diagnostic = Diagnostic::new(
                DuplicateHandlerException {
                    names: duplicates
                        .into_iter()
                        .map(|qualified_name| qualified_name.segments().join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                },
                expr.range(),
            );
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                // Single exceptions don't require parentheses, but since we're _removing_
                // parentheses, insert whitespace as needed.
                if let [elt] = unique_elts.as_slice() {
                    pad(
                        checker.generator().expr(elt),
                        expr.range(),
                        checker.locator(),
                    )
                } else {
                    // Multiple exceptions must always be parenthesized. This is done
                    // manually as the generator never parenthesizes lone tuples.
                    format!("({})", checker.generator().expr(&type_pattern(unique_elts)))
                },
                expr.range(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }

    seen
}

/// B025
pub(crate) fn duplicate_exceptions(checker: &Checker, handlers: &[ExceptHandler]) {
    let mut seen: FxHashSet<UnqualifiedName> = FxHashSet::default();
    let mut duplicates: FxHashMap<UnqualifiedName, Vec<&Expr>> = FxHashMap::default();
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_: Some(type_),
            ..
        }) = handler
        else {
            continue;
        };
        match type_.as_ref() {
            Expr::Attribute(_) | Expr::Name(_) => {
                if let Some(name) = UnqualifiedName::from_expr(type_) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(type_);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for (name, expr) in duplicate_handler_exceptions(checker, type_, elts) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(expr);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            _ => {}
        }
    }

    if checker.enabled(Rule::DuplicateTryBlockException) {
        for (name, exprs) in duplicates {
            for expr in exprs {
                let is_star = checker
                    .semantic()
                    .current_statement()
                    .as_try_stmt()
                    .is_some_and(|try_stmt| try_stmt.is_star);
                checker.report_diagnostic(Diagnostic::new(
                    DuplicateTryBlockException {
                        name: name.segments().join("."),
                        is_star,
                    },
                    expr.range(),
                ));
            }
        }
    }
}
