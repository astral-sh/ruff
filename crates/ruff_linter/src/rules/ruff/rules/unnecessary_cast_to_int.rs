use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::analyze::typing::IsStrictlyInt;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

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

    let Some(argument) = single_argument_to_int_call(semantic, call) else {
        return;
    };

    let applicability = match typing::is_strictly_int_expr(argument, semantic) {
        IsStrictlyInt::True => Applicability::Safe,
        IsStrictlyInt::Likely => Applicability::Unsafe,
        _ => return,
    };

    let edit = replace_with_inner(checker, call, argument);
    let fix = Fix::applicable_edit(edit, applicability);

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

fn replace_with_inner(checker: &mut Checker, call: &ExprCall, argument: &Expr) -> Edit {
    let has_parent_expr = checker.semantic().current_expression_parent().is_some();
    let argument_expr = checker.locator().slice(argument.range());

    let new_content = if has_parent_expr || should_be_parenthesized_when_standalone(argument) {
        format!("({argument_expr})")
    } else {
        argument_expr.to_string()
    };

    Edit::range_replacement(new_content, call.range)
}

/// Whether `expr` should be parenthesized when used on its own.
///
/// ```python
/// a := 0            # (a := 0)
/// a = b := 0        # a = (b := 0)
/// a for a in b      # (a for a in b)
/// a = b for b in c  # a = (b for b in c)
/// ```
#[inline]
fn should_be_parenthesized_when_standalone(expr: &Expr) -> bool {
    matches!(expr, Expr::Named(_) | Expr::Generator(_))
}
