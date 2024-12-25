use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::unnecessary_round::{
    rounded_and_ndigits, InferredType, NdigitsKind, RoundedKind,
};
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprCall};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
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

    let applicability = if matches!(
        ResolvedPythonType::from(argument),
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
    ) {
        Some(Applicability::Safe)
    } else if let Expr::Call(inner_call) = argument {
        call_applicability(checker, inner_call)
    } else {
        None
    };

    let Some(applicability) = applicability else {
        return;
    };

    let fix = unwrap_int_expression(checker, call, argument, applicability);
    let diagnostic = Diagnostic::new(UnnecessaryCastToInt, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

/// Creates a fix that replaces `int(expression)` with `expression`.
fn unwrap_int_expression(
    checker: &Checker,
    call: &ExprCall,
    argument: &Expr,
    applicability: Applicability,
) -> Fix {
    let (locator, semantic) = (checker.locator(), checker.semantic());

    let argument_expr = locator.slice(argument.range());

    let has_parent_expr = semantic.current_expression_parent().is_some();
    let new_content = if has_parent_expr || argument.is_named_expr() {
        format!("({argument_expr})")
    } else {
        argument_expr.to_string()
    };

    let edit = Edit::range_replacement(new_content, call.range);
    Fix::applicable_edit(edit, applicability)
}

/// Returns `Some` if `call` in `int(call(...))` is a method that returns an `int`
/// and `None` otherwise.
fn call_applicability(checker: &mut Checker, inner_call: &ExprCall) -> Option<Applicability> {
    let (func, arguments) = (&inner_call.func, &inner_call.arguments);

    let qualified_name = checker.semantic().resolve_qualified_name(func)?;

    match qualified_name.segments() {
        // Always returns a strict instance of `int`
        ["" | "builtins", "len" | "id" | "hash" | "ord" | "int"]
        | ["math", "comb" | "factorial" | "gcd" | "lcm" | "isqrt" | "perm"] => {
            Some(Applicability::Safe)
        }

        // Depends on `ndigits` and `number.__round__`
        ["" | "builtins", "round"] => round_applicability(checker, arguments),

        // Depends on `__ceil__`/`__floor__`/`__trunc__`
        ["math", "ceil" | "floor" | "trunc"] => Some(Applicability::Unsafe),

        _ => None,
    }
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

/// Determines the [`Applicability`] for a `round(..)` call.
///
/// The Applicability depends on the `ndigits` and the number argument.
fn round_applicability(checker: &Checker, arguments: &Arguments) -> Option<Applicability> {
    let (_rounded, rounded_kind, ndigits_kind) = rounded_and_ndigits(checker, arguments)?;

    match (rounded_kind, ndigits_kind) {
        (
            RoundedKind::Int(InferredType::Equivalent),
            NdigitsKind::Int(InferredType::Equivalent),
        )
        | (
            RoundedKind::Int(InferredType::Equivalent)
            | RoundedKind::Float(InferredType::Equivalent),
            NdigitsKind::NotGiven | NdigitsKind::LiteralNone,
        ) => Some(Applicability::Safe),

        (
            RoundedKind::Int(InferredType::AssignableTo),
            NdigitsKind::Int(InferredType::Equivalent),
        )
        | (
            RoundedKind::Int(InferredType::AssignableTo)
            | RoundedKind::Float(InferredType::AssignableTo)
            | RoundedKind::Other,
            NdigitsKind::NotGiven | NdigitsKind::LiteralNone,
        ) => Some(Applicability::Unsafe),

        _ => None,
    }
}
