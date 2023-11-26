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
/// respectively. This is more readable, precise, and efficient.
///
/// ## Example
/// ```python
/// import math
///
/// math.log(2, 2)
/// math.log(2, 10)
/// math.log(2, math.e)
/// ```
///
/// Use instead:
/// ```python
/// import math
///
/// math.log2(2)
/// math.log10(2)
/// math.log(2)
/// ```
///
/// ## References
/// - [Python documentation: `math.log`](https://docs.python.org/3/library/math.html#math.log)
/// - [Python documentation: `math.log2`](https://docs.python.org/3/library/math.html#math.log2)
/// - [Python documentation: `math.log10`](https://docs.python.org/3/library/math.html#math.log10)
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
        format!("Replace with `math.{log_function}({arg})`")
    }

    fn fix_title(&self) -> Option<String> {
        let RedundantLogBase { base, arg } = self;
        let log_function = base.to_log_function();
        Some(format!("Use `math.{log_function}({arg})`"))
    }
}

/// FURB163
pub(crate) fn redundant_log_base(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .as_ref()
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["math", "log"]))
    {
        return;
    }
    match &call.arguments.args.as_slice() {
        [arg, base] if is_number_literal(base, 2) => {
            let mut diagnostic = Diagnostic::new(
                RedundantLogBase {
                    base: Base::Two,
                    arg: checker.locator().slice(arg).into(),
                },
                call.range(),
            );
            diagnostic.try_set_fix(|| generate_fix(checker, call, Base::Two));
            checker.diagnostics.push(diagnostic);
        }
        [arg, base] if is_number_literal(base, 10) => {
            let mut diagnostic = Diagnostic::new(
                RedundantLogBase {
                    base: Base::Ten,
                    arg: checker.locator().slice(arg).into(),
                },
                call.range(),
            );
            diagnostic.try_set_fix(|| generate_fix(checker, call, Base::Ten));
            checker.diagnostics.push(diagnostic);
        }
        [arg, base]
            if checker
                .semantic()
                .resolve_call_path(base)
                .as_ref()
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["math", "e"])) =>
        {
            let mut diagnostic = Diagnostic::new(
                RedundantLogBase {
                    base: Base::E,
                    arg: checker.locator().slice(arg).into(),
                },
                call.range(),
            );
            diagnostic.try_set_fix(|| generate_fix(checker, call, Base::E));
            checker.diagnostics.push(diagnostic);
        }
        _ => {}
    }
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
        }
    }
    false
}

fn generate_fix(checker: &Checker, call: &ast::ExprCall, base: Base) -> Result<Fix> {
    let (edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("math", base.to_log_function()),
        call.start(),
        checker.semantic(),
    )?;
    let number = checker.locator().slice(&call.arguments.args[0]);
    Ok(Fix::safe_edits(
        Edit::range_replacement(format!("{binding}({number})"), call.range()),
        [edit],
    ))
}
