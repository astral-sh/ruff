use anyhow::{Context, Ok, Result};

use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_codegen::Stylist;
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::Edit;
use crate::Locator;
use crate::cst::matchers::{match_call_mut, match_dict, transform_expression};

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
pub(crate) fn remove_unused_positional_arguments_from_format_call(
    unused_arguments: &[usize],
    call: &ast::ExprCall,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    // If we're removing _all_ arguments, we can remove the entire call.
    //
    // For example, `"Hello".format(", world!")` -> `"Hello"`, as opposed to `"Hello".format()`.
    //
    // However, if the `format` call has other effects, like escaping `{{` to `{` or would raise a
    // `KeyError` for a missing field name, we preserve the empty call to avoid changing behavior.
    if unused_arguments.len() == call.arguments.len()
        && let Expr::Attribute(attribute) = &*call.func
        && let Expr::StringLiteral(string_expr) = &*attribute.value
        && !string_expr.value.to_str().contains(['{', '}'])
    {
        return Ok(Edit::range_replacement(
            locator.slice(string_expr).to_string(),
            call.range(),
        ));
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

/// Generate a [`Edit`] to remove the binding from an exception handler.
pub(crate) fn remove_exception_handler_assignment(
    bound_exception: &Binding,
    tokens: &Tokens,
) -> Result<Edit> {
    // Find the end of the exception type. This is a late pass, so we only have the binding
    // and can't look up its parent in the AST anymore.
    // ```
    // except ZeroDivisionError as err:
    //                             ^^^ This is the bound_exception range
    //                         ^^^^ tokens in this range
    //                         ^ preceding_end (we want to remove from here)
    // ```
    // There can't be any comments in that range.
    let (before, after) = tokens.split_at(bound_exception.start());
    // Whitespace and explicit line continuations do not produce parser tokens.
    let (as_token, before) = before
        .split_last()
        .context("expected the exception name to be preceded by `as`")?;
    debug_assert_eq!(as_token.kind(), TokenKind::As);

    // Find the end of the preceding token, which should be the exception value.
    let preceding = before
        .last()
        .context("expected the exception name to be preceded by a token")?;

    debug_assert!(
        after
            .first()
            .is_some_and(|token| token.range() == bound_exception.range())
    );
    // Skip the bound name and find the `:` token.
    let following = after
        .get(1)
        .context("expected the exception name to be followed by a colon")?;
    debug_assert_eq!(following.kind(), TokenKind::Colon);

    Ok(Edit::deletion(preceding.end(), following.start()))
}
