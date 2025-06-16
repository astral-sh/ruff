use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
///
/// Note that, in many cases, calling `str()` within an f-string is
/// unnecessary and can be removed entirely, as the value will be converted
/// to a string automatically, the notable exception being for classes that
/// implement a custom `__format__` method.
///
/// ## Example
/// ```python
/// a = "some string"
/// f"{repr(a)}"
/// ```
///
/// Use instead:
/// ```python
/// a = "some string"
/// f"{a!r}"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ExplicitFStringTypeConversion;

impl AlwaysFixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use explicit conversion flag".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with conversion flag".to_string()
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(checker: &Checker, f_string: &ast::FString) {
    for element in &f_string.elements {
        let Some(ast::InterpolatedElement {
            expression,
            conversion,
            ..
        }) = element.as_interpolation()
        else {
            continue;
        };

        // Skip if there's already a conversion flag.
        if !conversion.is_none() {
            continue;
        }

        let Expr::Call(call) = expression.as_ref() else {
            continue;
        };

        let builtin_symbol = checker.semantic().resolve_builtin_symbol(&call.func);

        let arg = match builtin_symbol {
            // Handles the cases: `f"{str(object=arg)}"` and `f"{str(arg)}"`
            Some("str") if call.arguments.len() == 1 => {
                let Some(arg) = call.arguments.find_argument_value("object", 0) else {
                    continue;
                };
                arg
            }
            _ => {
                // Can't be a conversion otherwise.
                if !call.arguments.keywords.is_empty() {
                    continue;
                }

                // Can't be a conversion otherwise.
                let [arg] = call.arguments.args.as_ref() else {
                    continue;
                };
                arg
            }
        };

        // Supress lint for starred expressions.
        if matches!(arg, Expr::Starred(_)) {
            return;
        }

        if !builtin_symbol.is_some_and(|builtin| matches!(builtin, "str" | "repr" | "ascii")) {
            continue;
        }

        let mut diagnostic =
            checker.report_diagnostic(ExplicitFStringTypeConversion, expression.range());
        diagnostic.try_set_fix(|| convert_call_to_conversion_flag(checker, element, call, arg));
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    checker: &Checker,
    element: &ast::InterpolatedStringElement,
    call: &ast::ExprCall,
    arg: &Expr,
) -> Result<Fix> {
    if element
        .as_interpolation()
        .is_some_and(|interpolation| interpolation.debug_text.is_some())
    {
        anyhow::bail!("Don't support fixing f-string with debug text!");
    }

    let name = UnqualifiedName::from_expr(&call.func).unwrap();
    let conversion = match name.segments() {
        ["str"] | ["builtins", "str"] => "s",
        ["repr"] | ["builtins", "repr"] => "r",
        ["ascii"] | ["builtins", "ascii"] => "a",
        _ => anyhow::bail!("Unexpected function call: `{:?}`", &call.func),
    };
    let arg_str = checker.locator().slice(arg);
    let contains_curly_brace = {
        let mut visitor = ContainsCurlyBraceVisitor { result: false };
        visitor.visit_expr(arg);
        visitor.result
    };

    let output = if contains_curly_brace {
        format!(" {arg_str}!{conversion}")
    } else if matches!(arg, Expr::Lambda(_) | Expr::Named(_)) {
        format!("({arg_str})!{conversion}")
    } else {
        format!("{arg_str}!{conversion}")
    };

    let replace_range = if let Some(range) = parenthesized_range(
        call.into(),
        element.into(),
        checker.comment_ranges(),
        checker.source(),
    ) {
        range
    } else {
        call.range()
    };

    Ok(Fix::safe_edit(Edit::range_replacement(
        output,
        replace_range,
    )))
}

struct ContainsCurlyBraceVisitor {
    result: bool,
}

impl<'a> Visitor<'a> for ContainsCurlyBraceVisitor {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Dict(_) | Expr::Set(_) | Expr::DictComp(_) | Expr::SetComp(_) => {
                self.result = true;
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
