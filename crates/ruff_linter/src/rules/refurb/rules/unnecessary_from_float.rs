use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::linter::float::as_non_finite_float_string_literal;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
/// However, there are important behavioral differences between the `from_*` methods
/// and the constructors:
/// - The `from_*` methods validate their argument types and raise `TypeError` for invalid types
/// - The constructors accept broader argument types without validation
/// - The `from_*` methods have different parameter names than the constructors
///
/// ## Example
/// ```python
/// from decimal import Decimal
/// from fractions import Fraction
///
/// Decimal.from_float(4.2)
/// Decimal.from_float(float("inf"))
/// Fraction.from_float(4.2)
/// Fraction.from_decimal(Decimal("4.2"))
/// ```
///
/// Use instead:
/// ```python
/// from decimal import Decimal
/// from fractions import Fraction
///
/// Decimal(4.2)
/// Decimal("inf")
/// Fraction(4.2)
/// Fraction(Decimal("4.2"))
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe by default because:
/// - The `from_*` methods provide type validation that the constructors don't
/// - Removing type validation can change program behavior
/// - The parameter names are different between methods and constructors
///
/// The fix is marked as safe only when:
/// - The argument type is known to be valid for the target constructor
/// - No keyword arguments are used, or they match the constructor's parameters
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

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryFromFloat {
            method_name,
            constructor,
        },
        call.range(),
    );

    // Validate that the method call has correct arguments and get the argument value
    let Some(arg_value) = has_valid_method_arguments(call, method_name, constructor) else {
        // Don't suggest a fix for invalid calls
        return;
    };

    let constructor_name = checker.locator().slice(&**value).to_string();

    // Special case for non-finite float literals: Decimal.from_float(float("inf")) -> Decimal("inf")
    if let Some(replacement) = handle_non_finite_float_special_case(
        call,
        method_name,
        constructor,
        arg_value,
        &constructor_name,
        checker,
    ) {
        diagnostic.set_fix(Fix::safe_edit(replacement));
        return;
    }

    // Check if we should suppress the fix due to type validation concerns
    let is_type_safe = is_valid_argument_type(arg_value, method_name, constructor, checker);
    let has_keywords = !call.arguments.keywords.is_empty();

    // Determine fix safety
    let applicability = if is_type_safe && !has_keywords {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };

    // Build the replacement
    let arg_text = checker.locator().slice(arg_value);
    let replacement_text = format!("{constructor_name}({arg_text})");

    let edit = Edit::range_replacement(replacement_text, call.range());

    diagnostic.set_fix(Fix::applicable_edit(edit, applicability));
}

/// Check if the argument would be valid for the target constructor
fn is_valid_argument_type(
    arg_expr: &Expr,
    method_name: MethodName,
    constructor: Constructor,
    checker: &Checker,
) -> bool {
    let semantic = checker.semantic();
    let resolved_type = ResolvedPythonType::from(arg_expr);

    let (is_int, is_float) = if let ResolvedPythonType::Unknown = resolved_type {
        arg_expr
            .as_name_expr()
            .and_then(|name| semantic.only_binding(name).map(|id| semantic.binding(id)))
            .map(|binding| {
                (
                    typing::is_int(binding, semantic),
                    typing::is_float(binding, semantic),
                )
            })
            .unwrap_or((false, false))
    } else {
        (false, false)
    };

    match (method_name, constructor) {
        // Decimal.from_float: Only int or bool are safe (float is unsafe due to FloatOperation trap)
        (MethodName::FromFloat, Constructor::Decimal) => match resolved_type {
            ResolvedPythonType::Atom(PythonType::Number(
                NumberLike::Integer | NumberLike::Bool,
            )) => true,
            ResolvedPythonType::Unknown => is_int,
            _ => false,
        },
        // Fraction.from_float accepts int, bool, float
        (MethodName::FromFloat, Constructor::Fraction) => match resolved_type {
            ResolvedPythonType::Atom(PythonType::Number(
                NumberLike::Integer | NumberLike::Bool | NumberLike::Float,
            )) => true,
            ResolvedPythonType::Unknown => is_int || is_float,
            _ => false,
        },
        // Fraction.from_decimal accepts int, bool, Decimal
        (MethodName::FromDecimal, Constructor::Fraction) => match resolved_type {
            ResolvedPythonType::Atom(PythonType::Number(
                NumberLike::Integer | NumberLike::Bool,
            )) => true,
            ResolvedPythonType::Unknown => is_int,
            _ => {
                // Check if it's a Decimal instance
                arg_expr
                    .as_call_expr()
                    .and_then(|call| semantic.resolve_qualified_name(&call.func))
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["decimal", "Decimal"])
                    })
            }
        },
        _ => false,
    }
}

/// Check if the call has valid arguments for the from_* method
fn has_valid_method_arguments(
    call: &ExprCall,
    method_name: MethodName,
    constructor: Constructor,
) -> Option<&Expr> {
    if call.arguments.len() != 1 {
        return None;
    }

    match method_name {
        MethodName::FromFloat => {
            // Decimal.from_float is positional-only; Fraction.from_float allows keyword 'f'.
            if constructor == Constructor::Decimal {
                // Only allow positional argument for Decimal.from_float
                call.arguments.find_positional(0)
            } else {
                // Fraction.from_float allows either positional or 'f' keyword
                call.arguments.find_argument_value("f", 0)
            }
        }
        MethodName::FromDecimal => {
            // from_decimal(dec) - should have exactly one positional argument or 'dec' keyword
            call.arguments.find_argument_value("dec", 0)
        }
    }
}

/// Handle the special case for non-finite float literals
fn handle_non_finite_float_special_case(
    call: &ExprCall,
    method_name: MethodName,
    constructor: Constructor,
    arg_value: &Expr,
    constructor_name: &str,
    checker: &Checker,
) -> Option<Edit> {
    // Only applies to Decimal.from_float
    if !matches!(
        (method_name, constructor),
        (MethodName::FromFloat, Constructor::Decimal)
    ) {
        return None;
    }

    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = arg_value
    else {
        return None;
    };

    // Must be a call to the `float` builtin.
    if !checker.semantic().match_builtin_expr(func, "float") {
        return None;
    }

    // Must have exactly one argument, which is a string literal.
    if !arguments.keywords.is_empty() {
        return None;
    }
    let [float_arg] = arguments.args.as_ref() else {
        return None;
    };
    let normalized = as_non_finite_float_string_literal(float_arg)?;
    let replacement_text = format!(r#"{constructor_name}("{normalized}")"#);
    Some(Edit::range_replacement(replacement_text, call.range()))
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
