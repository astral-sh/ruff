use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall, ExprNumberLiteral, Number};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::flake8_async::helpers::AsyncModule;

/// ## What it does
/// Checks for uses of `trio.sleep()` or `anyio.sleep()` with a delay greater than 24 hours.
///
/// ## Why is this bad?
/// Calling `sleep()` with a delay greater than 24 hours is usually intended
/// to sleep indefinitely. Instead of using a large delay,
/// `trio.sleep_forever()` or `anyio.sleep_forever()` better conveys the intent.
///
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
#[derive(ViolationMetadata)]
pub(crate) struct LongSleepNotForever {
    module: AsyncModule,
}

impl Violation for LongSleepNotForever {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { module } = self;
        format!(
            "`{module}.sleep()` with >24 hour interval should usually be `{module}.sleep_forever()`"
        )
    }

    fn fix_title(&self) -> Option<String> {
        let Self { module } = self;
        Some(format!("Replace with `{module}.sleep_forever()`"))
    }
}

/// ASYNC116
pub(crate) fn long_sleep_not_forever(checker: &Checker, call: &ExprCall) {
    if !(checker.semantic().seen_module(Modules::TRIO)
        || checker.semantic().seen_module(Modules::ANYIO))
    {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument_value("seconds", 0) else {
        return;
    };

    let Expr::NumberLiteral(ExprNumberLiteral { value, .. }) = arg else {
        return;
    };

    // TODO(ekohilas): Replace with Duration::from_days(1).as_secs(); when available.
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
        Number::Float(float_value) =>
        {
            #[allow(clippy::cast_precision_loss)]
            if *float_value <= one_day_in_secs as f64 {
                return;
            }
        }
        Number::Complex { .. } => return,
    }

    let Some(qualified_name) = checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
    else {
        return;
    };

    let Some(module) = AsyncModule::try_from(&qualified_name) else {
        return;
    };

    let is_relevant_module = matches!(module, AsyncModule::AnyIo | AsyncModule::Trio);

    let is_sleep = is_relevant_module && matches!(qualified_name.segments(), [_, "sleep"]);

    if !is_sleep {
        return;
    }

    let mut diagnostic = Diagnostic::new(LongSleepNotForever { module }, call.range());
    let replacement_function = "sleep_forever";
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from(&module.to_string(), replacement_function),
            call.func.start(),
            checker.semantic(),
        )?;
        let reference_edit = Edit::range_replacement(binding, call.func.range());
        let arg_edit = Edit::range_replacement("()".to_string(), call.arguments.range());
        Ok(Fix::unsafe_edits(import_edit, [reference_edit, arg_edit]))
    });
    checker.report_diagnostic(diagnostic);
}
