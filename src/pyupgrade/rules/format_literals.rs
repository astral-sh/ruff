use anyhow::{anyhow, bail, Result};
use libcst_native::{Arg, Codegen, CodegenState, Expression};
use num_traits::ToPrimitive;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Constant, Expr, ExprKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call, match_expression};
use crate::registry::Diagnostic;
use crate::violations;

// This checks for a an opening curly brace, followed by any integer, followed
// by any text, followed by closing brace.
static FORMAT_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<int>\d+)(?P<fmt>.*?)}").unwrap());

// When we check for a nested format specifier, the closing brace will be
// removed, so we just need to check for the opening brace and an integer.
static FIRST_HALF: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{(\d+)").unwrap());

/// Return the corrected argument vector.
fn generate_arguments<'a>(
    old_args: &[Arg<'a>],
    correct_order: &'a [usize],
) -> Result<Vec<Arg<'a>>> {
    let mut new_args: Vec<Arg> = Vec::with_capacity(old_args.len());
    for (idx, given) in correct_order.iter().enumerate() {
        // We need to keep the formatting in the same order but move the values.
        let values = old_args
            .get(*given)
            .ok_or_else(|| anyhow!("Failed to extract argument at: {given}"))?;
        let formatting = old_args
            .get(idx)
            .ok_or_else(|| anyhow!("Failed to extract argument at: {idx}"))?;
        let new_arg = Arg {
            value: values.value.clone(),
            comma: formatting.comma.clone(),
            equal: None,
            keyword: None,
            star: values.star,
            whitespace_after_star: formatting.whitespace_after_star.clone(),
            whitespace_after_arg: formatting.whitespace_after_arg.clone(),
        };
        new_args.push(new_arg);
    }
    Ok(new_args)
}

/// Returns the corrected function call.
fn generate_call(module_text: &str, correct_order: &[usize]) -> Result<String> {
    let mut expression = match_expression(module_text)?;
    let mut call = match_call(&mut expression)?;

    // Fix the call arguments.
    call.args = generate_arguments(&call.args, correct_order)?;

    // Fix the string itself.
    let Expression::Attribute(item) = &*call.func else {
        panic!("Expected: Expression::Attribute")
    };

    let mut state = CodegenState::default();
    item.codegen(&mut state);
    let cleaned = remove_specifiers(&state.to_string());

    call.func = Box::new(match_expression(&cleaned)?);

    let mut state = CodegenState::default();
    expression.codegen(&mut state);
    if module_text == state.to_string() {
        // Ex) `'{' '0}'.format(1)`
        bail!("Failed to generate call expression for: {module_text}")
    }
    Ok(state.to_string())
}

/// Extract the order of the format literals (e.g., "{0} {1}" would return [0,
/// 1]).
fn specifier_order(value: &str) -> Vec<usize> {
    let mut specifier_ints: Vec<usize> = vec![];
    let mut prev_l_brace = false;
    for (_, tok, _) in lexer::make_tokenizer(value).flatten() {
        if prev_l_brace {
            if let Tok::Int { ref value } = tok {
                if prev_l_brace {
                    if let Some(int_val) = value.to_usize() {
                        specifier_ints.push(int_val);
                    }
                }
            }
        }
        prev_l_brace = matches!(tok, Tok::Lbrace);
    }
    specifier_ints
}

/// Returns a string without the format specifiers. Ex. "Hello {0} {1}" ->
/// "Hello {} {}"
fn remove_specifiers(raw_specifiers: &str) -> String {
    FORMAT_SPECIFIER
        .replace_all(raw_specifiers, "{$fmt}")
        .to_string()
}

/// Returns `true` if there is at least one format literal in the string.
fn has_valid_specifiers(raw_specifiers: &str) -> bool {
    // If there's at least one match, return true...
    let mut has_valid_specifier = false;
    for cap in FORMAT_SPECIFIER.captures_iter(raw_specifiers) {
        has_valid_specifier = true;
        // ...unless there's a nested format specifier.
        if FIRST_HALF.is_match(&cap[2]) {
            return false;
        }
    }
    has_valid_specifier
}

/// Return `true` if the specifiers contain a complete range.
fn is_ordered(specifiers: &[usize]) -> bool {
    let mut specifiers = specifiers.to_vec();
    specifiers.sort_unstable();
    specifiers
        .iter()
        .enumerate()
        .all(|(index, specifier)| index == *specifier)
}

/// UP030
pub fn format_literals(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        // We only care about `format` calls.
        if attr != "format" {
            return;
        }
        if let ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } = &value.node
        {
            if !has_valid_specifiers(string) {
                return;
            }

            let specifiers = specifier_order(string);

            if !is_ordered(&specifiers) {
                return;
            }

            let mut diagnostic =
                Diagnostic::new(violations::FormatLiterals, Range::from_located(expr));
            if checker.patch(diagnostic.kind.code()) {
                let call_text = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(expr));
                // Currently, the only issue we know of is in LibCST:
                // https://github.com/Instagram/LibCST/issues/846
                if let Ok(contents) = generate_call(&call_text, &specifiers) {
                    diagnostic.amend(Fix::replacement(
                        contents,
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                };
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
