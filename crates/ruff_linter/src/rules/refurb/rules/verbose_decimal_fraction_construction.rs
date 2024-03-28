use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for unnecessary `from_float`, `from_decimal` method called
/// when constructing `Decimal` and `Fraction`.
///
/// ## Why is this bad?
/// From Python 3.2, it is possible to directly construct `Fraction`
/// from `float` and `Decimal`, or `Decimal` from `float` by passing argument to
/// the constructor.
/// There is no need to use `from_float` or `from_decimal` instances, thus prefer directly using
/// the constructor as it is more readable and idiomatic.
///
/// Further, if `Decimal` of special values i.e. `inf`, `-inf`, `Infinity`, `-Infinity` and `nan`
/// is constructed, there is no need to construct via float instance.
/// For example, expression like `Decimal("inf")` is possible.
///
/// ## Examples
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
/// Fraction(Decimal("4.2"))
/// ```
/// ## Fix safety
/// This rule's fix is marked as unsafe, as the target of the method call
/// could be a user-defined `Decimal` or `Fraction` class, rather
/// than method from the built-in module.
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
/// - [Python documentation: `fractions`](https://docs.python.org/3/library/fractions.html)
#[violation]
pub struct VerboseDecimalFractionConstruction {
    method_name: String,
    constructor: String,
}

impl Violation for VerboseDecimalFractionConstruction {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Verbose expression in constructing `{}`", self.constructor)
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Use constructor `{}` directly", self.constructor))
    }
}

/// FURB164
pub(crate) fn verbose_decimal_fraction_construction(checker: &mut Checker, call: &ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    if !matches!(
        qualified_name.segments(),
        ["decimal", "Decimal", "from_float"]
            | ["fractions", "Fraction", "from_float" | "from_decimal"]
    ) {
        return;
    }

    let [module, constructor, method_name] = qualified_name.segments() else {
        return;
    };

    let Some(value) = (if method_name == &"from_float" {
        call.arguments.find_argument("f", 0)
    } else {
        call.arguments.find_argument("dec", 0)
    }) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        VerboseDecimalFractionConstruction {
            method_name: (*method_name).to_string(),
            constructor: (*constructor).to_string(),
        },
        call.range(),
    );

    let new_constructor = checker.importer().get_or_import_symbol(
        &ImportRequest::import_from(module, constructor),
        call.start(),
        checker.semantic(),
    );
    let Ok((_, new_constructor)) = new_constructor else {
        checker.diagnostics.push(diagnostic);
        return;
    };
    let edit = Edit::range_replacement(new_constructor, call.func.range());

    // Short-circuit case for special values, such as
    // `Decimal.from_float(float("inf"))` to `Decimal("inf")`.
    'short_circuit: {
        if !(constructor == &"Decimal" && method_name == &"from_float") {
            break 'short_circuit;
        };
        let Expr::Call(
            inner_call @ ast::ExprCall {
                func, arguments, ..
            },
        ) = value
        else {
            break 'short_circuit;
        };
        let Some(func_name) = func.as_name_expr() else {
            break 'short_circuit;
        };
        if !(func_name.id == "float" && checker.semantic().is_builtin("float")) {
            break 'short_circuit;
        };
        // Must have exactly one argument, which is a string literal.
        if arguments.keywords.len() != 0 {
            break 'short_circuit;
        };
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
        };

        diagnostic.set_fix(Fix::unsafe_edits(
            edit,
            [Edit::range_replacement(
                format!("\"{}\"", float.value.to_str()),
                inner_call.range(),
            )],
        ));
        checker.diagnostics.push(diagnostic);

        return;
    }

    diagnostic.set_fix(Fix::unsafe_edit(edit));
    checker.diagnostics.push(diagnostic);
}
