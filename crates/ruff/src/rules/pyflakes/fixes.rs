use anyhow::{Context, Ok, Result};
use ruff_python_ast::{Expr, Ranged};
use ruff_text_size::TextRange;

use ruff_diagnostics::Edit;
use ruff_python_codegen::Stylist;
use ruff_python_semantic::Binding;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::Locator;

use crate::autofix::codemods::CodegenStylist;
use crate::cst::matchers::{match_call_mut, match_dict, match_expression};

/// Generate a [`Edit`] to remove unused keys from format dict.
pub(super) fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[usize],
    stmt: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_expression(module_text)?;
    let dict = match_dict(&mut tree)?;

    // Remove the elements at the given indexes.
    let mut index = 0;
    dict.elements.retain(|_| {
        let is_unused = unused_arguments.contains(&index);
        index += 1;
        !is_unused
    });

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        stmt.range(),
    ))
}

/// Generate a [`Edit`] to remove unused keyword arguments from a `format` call.
pub(super) fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[usize],
    location: TextRange,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

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

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        location,
    ))
}

/// Generate a [`Edit`] to remove unused positional arguments from a `format` call.
pub(crate) fn remove_unused_positional_arguments_from_format_call(
    unused_arguments: &[usize],
    location: TextRange,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    // Remove any unused arguments.
    let mut index = 0;
    call.args.retain(|_| {
        let is_unused = unused_arguments.contains(&index);
        index += 1;
        !is_unused
    });

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        location,
    ))
}

/// Generate a [`Edit`] to remove the binding from an exception handler.
pub(crate) fn remove_exception_handler_assignment(
    bound_exception: &Binding,
    locator: &Locator,
) -> Result<Edit> {
    // Lex backwards, to the token just before the `as`.
    let mut tokenizer = SimpleTokenizer::up_to_without_back_comment(
        bound_exception.range.start(),
        locator.contents(),
    )
    .skip_trivia();

    // Eat the `as` token.
    let preceding = tokenizer
        .next_back()
        .context("expected the exception name to be preceded by `as`")?;
    debug_assert!(matches!(preceding.kind, SimpleTokenKind::As));

    // Lex to the end of the preceding token, which should be the exception value.
    let preceding = tokenizer
        .next_back()
        .context("expected the exception name to be preceded by a token")?;

    // Lex forwards, to the `:` token.
    let following = SimpleTokenizer::starts_at(bound_exception.range.end(), locator.contents())
        .skip_trivia()
        .next()
        .context("expected the exception name to be followed by a colon")?;
    debug_assert!(matches!(following.kind, SimpleTokenKind::Colon));

    Ok(Edit::deletion(preceding.end(), following.start()))
}
