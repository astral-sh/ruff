use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, ExprContext, Operator};
use ruff_python_parser::typing::parse_type_annotation;
use ruff_python_semantic::{SemanticModel, TypingOnlyBindingsStatus};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_type_checking::helpers::quote_annotation;
use crate::settings::types::PythonVersion;
use crate::settings::LinterSettings;
use crate::Locator;

/// ## What it does
/// Checks for the presence of string literals in `X | Y`-style union types.
///
/// ## Why is this bad?
/// [PEP 604] introduced a new syntax for union type annotations based on the
/// `|` operator.
///
/// While Python's type annotations can typically be wrapped in strings to
/// avoid runtime evaluation, the use of a string member within an `X | Y`-style
/// union type will cause a runtime error.
///
/// Instead, remove the quotes, wrap the _entire_ union in quotes, or use
/// `from __future__ import annotations` to disable runtime evaluation of
/// annotations entirely.
///
/// ## Example
/// ```python
/// var: str | "int"
/// ```
///
/// Use instead:
/// ```python
/// var: str | int
/// ```
///
/// Or, extend the quotes to include the entire union:
/// ```python
/// var: "str | int"
/// ```
///
/// ## Fix safety
/// This fix is safe as long as the fix doesn't remove a comment, which can happen
/// when the union spans multiple lines.
///
/// ## References
/// - [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[derive(ViolationMetadata)]
pub(crate) struct RuntimeStringUnion {
    strategy: Option<Strategy>,
}

impl Violation for RuntimeStringUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid string member in `X | Y`-style union type".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let Self {
            strategy: Some(strategy),
            ..
        } = self
        else {
            return None;
        };
        match strategy {
            Strategy::RemoveQuotes => Some("Remove quotes".to_string()),
            Strategy::ExtendQuotes => Some("Extend quotes".to_string()),
        }
    }
}

/// TC010
pub(crate) fn runtime_string_union(checker: &Checker, expr: &Expr) {
    if !checker.semantic().in_type_definition() {
        return;
    }

    // The union is only problematic at runtime. Even though stub files are never
    // executed, some of the nodes still end up having a runtime execution context
    if checker.source_type.is_stub() || !checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Search for strings within the binary operator.
    let mut string_results = Vec::new();
    let quotes_are_extendable = traverse_op(
        checker.semantic(),
        checker.locator(),
        expr,
        &mut string_results,
        checker.settings,
    );

    if string_results.is_empty() {
        return;
    }

    if quotes_are_extendable
        && string_results
            .iter()
            .any(|result| !result.quotes_are_removable)
    {
        // all union members will share a single fix which extend the quotes
        // to the smallest valid type expression
        let edit = quote_annotation(
            checker
                .semantic()
                .current_expression_id()
                .expect("No current expression"),
            checker.semantic(),
            checker.stylist(),
            checker.locator(),
            checker.default_string_flags(),
        );
        let parent = expr.range().start();
        let fix = if checker.comment_ranges().intersects(expr.range()) {
            Fix::unsafe_edit(edit)
        } else {
            Fix::safe_edit(edit)
        };

        for result in string_results {
            let mut diagnostic = Diagnostic::new(
                RuntimeStringUnion {
                    strategy: Some(Strategy::ExtendQuotes),
                },
                result.string.range(),
            );
            diagnostic.set_parent(parent);
            diagnostic.set_fix(fix.clone());
            checker.report_diagnostic(diagnostic);
        }
        return;
    }

    // all union members will have their own fix which removes the quotes
    for result in string_results {
        let strategy = if result.quotes_are_removable {
            Some(Strategy::RemoveQuotes)
        } else {
            None
        };
        let mut diagnostic =
            Diagnostic::new(RuntimeStringUnion { strategy }, result.string.range());
        // we can only fix string literals, not bytes literals
        if result.quotes_are_removable {
            let string = result
                .string
                .as_string_literal_expr()
                .expect("Expected string literal");
            let edit = Edit::range_replacement(string.value.to_string(), string.range());
            if checker.comment_ranges().intersects(string.range()) {
                diagnostic.set_fix(Fix::unsafe_edit(edit));
            } else {
                diagnostic.set_fix(Fix::safe_edit(edit));
            }
        }
        checker.report_diagnostic(diagnostic);
    }
}

struct StringResult<'a> {
    pub string: &'a Expr,
    pub quotes_are_removable: bool,
}

/// Collect all string members in possibly-nested binary `|` expressions.
/// Returns whether or not the quotes can be expanded to the entire union
fn traverse_op<'a>(
    semantic: &'_ SemanticModel,
    locator: &'_ Locator,
    expr: &'a Expr,
    strings: &mut Vec<StringResult<'a>>,
    settings: &'_ LinterSettings,
) -> bool {
    match expr {
        Expr::StringLiteral(literal) => {
            if let Ok(result) = parse_type_annotation(literal, locator.contents()) {
                strings.push(StringResult {
                    string: expr,
                    quotes_are_removable: quotes_are_removable(
                        semantic,
                        result.expression(),
                        settings,
                    ),
                });
                // the only time quotes can be extended is if all quoted expression
                // can be parsed as forward references
                true
            } else {
                strings.push(StringResult {
                    string: expr,
                    quotes_are_removable: false,
                });
                false
            }
        }
        Expr::BytesLiteral(_) => {
            strings.push(StringResult {
                string: expr,
                quotes_are_removable: false,
            });
            false
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            right,
            op: Operator::BitOr,
            ..
        }) => {
            // we don't want short-circuiting here, since we need to collect
            // string results from both branches
            traverse_op(semantic, locator, left, strings, settings)
                & traverse_op(semantic, locator, right, strings, settings)
        }
        _ => true,
    }
}

/// Traverses the type expression and checks if the expression can safely
/// be unquoted
fn quotes_are_removable(semantic: &SemanticModel, expr: &Expr, settings: &LinterSettings) -> bool {
    match expr {
        Expr::BinOp(ast::ExprBinOp {
            left, right, op, ..
        }) => {
            match op {
                Operator::BitOr => {
                    if settings.target_version < PythonVersion::Py310 {
                        return false;
                    }
                    quotes_are_removable(semantic, left, settings)
                        && quotes_are_removable(semantic, right, settings)
                }
                // for now we'll treat uses of other operators as unremovable quotes
                // since that would make it an invalid type expression anyways. We skip
                // walking subscript
                _ => false,
            }
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx: ExprContext::Load,
            ..
        }) => quotes_are_removable(semantic, value, settings),
        // Subscript or attribute accesses that are valid type expressions may fail
        // at runtime, so we have to assume that they do, to keep code working.
        Expr::Subscript(_) | Expr::Attribute(_) => false,
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for elt in elts {
                if !quotes_are_removable(semantic, elt, settings) {
                    return false;
                }
            }
            true
        }
        Expr::Name(name) => {
            semantic.lookup_symbol(name.id.as_str()).is_none()
                || semantic
                    .simulate_runtime_load(name, TypingOnlyBindingsStatus::Disallowed)
                    .is_some()
        }
        _ => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// The quotes should be removed.
    RemoveQuotes,
    /// The quotes should be extended to cover the entire union.
    ExtendQuotes,
}
