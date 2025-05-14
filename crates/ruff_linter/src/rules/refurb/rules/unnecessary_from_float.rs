use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary `from_float` and `from_decimal` usages to construct
/// `Decimal` and `Fraction` instances.
///
/// ## Why is this bad?
/// Since Python 3.2, the `Fraction` and `Decimal` classes can be constructed
/// by passing float or decimal instances to the constructor directly. As such,
/// the use of `from_float` and `from_decimal` methods is unnecessary, and
/// should be avoided in favor of the more concise constructor syntax.
///
/// ## Example
/// ```python
/// Decimal.from_float(4.2)
/// Decimal.from_float(float("inf"))
/// Fraction.from_float(4.2)
/// Fraction.from_decimal(Decimal("4.2"))
/// ```
///
/// Use instead:
/// ```python
/// Decimal(4.2)
/// Decimal("inf")
/// Fraction(4.2)
/// Fraction(Decimal(4.2))
/// ```
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
/// - [Python documentation: `fractions`](https://docs.python.org/3/library/fractions.html)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryFromFloat {
    method_name: MethodName,
    constructor: Constructor,
}

impl Violation for UnnecessaryFromFloat {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryFromFloat {
            method_name,
            constructor,
        } = self;
        format!("Verbose method `{method_name}` in `{constructor}` construction",)
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryFromFloat { constructor, .. } = self;
        Some(format!("Replace with `{constructor}` constructor"))
    }
}

/// FURB164
pub(crate) fn unnecessary_from_float(checker: &Checker, call: &ExprCall) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = &*call.func else {
        return;
    };

    // The method name must be either `from_float` or `from_decimal`.
    let method_name = match attr.as_str() {
        "from_float" => MethodName::FromFloat,
        "from_decimal" => MethodName::FromDecimal,
        _ => return,
    };

    let semantic = checker.semantic();

    // The value must be either `decimal.Decimal` or `fractions.Fraction`.
    let Some(qualified_name) = semantic.resolve_qualified_name(value) else {
        return;
    };

    let constructor = match qualified_name.segments() {
        ["decimal", "Decimal"] => Constructor::Decimal,
        ["fractions", "Fraction"] => Constructor::Fraction,
        _ => return,
    };

    // `Decimal.from_decimal` doesn't exist.
    if matches!(
        (method_name, constructor),
        (MethodName::FromDecimal, Constructor::Decimal)
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryFromFloat {
            method_name,
            constructor,
        },
        call.range(),
    );

    let edit = Edit::range_replacement(
        checker.locator().slice(&**value).to_string(),
        call.func.range(),
    );

    // Short-circuit case for special values, such as: `Decimal.from_float(float("inf"))` to `Decimal("inf")`.
    'short_circuit: {
        if !matches!(constructor, Constructor::Decimal) {
            break 'short_circuit;
        }
        if !(method_name == MethodName::FromFloat) {
            break 'short_circuit;
        }

        let Some(value) = (match method_name {
            MethodName::FromFloat => call.arguments.find_argument_value("f", 0),
            MethodName::FromDecimal => call.arguments.find_argument_value("dec", 0),
        }) else {
            return;
        };

        let Expr::Call(
            call @ ast::ExprCall {
                func, arguments, ..
            },
        ) = value
        else {
            break 'short_circuit;
        };

        // Must have exactly one argument, which is a string literal.
        if !arguments.keywords.is_empty() {
            break 'short_circuit;
        }
        let [float] = arguments.args.as_ref() else {
            break 'short_circuit;
        };
        let Some(float) = float.as_string_literal_expr() else {
            break 'short_circuit;
        };
        if !matches!(
            float.value.to_str().to_lowercase().as_str(),
            "inf" | "-inf" | "infinity" | "-infinity" | "nan"
        ) {
            break 'short_circuit;
        }

        // Must be a call to the `float` builtin.
        if !semantic.match_builtin_expr(func, "float") {
            break 'short_circuit;
        }

        let replacement = checker.locator().slice(float).to_string();
        diagnostic.set_fix(Fix::safe_edits(
            edit,
            [Edit::range_replacement(replacement, call.range())],
        ));
        checker.report_diagnostic(diagnostic);

        return;
    }

    diagnostic.set_fix(Fix::safe_edit(edit));
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MethodName {
    FromFloat,
    FromDecimal,
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MethodName::FromFloat => fmt.write_str("from_float"),
            MethodName::FromDecimal => fmt.write_str("from_decimal"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Constructor {
    Decimal,
    Fraction,
}

impl std::fmt::Display for Constructor {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Constructor::Decimal => fmt.write_str("Decimal"),
            Constructor::Fraction => fmt.write_str("Fraction"),
        }
    }
}
