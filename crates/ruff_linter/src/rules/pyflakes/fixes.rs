use anyhow::{Context, Ok, Result};

use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_codegen::Stylist;
use ruff_python_semantic::Binding;
use ruff_python_trivia::{BackwardsTokenizer, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::Edit;
use crate::Locator;
use crate::cst::matchers::{match_call_mut, match_dict, transform_expression};
use crate::rules::pyflakes::format::FormatSummary;

/// Generate a [`Edit`] to remove unused keys from format dict.
pub(super) fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[usize],
    dict: &ast::ExprDict,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let source_code = locator.slice(dict);
    transform_expression(source_code, stylist, |mut expression| {
        let dict = match_dict(&mut expression)?;

        // Remove the elements at the given indexes.
        let mut index = 0;
        dict.elements.retain(|_| {
            let is_unused = unused_arguments.contains(&index);
            index += 1;
            !is_unused
        });

        Ok(expression)
    })
    .map(|output| Edit::range_replacement(output, dict.range()))
}

/// Generate a [`Edit`] to remove unused keyword arguments from a `format` call.
pub(super) fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[usize],
    call: &ast::ExprCall,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let source_code = locator.slice(call);
    transform_expression(source_code, stylist, |mut expression| {
        let call = match_call_mut(&mut expression)?;

        // Remove the keyword arguments at the given indexes.
        let mut index = 0;
        call.args.retain(|arg| {
            if arg.keyword.is_none() {
                return true;
            }

            let is_unused = unused_arguments.contains(&index);
            index += 1;
            !is_unused
        });

        Ok(expression)
    })
    .map(|output| Edit::range_replacement(output, call.range()))
}

/// Generate a [`Edit`] to remove unused positional arguments from a `format` call.
///
/// When the rewrite would leave `.format(...)` with zero arguments and the
/// format string has no replacement fields, the call itself is dropped and any
/// `{{` / `}}` escapes in the literal are reduced to single braces so the
/// result matches what `.format()` would have produced. When the string still
/// has replacement fields (or the literal cannot be unescaped textually) the
/// empty `.format()` call is kept so that runtime behaviour, including any
/// `KeyError`, is preserved.
pub(crate) fn remove_unused_positional_arguments_from_format_call(
    unused_arguments: &[usize],
    call: &ast::ExprCall,
    summary: &FormatSummary,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    if unused_arguments.len() == call.arguments.len()
        && summary.autos.is_empty()
        && summary.indices.is_empty()
        && summary.keywords.is_empty()
        && let Expr::Attribute(attribute) = &*call.func
        && let Some(replacement) = literal_source_without_format_call(&attribute.value, locator)
    {
        return Ok(Edit::range_replacement(replacement, call.range()));
    }

    let source_code = locator.slice(call);
    transform_expression(source_code, stylist, |mut expression| {
        let call = match_call_mut(&mut expression)?;

        // Remove any unused arguments.
        let mut index = 0;
        call.args.retain(|_| {
            let is_unused = unused_arguments.contains(&index);
            index += 1;
            !is_unused
        });

        Ok(expression)
    })
    .map(|output| Edit::range_replacement(output, call.range()))
}

/// Returns the source text to substitute for `expr.format(...)` when the
/// `.format(...)` call is being dropped because it has no replacement fields
/// and no remaining arguments.
///
/// Returns `None` when the rewrite cannot be done safely, in which case the
/// caller should keep the `.format(...)` call (with no args) instead.
///
/// If the literal's decoded value has no braces, `.format()` is a no-op on it
/// and the original source can be reused verbatim. When the value contains
/// `{{` / `}}` escapes, those are reduced to single braces, but only when the
/// literal's source content equals its decoded value: an escape such as
/// `\x7b` could otherwise interact with a neighbouring literal brace and
/// silently change the resulting string.
fn literal_source_without_format_call(expr: &Expr, locator: &Locator) -> Option<String> {
    let Expr::StringLiteral(string_expr) = expr else {
        return None;
    };
    if string_expr.value.is_implicit_concatenated() {
        return None;
    }
    let literal = string_expr.value.iter().next()?;

    if !literal.value.contains(['{', '}']) {
        // `.format()` would not have changed anything in the resulting
        // string, so reuse the original source as-is.
        return Some(locator.slice(string_expr).to_string());
    }

    let content_range = literal.content_range();
    let source_content = locator.slice(content_range);

    // The textual `{{` -> `{` rewrite below is only safe when the source
    // content matches the decoded value: otherwise a Python escape sequence
    // such as `\x7b` could combine with a neighbouring literal brace and
    // change the resulting string.
    if source_content != &*literal.value {
        return None;
    }

    let unescaped = unescape_format_braces(source_content);
    let prefix_and_opener = locator.slice(ruff_text_size::TextRange::new(
        literal.start(),
        content_range.start(),
    ));
    let closer = locator.slice(ruff_text_size::TextRange::new(
        content_range.end(),
        literal.end(),
    ));

    Some(format!("{prefix_and_opener}{unescaped}{closer}"))
}

/// Replaces every `{{` with `{` and every `}}` with `}` in `text`.
fn unescape_format_braces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if (ch == '{' || ch == '}') && chars.peek() == Some(&ch) {
            chars.next();
        }
        out.push(ch);
    }
    out
}

/// Generate a [`Edit`] to remove the binding from an exception handler.
pub(crate) fn remove_exception_handler_assignment(
    bound_exception: &Binding,
    locator: &Locator,
) -> Result<Edit> {
    // Find the position just after the exception name. This is a late pass so we only have the
    // binding and can't look its parent in the AST up anymore.
    // ```
    // except ZeroDivisionError as err:
    //                             ^^^ This is the bound_exception range
    //                         ^^^^ lex this range
    //                         ^ preceding_end (we want to remove from here)
    // ```
    // There can't be any comments in that range.
    let mut tokenizer =
        BackwardsTokenizer::up_to(bound_exception.start(), locator.contents(), &[]).skip_trivia();

    // Eat the `as` token.
    let preceding = tokenizer
        .next()
        .context("expected the exception name to be preceded by `as`")?;
    debug_assert!(matches!(preceding.kind, SimpleTokenKind::As));

    // Lex to the end of the preceding token, which should be the exception value.
    let preceding = tokenizer
        .next()
        .context("expected the exception name to be preceded by a token")?;

    // Lex forwards, to the `:` token.
    let following = SimpleTokenizer::starts_at(bound_exception.end(), locator.contents())
        .skip_trivia()
        .next()
        .context("expected the exception name to be followed by a colon")?;
    debug_assert!(matches!(following.kind, SimpleTokenKind::Colon));

    Ok(Edit::deletion(preceding.end(), following.start()))
}
