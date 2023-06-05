use anyhow::{bail, Ok, Result};
use libcst_native::{DictElement, Expression};
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Excepthandler, Expr, Ranged};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::str::raw_contents;

use crate::autofix::codemods::CodegenStylist;
use crate::cst::matchers::{match_call_mut, match_dict, match_expression};

/// Generate a [`Edit`] to remove unused keys from format dict.
pub(crate) fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[&str],
    stmt: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_expression(module_text)?;
    let dict = match_dict(&mut tree)?;

    dict.elements.retain(|e| {
        !matches!(e, DictElement::Simple {
            key: Expression::SimpleString(name),
            ..
        } if raw_contents(name.value).map_or(false, |name| unused_arguments.contains(&name)))
    });

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        stmt.range(),
    ))
}

/// Generate a [`Edit`] to remove unused keyword arguments from a `format` call.
pub(crate) fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[&str],
    location: TextRange,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(location);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    call.args
        .retain(|e| !matches!(&e.keyword, Some(kw) if unused_arguments.contains(&kw.value)));

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
    excepthandler: &Excepthandler,
    locator: &Locator,
) -> Result<Edit> {
    let contents = locator.slice(excepthandler.range());
    let mut fix_start = None;
    let mut fix_end = None;

    // End of the token just before the `as` to the semicolon.
    let mut prev = None;
    for (tok, range) in
        lexer::lex_starts_at(contents, Mode::Module, excepthandler.start()).flatten()
    {
        if matches!(tok, Tok::As) {
            fix_start = prev;
        }
        if matches!(tok, Tok::Colon) {
            fix_end = Some(range.start());
            break;
        }
        prev = Some(range.end());
    }

    if let (Some(start), Some(end)) = (fix_start, fix_end) {
        Ok(Edit::deletion(start, end))
    } else {
        bail!("Could not find span of exception handler")
    }
}
