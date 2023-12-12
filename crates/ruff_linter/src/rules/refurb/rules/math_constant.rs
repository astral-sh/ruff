use anyhow::Result;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
/// ## References
/// - [Python documentation: `math` constants](https://docs.python.org/3/library/math.html#constants)
#[violation]
pub struct MathConstant {
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
pub(crate) fn math_constant(checker: &mut Checker, literal: &ast::ExprNumberLiteral) {
    let Number::Float(value) = literal.value else {
        return;
    };

    if let Some(constant) = Constant::from_value(value) {
        let mut diagnostic = Diagnostic::new(
            MathConstant {
                literal: checker.locator().slice(literal).into(),
                constant: constant.name(),
            },
            literal.range(),
        );
        diagnostic.try_set_fix(|| convert_to_constant(literal, constant.name(), checker));
        checker.diagnostics.push(diagnostic);
    }
}

fn convert_to_constant(
    literal: &ast::ExprNumberLiteral,
    constant: &'static str,
    checker: &Checker,
) -> Result<Fix> {
    let (edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("math", constant),
        literal.start(),
        checker.semantic(),
    )?;
    Ok(Fix::safe_edits(
        Edit::range_replacement(binding, literal.range()),
        [edit],
    ))
}

#[derive(Debug, Clone, Copy)]
enum Constant {
    Pi,
    E,
    Tau,
}

impl Constant {
    #[allow(clippy::approx_constant)]
    fn from_value(value: f64) -> Option<Self> {
        if (3.14..3.15).contains(&value) {
            Some(Self::Pi)
        } else if (2.71..2.72).contains(&value) {
            Some(Self::E)
        } else if (6.28..6.29).contains(&value) {
            Some(Self::Tau)
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
}
