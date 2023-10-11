use anyhow::{anyhow, Result};
use libcst_native::{Arg, Expression};
use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::matchers::{
    match_attribute, match_call_mut, match_expression, transform_expression_text,
};
use crate::fix::codemods::CodegenStylist;

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use implicit references for positional format fields")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove explicit positional indices".to_string())
    }
}

/// UP030
pub(crate) fn format_literals(
    checker: &mut Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
) {
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

    // If the positional indices aren't sequential (e.g., `"{1} {0}".format(1, 2)`), then we
    // need to reorder the function arguments; so we need to ensure that the function
    // arguments aren't splatted (e.g., `"{1} {0}".format(*foo)`), that there are a sufficient
    // number of them, etc.
    let arguments = if is_sequential(&summary.indices) {
        Arguments::Preserve
    } else {
        // Ex) `"{1} {0}".format(foo=1, bar=2)`
        if !call.arguments.keywords.is_empty() {
            return;
        }

        // Ex) `"{1} {0}".format(foo)`
        if call.arguments.args.len() < summary.indices.len() {
            return;
        }

        // Ex) `"{1} {0}".format(*foo)`
        if call
            .arguments
            .args
            .iter()
            .take(summary.indices.len())
            .any(Expr::is_starred_expr)
        {
            return;
        }

        Arguments::Reorder(&summary.indices)
    };

    let mut diagnostic = Diagnostic::new(FormatLiterals, call.range());
    diagnostic.try_set_fix(|| {
        generate_call(call, arguments, checker.locator(), checker.stylist())
            .map(|suggestion| Fix::unsafe_edit(Edit::range_replacement(suggestion, call.range())))
    });
    checker.diagnostics.push(diagnostic);
}

/// Returns true if the indices are sequential.
fn is_sequential(indices: &[usize]) -> bool {
    indices.iter().enumerate().all(|(idx, value)| idx == *value)
}

// An opening curly brace, followed by any integer, followed by any text,
// followed by a closing brace.
static FORMAT_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<int>\d+)(?P<fmt>.*?)}").unwrap());

/// Remove the explicit positional indices from a format string.
fn remove_specifiers<'a>(value: &mut Expression<'a>, arena: &'a typed_arena::Arena<String>) {
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
fn generate_arguments<'a>(arguments: &[Arg<'a>], order: &[usize]) -> Result<Vec<Arg<'a>>> {
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

#[derive(Debug, Copy, Clone)]
enum Arguments<'a> {
    /// Preserve the arguments to the `.format(...)` call.
    Preserve,
    /// Reorder the arguments to the `.format(...)` call, based on the given
    /// indices.
    Reorder(&'a [usize]),
}

/// Returns the corrected function call.
fn generate_call(
    call: &ast::ExprCall,
    arguments: Arguments,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let source_code = locator.slice(call);

    let output = transform_expression_text(source_code, |source_code| {
        let mut expression = match_expression(&source_code)?;

        // Fix the call arguments.
        let call = match_call_mut(&mut expression)?;
        if let Arguments::Reorder(order) = arguments {
            call.args = generate_arguments(&call.args, order)?;
        }

        // Fix the string itself.
        let item = match_attribute(&mut call.func)?;
        let arena = typed_arena::Arena::new();
        remove_specifiers(&mut item.value, &arena);

        Ok(expression.codegen_stylist(stylist))
    })?;

    // Ex) `'{' '0}'.format(1)`
    if output == source_code {
        return Err(anyhow!("Unable to identify format literals"));
    }

    Ok(output)
}
