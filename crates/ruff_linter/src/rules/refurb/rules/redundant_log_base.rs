use anyhow::Result;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for `math.log` calls with a redundant base.
///
/// ## Why is this bad?
/// The default base of `math.log` is `e`, so specifying it explicitly is
/// redundant.
///
/// Instead of passing 2 or 10 as the base, use `math.log2` or `math.log10`
/// respectively, as these dedicated variants are typically more accurate
/// than `math.log`.
///
/// ## Example
/// ```python
/// import math
///
/// math.log(4, math.e)
/// math.log(4, 2)
/// math.log(4, 10)
/// ```
///
/// Use instead:
/// ```python
/// import math
///
/// math.log(4)
/// math.log2(4)
/// math.log10(4)
/// ```
///
/// ## References
/// - [Python documentation: `math.log`](https://docs.python.org/3/library/math.html#math.log)
/// - [Python documentation: `math.log2`](https://docs.python.org/3/library/math.html#math.log2)
/// - [Python documentation: `math.log10`](https://docs.python.org/3/library/math.html#math.log10)
/// - [Python documentation: `math.e`](https://docs.python.org/3/library/math.html#math.e)
#[violation]
pub struct RedundantLogBase {
    base: Base,
    arg: String,
}

impl Violation for RedundantLogBase {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantLogBase { base, arg } = self;
        let log_function = base.to_log_function();
        format!("Prefer `math.{log_function}({arg})` over `math.log` with a redundant base")
    }

    fn fix_title(&self) -> Option<String> {
        let RedundantLogBase { base, arg } = self;
        let log_function = base.to_log_function();
        Some(format!("Replace with `math.{log_function}({arg})`"))
    }
}

/// FURB163
pub(crate) fn redundant_log_base(checker: &mut Checker, call: &ast::ExprCall) {
    if !call.arguments.keywords.is_empty() {
        return;
    }

    let [arg, base] = &*call.arguments.args else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["math", "log"]))
    {
        return;
    }

    let base = if is_number_literal(base, 2) {
        Base::Two
    } else if is_number_literal(base, 10) {
        Base::Ten
    } else if checker
        .semantic()
        .resolve_qualified_name(base)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["math", "e"]))
    {
        Base::E
    } else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        RedundantLogBase {
            base,
            arg: checker.locator().slice(arg).into(),
        },
        call.range(),
    );
    diagnostic.try_set_fix(|| generate_fix(checker, call, base, arg));
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    E,
    Two,
    Ten,
}

impl Base {
    fn to_log_function(self) -> &'static str {
        match self {
            Base::E => "log",
            Base::Two => "log2",
            Base::Ten => "log10",
        }
    }
}

fn is_number_literal(expr: &Expr, value: i8) -> bool {
    if let Expr::NumberLiteral(number_literal) = expr {
        if let Number::Int(number) = &number_literal.value {
            return number.as_i8().is_some_and(|number| number == value);
        } else if let Number::Float(number) = number_literal.value {
            #[allow(clippy::float_cmp)]
            return number == f64::from(value);
        }
    }
    false
}

fn generate_fix(checker: &Checker, call: &ast::ExprCall, base: Base, arg: &Expr) -> Result<Fix> {
    let (edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("math", base.to_log_function()),
        call.start(),
        checker.semantic(),
    )?;
    let number = checker.locator().slice(arg);
    Ok(Fix::safe_edits(
        Edit::range_replacement(format!("{binding}({number})"), call.range()),
        [edit],
    ))
}
