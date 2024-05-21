use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprCall, ExprNumberLiteral, Number};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for uses of `trio.sleep()` with >24 hour interval.
///
/// ## Why is this bad?
/// `trio.sleep()` with a >24 hour interval is usually intended to sleep indefintely.
/// This intent is be better conveyed using `trio.sleep_forever()`.
///
/// ## Example
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.sleep(86401)
/// ```
///
/// Use instead:
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.sleep_forever()
/// ```
#[violation]
pub struct SleepForeverCall;

impl Violation for SleepForeverCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`trio.sleep()` with >24 hour interval should usually be `trio.sleep_forever()`.")
    }
}

/// ASYNC116
pub(crate) fn sleep_forever_call(checker: &mut Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::TRIO) {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    // Is this the zeroth arg?
    let Some(arg) = call.arguments.find_argument("seconds", 0) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["trio", "sleep"]))
    {
        return;
    }

    let Expr::NumberLiteral(ExprNumberLiteral { value, .. }) = arg else {
        return;
    };

    // TODO: Replace with Duration::from_days(1).as_secs(); when available.
    let one_day_in_secs = 60 * 60 * 24;
    match value {
        Number::Int(int_value) => {
            let Some(int_value) = int_value.as_u64() else {
                return;
            };
            if int_value <= one_day_in_secs {
                return;
            }
        }
        Number::Float(float_value) => {
            if *float_value <= one_day_in_secs as f64 {
                return;
            }
        }
        // Number::Complex is a type error.
        _ => return,
    }

    let mut diagnostic = Diagnostic::new(SleepForeverCall, call.range());
    let replacement_function = "sleep_forever";
    diagnostic.try_set_fix(|| {
        let (import_edit, ..) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("trio", replacement_function),
            call.func.start(),
            checker.semantic(),
        )?;
        let reference_edit =
            Edit::range_replacement(replacement_function.to_string(), call.func.range());
        let arg_edit = Edit::range_replacement("()".to_string(), call.arguments.range());
        Ok(Fix::unsafe_edits(import_edit, [reference_edit, arg_edit]))
    });
    checker.diagnostics.push(diagnostic);
}
