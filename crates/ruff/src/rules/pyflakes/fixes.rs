use anyhow::{Context, Ok, Result};

use ruff_diagnostics::Edit;
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_semantic::Binding;
use ruff_python_trivia::{is_python_whitespace, Cursor, SimpleTokenizer};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

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
    let mut cursor = Cursor::new(&locator.contents()[TextRange::up_to(bound_exception.start())]);

    assert!(
        is_python_whitespace(cursor.last()),
        "expected whitespace before the binding"
    );
    cursor.eat_back_while(is_python_whitespace);

    let token_as = cursor.eat_char_back('s') && cursor.eat_char_back('a');
    assert!(
        token_as,
        "expected the exception name to be preceded by `as`"
    );
    assert!(
        is_python_whitespace(cursor.last()),
        "expected whitespace before the `as`"
    );
    cursor.eat_back_while(is_python_whitespace);

    let preceding_end = bound_exception.start() - cursor.token_len();

    // Lex forwards, to the `:` token.
    let following = SimpleTokenizer::starts_at(bound_exception.end(), locator.contents())
        .skip_trivia()
        .next()
        .context("expected the exception name to be followed by a colon")?;

    Ok(Edit::deletion(preceding_end, following.start()))
}
