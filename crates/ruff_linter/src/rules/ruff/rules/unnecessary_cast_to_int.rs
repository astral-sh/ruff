use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprName, ExprNumberLiteral, Number};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for `int` conversions of values that are already integers.
///
/// ## Why is this bad?
/// Such a conversion is unnecessary.
///
/// ## Known problems
/// This rule may produce false positives for `round`, `math.ceil`, `math.floor`,
/// and `math.trunc` calls when values override the `__round__`, `__ceil__`, `__floor__`,
/// or `__trunc__` operators such that they don't return an integer.
///
/// ## Example
///
/// ```python
/// int(len([]))
/// int(round(foo, None))
/// ```
///
/// Use instead:
///
/// ```python
/// len([])
/// round(foo)
/// ```
///
/// ## Fix safety
/// The fix for `round`, `math.ceil`, `math.floor`, and `math.truncate` is unsafe
/// because removing the `int` conversion can change the semantics for values
/// overriding the `__round__`, `__ceil__`, `__floor__`, or `__trunc__` dunder methods
/// such that they don't return an integer.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryCastToInt;

impl AlwaysFixableViolation for UnnecessaryCastToInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Value being casted is already an integer".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary conversion to `int`".to_string()
    }
}

/// RUF046
pub(crate) fn unnecessary_cast_to_int(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    let Some(Expr::Call(inner_call)) = single_argument_to_int_call(semantic, call) else {
        return;
    };

    let (func, arguments) = (&inner_call.func, &inner_call.arguments);
    let (outer_range, inner_range) = (call.range, inner_call.range);

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(func) else {
        return;
    };

    let fix = match qualified_name.segments() {
        // Always returns a strict instance of `int`
        ["" | "builtins", "len" | "id" | "hash" | "ord" | "int"]
        | ["math", "comb" | "factorial" | "gcd" | "lcm" | "isqrt" | "perm"] => {
            Fix::safe_edit(replace_with_inner(checker, outer_range, inner_range))
        }

        // Depends on `ndigits` and `number.__round__`
        ["" | "builtins", "round"] => {
            if let Some(fix) = replace_with_shortened_round_call(checker, outer_range, arguments) {
                fix
            } else {
                return;
            }
        }

        // Depends on `__ceil__`/`__floor__`/`__trunc__`
        ["math", "ceil" | "floor" | "trunc"] => {
            Fix::unsafe_edit(replace_with_inner(checker, outer_range, inner_range))
        }

        _ => return,
    };

    checker
        .diagnostics
        .push(Diagnostic::new(UnnecessaryCastToInt, call.range).with_fix(fix));
}

fn single_argument_to_int_call<'a>(
    semantic: &SemanticModel,
    call: &'a ExprCall,
) -> Option<&'a Expr> {
    let ExprCall {
        func, arguments, ..
    } = call;

    if !semantic.match_builtin_expr(func, "int") {
        return None;
    }

    if !arguments.keywords.is_empty() {
        return None;
    }

    let [argument] = &*arguments.args else {
        return None;
    };

    Some(argument)
}

/// Returns an [`Edit`] when the call is of any of the forms:
/// * `round(integer)`, `round(integer, 0)`, `round(integer, None)`
/// * `round(whatever)`, `round(whatever, None)`
fn replace_with_shortened_round_call(
    checker: &Checker,
    outer_range: TextRange,
    arguments: &Arguments,
) -> Option<Fix> {
    if arguments.len() > 2 {
        return None;
    }

    let number = arguments.find_argument("number", 0)?;
    let ndigits = arguments.find_argument("ndigits", 1);

    let number_is_int = match number {
        Expr::Name(name) => is_int(checker.semantic(), name),
        Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => matches!(value, Number::Int(..)),
        _ => false,
    };

    match ndigits {
        Some(Expr::NumberLiteral(ExprNumberLiteral { value, .. }))
            if is_literal_zero(value) && number_is_int => {}
        Some(Expr::NoneLiteral(_)) | None => {}
        _ => return None,
    };

    let number_expr = checker.locator().slice(number);
    let new_content = format!("round({number_expr})");

    let applicability = if number_is_int {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };

    Some(Fix::applicable_edit(
        Edit::range_replacement(new_content, outer_range),
        applicability,
    ))
}

fn is_int(semantic: &SemanticModel, name: &ExprName) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_int(binding, semantic)
}

fn is_literal_zero(value: &Number) -> bool {
    let Number::Int(int) = value else {
        return false;
    };

    matches!(int.as_u8(), Some(0))
}

fn replace_with_inner(checker: &Checker, outer_range: TextRange, inner_range: TextRange) -> Edit {
    let inner_expr = checker.locator().slice(inner_range);

    Edit::range_replacement(inner_expr.to_string(), outer_range)
}
