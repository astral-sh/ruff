use ast::FStringFlags;
use itertools::Itertools;

use crate::fix::edits::pad;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

use crate::rules::flynt::helpers;

/// ## What it does
/// Checks for `str.join` calls that can be replaced with f-strings.
///
/// ## Why is this bad?
/// f-strings are more readable and generally preferred over `str.join` calls.
///
/// ## Example
/// ```python
/// " ".join((foo, bar))
/// ```
///
/// Use instead:
/// ```python
/// f"{foo} {bar}"
/// ```
///
/// ## References
/// - [Python documentation: f-strings](https://docs.python.org/3/reference/lexical_analysis.html#f-strings)
#[violation]
pub struct StaticJoinToFString {
    expression: SourceCodeSnippet,
}

impl AlwaysFixableViolation for StaticJoinToFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticJoinToFString { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Consider `{expression}` instead of string join")
        } else {
            format!("Consider f-string instead of string join")
        }
    }

    fn fix_title(&self) -> String {
        let StaticJoinToFString { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Replace with `{expression}`")
        } else {
            format!("Replace with f-string")
        }
    }
}

fn is_static_length(elts: &[Expr]) -> bool {
    elts.iter().all(|e| !e.is_starred_expr())
}

fn build_fstring(joiner: &str, joinees: &[Expr]) -> Option<Expr> {
    // If all elements are string constants, join them into a single string.
    if joinees.iter().all(Expr::is_string_literal_expr) {
        let node = ast::StringLiteral {
            value: joinees
                .iter()
                .filter_map(|expr| {
                    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = expr {
                        Some(value.to_str())
                    } else {
                        None
                    }
                })
                .join(joiner)
                .into_boxed_str(),
            ..ast::StringLiteral::default()
        };
        return Some(node.into());
    }

    let mut f_string_elements = Vec::with_capacity(joinees.len() * 2);
    let mut first = true;

    for expr in joinees {
        if expr.is_f_string_expr() {
            // Oops, already an f-string. We don't know how to handle those
            // gracefully right now.
            return None;
        }
        if !std::mem::take(&mut first) {
            f_string_elements.push(helpers::to_f_string_literal_element(joiner));
        }
        f_string_elements.push(helpers::to_f_string_element(expr)?);
    }

    let node = ast::FString {
        elements: f_string_elements.into(),
        range: TextRange::default(),
        flags: FStringFlags::default(),
    };
    Some(node.into())
}

/// FLY002
pub(crate) fn static_join_to_fstring(checker: &mut Checker, expr: &Expr, joiner: &str) {
    let Expr::Call(ast::ExprCall {
        arguments: Arguments { args, keywords, .. },
        ..
    }) = expr
    else {
        return;
    };

    // If there are kwargs or more than one argument, this is some non-standard
    // string join call.
    if !keywords.is_empty() {
        return;
    }
    let [arg] = &**args else {
        return;
    };

    // Get the elements to join; skip (e.g.) generators, sets, etc.
    let joinees = match &arg {
        Expr::List(ast::ExprList { elts, .. }) if is_static_length(elts) => elts,
        Expr::Tuple(ast::ExprTuple { elts, .. }) if is_static_length(elts) => elts,
        _ => return,
    };

    // Try to build the fstring (internally checks whether e.g. the elements are
    // convertible to f-string elements).
    let Some(new_expr) = build_fstring(joiner, joinees) else {
        return;
    };

    let contents = checker.generator().expr(&new_expr);

    let mut diagnostic = Diagnostic::new(
        StaticJoinToFString {
            expression: SourceCodeSnippet::new(contents.clone()),
        },
        expr.range(),
    );
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        pad(contents, expr.range(), checker.locator()),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
