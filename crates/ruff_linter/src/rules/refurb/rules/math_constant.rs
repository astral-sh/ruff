use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for literals that are similar to constants in `math` module.
///
/// ## Why is this bad?
/// Hard-coding mathematical constants like π increases code duplication,
/// reduces readability, and may lead to a lack of precision.
///
/// ## Example
/// ```python
/// A = 3.141592 * r**2
/// ```
///
/// Use instead:
/// ```python
/// A = math.pi * r**2
/// ```
///
/// ## Known problems
/// A literal is matched on its digits alone, so a value that merely happens to
/// begin with the same digits as a mathematical constant is flagged even when it
/// means something else entirely. A measurement that rounds to `3.14`, or a price
/// of `2.718`, is indistinguishable here from an approximation of π or e.
///
/// ## Fix safety
/// This rule's fix is marked as safe only when the literal already denotes exactly
/// the same float as the constant, so that replacing it cannot change a result.
/// Shorter approximations are rewritten under an unsafe fix, because `math.pi` is
/// not equal to `3.14`: a literal that was deliberately chosen — a rounded
/// measurement, a threshold, a test fixture — computes a different value once it
/// becomes a constant.
///
/// ## References
/// - [Python documentation: `math` constants](https://docs.python.org/3/library/math.html#constants)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.6", category = Category::Suspicious)]
pub(crate) struct MathConstant {
    literal: String,
    constant: &'static str,
}

impl Violation for MathConstant {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let MathConstant { literal, constant } = self;
        format!("Replace `{literal}` with `math.{constant}`")
    }

    fn fix_title(&self) -> Option<String> {
        let MathConstant { constant, .. } = self;
        Some(format!("Use `math.{constant}`"))
    }
}

/// FURB152
pub(crate) fn math_constant(checker: &Checker, literal: &ast::ExprNumberLiteral) {
    let Number::Float(value) = literal.value else {
        return;
    };

    if let Some(constant) = Constant::from_value(value) {
        let mut diagnostic = checker.report_diagnostic(
            MathConstant {
                literal: checker.locator().slice(literal).into(),
                constant: constant.name(),
            },
            literal.range(),
        );
        diagnostic.try_set_fix(|| convert_to_constant(literal, value, constant, checker));
    }
}

fn convert_to_constant(
    literal: &ast::ExprNumberLiteral,
    value: f64,
    constant: Constant,
    checker: &Checker,
) -> Result<Fix> {
    let (edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("math", constant.name()),
        literal.start(),
        checker.semantic(),
    )?;
    let replacement = Edit::range_replacement(binding, literal.range());

    // The rule matches any literal that rounds to the constant, so `3.14` is reported
    // just as `3.141592653589793` is. Substituting the constant only leaves the
    // program computing the same values in the latter case; in the former it silently
    // replaces the author's number with a different one.
    #[expect(
        clippy::float_cmp,
        reason = "an exact comparison is the point: only a literal that is already the \
                  same float can be replaced without changing what the program computes"
    )]
    let is_exact = value == constant.value();

    if is_exact {
        Ok(Fix::safe_edits(replacement, [edit]))
    } else {
        Ok(Fix::unsafe_edits(replacement, [edit]))
    }
}

fn matches_constant(constant: f64, value: f64) -> bool {
    for point in 2..=15 {
        let rounded = (constant * 10_f64.powi(point)).round() / 10_f64.powi(point);
        if (rounded - value).abs() < f64::EPSILON {
            return true;
        }
        let rounded = (constant * 10_f64.powi(point)).floor() / 10_f64.powi(point);
        if (rounded - value).abs() < f64::EPSILON {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy)]
enum Constant {
    Pi,
    E,
    Tau,
}

impl Constant {
    #[expect(clippy::approx_constant)]
    fn from_value(value: f64) -> Option<Self> {
        if (3.14..3.15).contains(&value) {
            matches_constant(std::f64::consts::PI, value).then_some(Self::Pi)
        } else if (2.71..2.72).contains(&value) {
            matches_constant(std::f64::consts::E, value).then_some(Self::E)
        } else if (6.28..6.29).contains(&value) {
            matches_constant(std::f64::consts::TAU, value).then_some(Self::Tau)
        } else {
            None
        }
    }

    fn name(self) -> &'static str {
        match self {
            Constant::Pi => "pi",
            Constant::E => "e",
            Constant::Tau => "tau",
        }
    }

    /// The value `math.<name>` evaluates to, for comparing against a matched literal.
    fn value(self) -> f64 {
        match self {
            Constant::Pi => std::f64::consts::PI,
            Constant::E => std::f64::consts::E,
            Constant::Tau => std::f64::consts::TAU,
        }
    }
}
