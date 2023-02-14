use anyhow::{anyhow, bail, Result};
use libcst_native::{Arg, Codegen, CodegenState, Expression};
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call, match_expression};
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pyflakes::format::FormatSummary;
use crate::source_code::{Locator, Stylist};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct FormatLiterals;
);
impl AlwaysAutofixableViolation for FormatLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use implicit references for positional format fields")
    }

    fn autofix_title(&self) -> String {
        "Remove explicit positional indexes".to_string()
    }
}

// An opening curly brace, followed by any integer, followed by any text,
// followed by a closing brace.
static FORMAT_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<int>\d+)(?P<fmt>.*?)}").unwrap());

/// Returns a string without the format specifiers.
/// Ex. "Hello {0} {1}" -> "Hello {} {}"
fn remove_specifiers(raw_specifiers: &str) -> String {
    FORMAT_SPECIFIER
        .replace_all(raw_specifiers, "{$fmt}")
        .to_string()
}

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

/// Returns true if the indices are sequential.
fn is_sequential(indices: &[usize]) -> bool {
    for (expected, actual) in indices.iter().enumerate() {
        if expected != *actual {
            return false;
        }
    }
    true
}

/// Returns the corrected function call.
fn generate_call(
    expr: &Expr,
    correct_order: &[usize],
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let module_text = locator.slice_source_code_range(&Range::from_located(expr));
    let mut expression = match_expression(module_text)?;
    let mut call = match_call(&mut expression)?;

    // Fix the call arguments.
    if !is_sequential(correct_order) {
        call.args = generate_arguments(&call.args, correct_order)?;
    }

    // Fix the string itself.
    let Expression::Attribute(item) = &*call.func else {
        panic!("Expected: Expression::Attribute")
    };

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    item.codegen(&mut state);
    let cleaned = remove_specifiers(&state.to_string());

    call.func = Box::new(match_expression(&cleaned)?);

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    expression.codegen(&mut state);
    if module_text == state.to_string() {
        // Ex) `'{' '0}'.format(1)`
        bail!("Failed to generate call expression for: {module_text}")
    }
    Ok(state.to_string())
}

/// UP030
pub(crate) fn format_literals(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    // The format we expect is, e.g.: `"{0} {1}".format(...)`
    if summary.has_nested_parts {
        return;
    }
    if !summary.keywords.is_empty() {
        return;
    }
    if !summary.autos.is_empty() {
        return;
    }
    if !(0..summary.indexes.len()).all(|index| summary.indexes.contains(&index)) {
        return;
    }

    let mut diagnostic = Diagnostic::new(FormatLiterals, Range::from_located(expr));
    if checker.patch(diagnostic.kind.rule()) {
        // Currently, the only issue we know of is in LibCST:
        // https://github.com/Instagram/LibCST/issues/846
        if let Ok(contents) =
            generate_call(expr, &summary.indexes, checker.locator, checker.stylist)
        {
            diagnostic.amend(Fix::replacement(
                contents,
                expr.location,
                expr.end_location.unwrap(),
            ));
        };
    }
    checker.diagnostics.push(diagnostic);
}
