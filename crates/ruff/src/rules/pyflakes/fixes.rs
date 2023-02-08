use anyhow::{bail, Result};
use libcst_native::{Call, Codegen, CodegenState, Dict, DictElement, Expression};
use ruff_python::string::strip_quotes_and_prefixes;
use rustpython_parser::ast::{Excepthandler, Expr};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::cst::matchers::{match_expr, match_module};
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// Generate a [`Fix`] to remove unused keys from format dict.
pub fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[&str],
    stmt: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;

    let new_dict = {
        let Expression::Dict(dict) = &body.value else {
            bail!("Expected Expression::Dict")
        };

        Dict {
            lbrace: dict.lbrace.clone(),
            lpar: dict.lpar.clone(),
            rbrace: dict.rbrace.clone(),
            rpar: dict.rpar.clone(),
            elements: dict
                .elements
                .iter()
                .filter_map(|e| match e {
                    DictElement::Simple {
                        key: Expression::SimpleString(name),
                        ..
                    } if unused_arguments.contains(&strip_quotes_and_prefixes(name.value)) => None,
                    e => Some(e.clone()),
                })
                .collect(),
        }
    };

    body.value = Expression::Dict(Box::new(new_dict));

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// Generate a [`Fix`] to remove unused keyword arguments from format call.
pub fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[&str],
    location: Range,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&location);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;

    let new_call = {
        let Expression::Call(call) = &body.value else {
            bail!("Expected Expression::Call")
        };

        Call {
            func: call.func.clone(),
            lpar: call.lpar.clone(),
            rpar: call.rpar.clone(),
            whitespace_before_args: call.whitespace_before_args.clone(),
            whitespace_after_func: call.whitespace_after_func.clone(),
            args: call
                .args
                .iter()
                .filter_map(|e| match &e.keyword {
                    Some(kw) if unused_arguments.contains(&kw.value) => None,
                    _ => Some(e.clone()),
                })
                .collect(),
        }
    };

    body.value = Expression::Call(Box::new(new_call));

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        location.location,
        location.end_location,
    ))
}

/// Generate a [`Fix`] to remove the binding from an exception handler.
pub fn remove_exception_handler_assignment(
    excepthandler: &Excepthandler,
    locator: &Locator,
) -> Result<Fix> {
    let contents = locator.slice_source_code_range(&Range::from_located(excepthandler));
    let mut fix_start = None;
    let mut fix_end = None;

    // End of the token just before the `as` to the semicolon.
    let mut prev = None;
    for (start, tok, end) in
        lexer::make_tokenizer_located(contents, excepthandler.location).flatten()
    {
        if matches!(tok, Tok::As) {
            fix_start = prev;
        }
        if matches!(tok, Tok::Colon) {
            fix_end = Some(start);
            break;
        }
        prev = Some(end);
    }

    if let (Some(start), Some(end)) = (fix_start, fix_end) {
        Ok(Fix::deletion(start, end))
    } else {
        bail!("Could not find span of exception handler")
    }
}
