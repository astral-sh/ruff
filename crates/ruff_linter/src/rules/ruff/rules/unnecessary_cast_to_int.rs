use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall};
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

    let func = &inner_call.func;
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

        // Depends on `__ceil__`/`__floor__`/`__trunc__`/`__round__`
        ["math", "ceil" | "floor" | "trunc"] | ["" | "builtins", "round"] => {
            Fix::unsafe_edit(replace_with_inner(checker, outer_range, inner_range))
        }

        _ => return,
    };

    let diagnostic = Diagnostic::new(UnnecessaryCastToInt, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
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

fn replace_with_inner(checker: &Checker, outer_range: TextRange, inner_range: TextRange) -> Edit {
    let inner_expr = checker.locator().slice(inner_range);

    Edit::range_replacement(inner_expr.to_string(), outer_range)
}
