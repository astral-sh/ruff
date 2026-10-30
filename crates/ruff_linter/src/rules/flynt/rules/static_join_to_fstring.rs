use ast::FStringFlags;
use itertools::Itertools;
use ruff_python_ast::flat;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, StringFlags, str::Quote};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::pad;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// ## Fix safety
/// The fix is always marked unsafe because the evaluation of the f-string
/// expressions will default to calling the `__format__` method of each
/// object, whereas `str.join` expects each object to be an instance of
/// `str` and uses the corresponding string. Therefore it is possible for
/// the values of the resulting strings to differ, or for one expression
/// to raise an exception while the other does not.
///
/// ## References
/// - [Python documentation: f-strings](https://docs.python.org/3/reference/lexical_analysis.html#f-strings)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.266", category = Category::Complexity)]
pub(crate) struct StaticJoinToFString {
    expression: SourceCodeSnippet,
}

impl AlwaysFixableViolation for StaticJoinToFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticJoinToFString { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Consider `{expression}` instead of string join")
        } else {
            "Consider f-string instead of string join".to_string()
        }
    }

    fn fix_title(&self) -> String {
        let StaticJoinToFString { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Replace with `{expression}`")
        } else {
            "Replace with f-string".to_string()
        }
    }
}

fn is_static_length(elts: flat::NodeList<'_, Expr>) -> bool {
    elts.iter().all(|e| !e.is_starred_expr())
}

/// Build an f-string consisting of `joinees` joined by `joiner` with `flags`.
fn build_fstring(
    joiner: &str,
    joinees: flat::NodeList<'_, Expr>,
    flags: FStringFlags,
) -> Option<Expr> {
    // If all elements are string constants, join them into a single string.
    if joinees.iter().all(flat::View::is_string_literal_expr) {
        let mut flags: Option<ast::StringLiteralFlags> = None;
        let content = joinees
            .iter()
            .filter_map(|expr| {
                if let flat::ExprKind::StringLiteral(string_literal) = expr.classify() {
                    let value = string_literal.value();
                    if flags.is_none() {
                        // Take the flags from the first Expr
                        flags = Some(value.first_literal_flags());
                    }
                    Some(value.to_str())
                } else {
                    None
                }
            })
            .join(joiner);

        let mut flags = flags?;

        // If the result is a raw string and contains a newline, use triple quotes.
        if flags.prefix().is_raw() && content.contains(['\n', '\r']) {
            flags = flags.with_triple_quotes(ruff_python_ast::str::TripleQuotes::Yes);

            // Prefer a delimiter that doesn't occur in the content; if both occur, bail.
            if content.contains(flags.quote_str()) {
                flags = flags.with_quote_style(flags.quote_style().opposite());
                if content.contains(flags.quote_str()) {
                    // Both "'''" and "\"\"\"" are present in content; avoid emitting
                    // an invalid raw triple-quoted literal (or escaping). Bail on the fix.
                    return None;
                }
            }
        }

        let node = ast::StringLiteral {
            value: content.into_boxed_str(),
            flags,
            range: TextRange::default(),
            node_index: ruff_python_ast::NodeIndex::NONE,
        };
        return Some(node.into());
    }

    let mut f_string_elements = Vec::with_capacity(joinees.len() * 2);
    let mut has_single_quote = joiner.contains('\'');
    let mut has_double_quote = joiner.contains('"');
    let mut first = true;

    for expr in joinees {
        if expr.is_f_string_expr() {
            // Oops, already an f-string. We don't know how to handle those
            // gracefully right now.
            return None;
        }
        if !std::mem::take(&mut first) {
            f_string_elements.push(helpers::to_interpolated_string_literal_element(joiner));
        }
        let element = helpers::to_interpolated_string_element(expr)?;
        if let ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
            value,
            ..
        }) = &element
        {
            has_single_quote |= value.contains('\'');
            has_double_quote |= value.contains('"');
        }
        f_string_elements.push(element);
    }

    let quote = flags.quote_style();
    let adjusted_quote = match quote {
        Quote::Single if has_single_quote && !has_double_quote => quote.opposite(),
        Quote::Double if has_double_quote && !has_single_quote => quote.opposite(),
        _ if has_double_quote && has_single_quote => return None,
        _ => quote,
    };

    let node = ast::FString {
        elements: f_string_elements.into(),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
        flags: flags.with_quote_style(adjusted_quote),
    };
    Some(node.into())
}

/// FLY002
pub(crate) fn static_join_to_fstring(checker: &Checker, expr: flat::View<'_, Expr>, joiner: &str) {
    let flat::ExprKind::Call(call_node) = expr.classify() else {
        return;
    };
    let arguments = call_node.arguments();
    let args = arguments.args();
    let keywords = arguments.keywords();

    // If there are kwargs or more than one argument, this is some non-standard
    // string join call.
    if !keywords.is_empty() {
        return;
    }
    let Some([arg]) = args.as_array() else {
        return;
    };

    // Get the elements to join; skip (e.g.) generators, sets, etc.
    let joinees = match arg.classify() {
        flat::ExprKind::List(expr_list) if is_static_length(expr_list.elts()) => expr_list.elts(),
        flat::ExprKind::Tuple(expr_tuple) if is_static_length(expr_tuple.elts()) => {
            expr_tuple.elts()
        }
        _ => return,
    };

    // Try to build the fstring (internally checks whether e.g. the elements are
    // convertible to f-string elements).
    let Some(new_expr) = build_fstring(joiner, joinees, checker.default_fstring_flags()) else {
        return;
    };

    let contents = checker.generator().expr(&new_expr);

    let mut diagnostic = checker.report_diagnostic(
        StaticJoinToFString {
            expression: SourceCodeSnippet::new(contents.clone()),
        },
        expr.range(),
    );
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        pad(contents, expr.range(), checker.locator()),
        expr.range(),
    )));
}
