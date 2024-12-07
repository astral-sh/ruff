use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::{expr_is_strictly_int, IsStrictlyInt};
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprName, ExprNumberLiteral, Number};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;

/// ## What it does
/// Checks for `round()` calls where the second argument is redundant.
///
/// `round(foo, None)` is exactly the same as `round(foo)`.
/// Similarly, `round(integer, 0)` is the same as `round(integer)`.
///
/// ## Why is this bad?
/// The unnecessary second argument to `ndigits` may make the code less readable.
///
/// ## Known problems
/// This rule is prone to false positives due to type inference limitations.
///
/// ## Example
///
/// ```python
/// round(1, 0)
/// round(foo, None)
/// ```
///
/// Use instead:
///
/// ```python
/// round(1)
/// round(foo)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryRoundNdigits;

impl AlwaysFixableViolation for UnnecessaryRoundNdigits {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary argument to `round`'s `ndigits`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove second argument".to_string()
    }
}

/// RUF057
pub(crate) fn unnecessary_round_ndigits(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !is_builtin_round(semantic, &call.func) {
        return;
    }

    let Some((number, Some(ndigits))) = round_number_and_ndigits(&call.arguments) else {
        return;
    };

    let (number_is_known_to_be_int, mut applicability) = match number {
        Expr::Name(name) => (
            variable_was_defined_as_int(checker.semantic(), name),
            Applicability::Unsafe,
        ),
        _ => match expr_is_strictly_int(checker.semantic(), number) {
            IsStrictlyInt::True => (true, Applicability::Safe),
            _ => (false, Applicability::Unsafe),
        },
    };

    match ndigits {
        // round(integer, 0)
        Expr::NumberLiteral(ExprNumberLiteral { value, .. })
            if is_literal_zero(value) && number_is_known_to_be_int => {}

        // round(whatever, None)
        Expr::NoneLiteral(_) => applicability = Applicability::Safe,

        _ => return,
    };

    let number_expr = checker.locator().slice(number);
    let new_content = format!("round({number_expr})");
    let edit = Edit::range_replacement(new_content, call.range);
    let fix = Fix::applicable_edit(edit, applicability);

    let diagnostic = Diagnostic::new(UnnecessaryRoundNdigits, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

pub(crate) fn is_builtin_round(semantic: &SemanticModel, func: &Expr) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    matches!(qualified_name.segments(), ["" | "builtins", "round"])
}

fn variable_was_defined_as_int(semantic: &SemanticModel, name: &ExprName) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_int(binding, semantic)
}

pub(crate) fn round_number_and_ndigits(arguments: &Arguments) -> Option<(&Expr, Option<&Expr>)> {
    if arguments.len() > 2 {
        return None;
    }

    let number = arguments.find_argument("number", 0)?;
    let ndigits = arguments.find_argument("ndigits", 1);

    Some((number, ndigits))
}

pub(crate) fn is_literal_zero(value: &Number) -> bool {
    let Number::Int(int) = value else {
        return false;
    };

    matches!(int.as_u8(), Some(0))
}
