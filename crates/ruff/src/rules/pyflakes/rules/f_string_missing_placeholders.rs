use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::helpers::find_useless_f_strings;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ## What it does
    /// Checks for f-strings that do not contain any placeholder expressions.
    ///
    /// ## Why is this bad?
    /// F-strings are a convenient way to format strings, but they are not
    /// necessary if there are no placeholder expressions to format. In this
    /// case, a regular string should be used instead, as an f-string without
    /// placeholders can be confusing for readers, who may expect such a
    /// placeholder to be present.
    ///
    /// An f-string without any placeholders could also indicate that the
    /// author forgot to add a placeholder expression.
    ///
    /// ## Example
    /// ```python
    /// f"Hello, world!"
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// "Hello, world!"
    /// ```
    ///
    /// ## References
    /// * [PEP 498](https://www.python.org/dev/peps/pep-0498/)
    pub struct FStringMissingPlaceholders;
);
impl AlwaysAutofixableViolation for FStringMissingPlaceholders {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string without any placeholders")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }
}

fn unescape_f_string(content: &str) -> String {
    content.replace("{{", "{").replace("}}", "}")
}

fn fix_f_string_missing_placeholders(
    prefix_range: &Range,
    tok_range: &Range,
    checker: &mut Checker,
) -> Fix {
    let content = checker.locator.slice(&Range::new(
        prefix_range.end_location,
        tok_range.end_location,
    ));
    Fix::replacement(
        unescape_f_string(content),
        prefix_range.location,
        tok_range.end_location,
    )
}

/// F541
pub fn f_string_missing_placeholders(expr: &Expr, values: &[Expr], checker: &mut Checker) {
    if !values
        .iter()
        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
    {
        for (prefix_range, tok_range) in find_useless_f_strings(expr, checker.locator) {
            let mut diagnostic = Diagnostic::new(FStringMissingPlaceholders, tok_range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(fix_f_string_missing_placeholders(
                    &prefix_range,
                    &tok_range,
                    checker,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
