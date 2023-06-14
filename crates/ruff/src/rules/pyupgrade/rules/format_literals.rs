use anyhow::{anyhow, Result};
use libcst_native::{Arg, Expression};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::autofix::codemods::CodegenStylist;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_attribute, match_call_mut, match_expression};
use crate::registry::AsRule;
use crate::rules::pyflakes::format::FormatSummary;

/// ## What it does
/// Checks for unnecessary positional indices in format strings.
///
/// ## Why is this bad?
/// In Python 3.1 and later, format strings can use implicit positional
/// references. For example, `"{0}, {1}".format("Hello", "World")` can be
/// rewritten as `"{}, {}".format("Hello", "World")`.
///
/// If the positional indices appear exactly in-order, they can be omitted
/// in favor of automatic indices to improve readability.
///
/// ## Example
/// ```python
/// "{0}, {1}".format("Hello", "World")  # "Hello, World"
/// ```
///
/// Use instead:
/// ```python
/// "{}, {}".format("Hello", "World")  # "Hello, World"
/// ```
///
/// ## References
/// - [Python documentation: Format String Syntax](https://docs.python.org/3/library/string.html#format-string-syntax)
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[violation]
pub struct FormatLiterals;

impl Violation for FormatLiterals {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use implicit references for positional format fields")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Remove explicit positional indices".to_string())
    }
}

// An opening curly brace, followed by any integer, followed by any text,
// followed by a closing brace.
static FORMAT_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<int>\d+)(?P<fmt>.*?)}").unwrap());

/// Remove the explicit positional indices from a format string.
fn remove_specifiers<'a>(value: &mut Expression<'a>, arena: &'a mut typed_arena::Arena<String>) {
    match value {
        Expression::SimpleString(expr) => {
            expr.value = arena.alloc(
                FORMAT_SPECIFIER
                    .replace_all(expr.value, "{$fmt}")
                    .to_string(),
            );
        }
        Expression::ConcatenatedString(expr) => {
            let mut stack = vec![&mut expr.left, &mut expr.right];
            while let Some(string) = stack.pop() {
                match string.as_mut() {
                    libcst_native::String::Simple(string) => {
                        string.value = arena.alloc(
                            FORMAT_SPECIFIER
                                .replace_all(string.value, "{$fmt}")
                                .to_string(),
                        );
                    }
                    libcst_native::String::Concatenated(string) => {
                        stack.push(&mut string.left);
                        stack.push(&mut string.right);
                    }
                    libcst_native::String::Formatted(_) => {}
                }
            }
        }
        _ => {}
    }
}

/// Return the corrected argument vector.
fn generate_arguments<'a>(arguments: &[Arg<'a>], order: &'a [usize]) -> Result<Vec<Arg<'a>>> {
    let mut new_arguments: Vec<Arg> = Vec::with_capacity(arguments.len());
    for (idx, given) in order.iter().enumerate() {
        // We need to keep the formatting in the same order but move the values.
        let values = arguments
            .get(*given)
            .ok_or_else(|| anyhow!("Failed to extract argument at: {given}"))?;
        let formatting = arguments
            .get(idx)
            .ok_or_else(|| anyhow!("Failed to extract argument at: {idx}"))?;
        let argument = Arg {
            value: values.value.clone(),
            comma: formatting.comma.clone(),
            equal: None,
            keyword: None,
            star: values.star,
            whitespace_after_star: formatting.whitespace_after_star.clone(),
            whitespace_after_arg: formatting.whitespace_after_arg.clone(),
        };
        new_arguments.push(argument);
    }
    Ok(new_arguments)
}

/// Returns true if the indices are sequential.
fn is_sequential(indices: &[usize]) -> bool {
    indices.iter().enumerate().all(|(idx, value)| idx == *value)
}

/// Returns the corrected function call.
fn generate_call(
    expr: &Expr,
    correct_order: &[usize],
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let content = locator.slice(expr.range());
    let parenthesized_content = format!("({content})");
    let mut expression = match_expression(&parenthesized_content)?;

    // Fix the call arguments.
    let call = match_call_mut(&mut expression)?;
    if !is_sequential(correct_order) {
        call.args = generate_arguments(&call.args, correct_order)?;
    }

    // Fix the string itself.
    let item = match_attribute(&mut call.func)?;
    let mut arena = typed_arena::Arena::new();
    remove_specifiers(&mut item.value, &mut arena);

    // Remove the parentheses (first and last characters).
    let mut output = expression.codegen_stylist(stylist);
    output.remove(0);
    output.pop();

    // Ex) `'{' '0}'.format(1)`
    if output == content {
        return Err(anyhow!("Unable to identify format literals"));
    }

    Ok(output)
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
    if summary.indices.is_empty() {
        return;
    }
    if (0..summary.indices.len()).any(|index| !summary.indices.contains(&index)) {
        return;
    }

    let mut diagnostic = Diagnostic::new(FormatLiterals, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            Ok(Fix::suggested(Edit::range_replacement(
                generate_call(expr, &summary.indices, checker.locator, checker.stylist)?,
                expr.range(),
            )))
        });
    }
    checker.diagnostics.push(diagnostic);
}
