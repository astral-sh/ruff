use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for usages of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
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
#[violation]
pub struct ExplicitFStringTypeConversion;

impl AlwaysAutofixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use conversion in f-string")
    }

    fn autofix_title(&self) -> String {
        "Replace f-string function call with conversion".to_string()
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(
    checker: &mut Checker,
    formatted_value: &Expr,
    conversion: ast::Int,
) {
    // Skip if there's already a conversion flag.
    if conversion != ast::ConversionFlag::None as u32 {
        return;
    }

    let ExprKind::Call(ast::ExprCall {
        func,
        args,
        keywords,
    }) = &formatted_value.node else {
        return;
    };

    // Can't be a conversion otherwise.
    if args.len() != 1 || !keywords.is_empty() {
        return;
    }

    let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else {
        return;
    };

    let conversion = match id.as_str() {
        "ascii" => 'a',
        "str" => 's',
        "repr" => 'r',
        _ => return,
    };

    if !checker.ctx.is_builtin(id) {
        return;
    }

    let formatted_value_range = formatted_value.range();
    let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, formatted_value_range);

    if checker.patch(diagnostic.kind.rule()) {
        let arg_range = args[0].range();
        let remove_call = Edit::deletion(formatted_value_range.start(), arg_range.start());
        let add_conversion = Edit::replacement(
            format!("!{conversion}"),
            arg_range.end(),
            formatted_value_range.end(),
        );
        diagnostic.set_fix(Fix::automatic_edits(remove_call, [add_conversion]));
    }

    checker.diagnostics.push(diagnostic);
}
