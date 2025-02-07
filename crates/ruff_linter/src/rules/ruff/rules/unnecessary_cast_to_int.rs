use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{Arguments, Expr, ExprCall};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{lines_after_ignoring_trivia, CommentRanges};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::unnecessary_round::{
    rounded_and_ndigits, InferredType, NdigitsValue, RoundedValue,
};
use crate::Locator;

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
        "Value being cast to `int` is already an integer".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary `int` call".to_string()
    }
}

/// RUF046
pub(crate) fn unnecessary_cast_to_int(checker: &Checker, call: &ExprCall) {
    let Some(argument) = single_argument_to_int_call(call, checker.semantic()) else {
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

    let fix = unwrap_int_expression(
        call,
        argument,
        applicability,
        checker.semantic(),
        checker.locator(),
        checker.comment_ranges(),
        checker.source(),
    );
    let diagnostic = Diagnostic::new(UnnecessaryCastToInt, call.range());

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

/// Creates a fix that replaces `int(expression)` with `expression`.
fn unwrap_int_expression(
    call: &ExprCall,
    argument: &Expr,
    applicability: Applicability,
    semantic: &SemanticModel,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Fix {
    let content = if let Some(range) = parenthesized_range(
        argument.into(),
        (&call.arguments).into(),
        comment_ranges,
        source,
    ) {
        locator.slice(range).to_string()
    } else {
        let parenthesize = semantic.current_expression_parent().is_some()
            || argument.is_named_expr()
            || locator.count_lines(argument.range()) > 0;
        if parenthesize && !has_own_parentheses(argument, comment_ranges, source) {
            format!("({})", locator.slice(argument.range()))
        } else {
            locator.slice(argument.range()).to_string()
        }
    };

    // Since we're deleting the complement of the argument range within
    // the call range, we have to check both ends for comments.
    //
    // For example:
    // ```python
    // int( # comment
    //     round(
    //         42.1
    //     ) # comment
    // )
    // ```
    let applicability = {
        let call_to_arg_start = TextRange::new(call.start(), argument.start());
        let arg_to_call_end = TextRange::new(argument.end(), call.end());
        if comment_ranges.intersects(call_to_arg_start)
            || comment_ranges.intersects(arg_to_call_end)
        {
            Applicability::Unsafe
        } else {
            applicability
        }
    };

    let edit = Edit::range_replacement(content, call.range());
    Fix::applicable_edit(edit, applicability)
}

/// Returns `Some` if `call` in `int(call(...))` is a method that returns an `int`
/// and `None` otherwise.
fn call_applicability(checker: &Checker, inner_call: &ExprCall) -> Option<Applicability> {
    let (func, arguments) = (&inner_call.func, &inner_call.arguments);

    let qualified_name = checker.semantic().resolve_qualified_name(func)?;

    match qualified_name.segments() {
        // Always returns a strict instance of `int`
        ["" | "builtins", "len" | "id" | "hash" | "ord" | "int"]
        | ["math", "comb" | "factorial" | "gcd" | "lcm" | "isqrt" | "perm"] => {
            Some(Applicability::Safe)
        }

        // Depends on `ndigits` and `number.__round__`
        ["" | "builtins", "round"] => round_applicability(arguments, checker.semantic()),

        // Depends on `__ceil__`/`__floor__`/`__trunc__`
        ["math", "ceil" | "floor" | "trunc"] => Some(Applicability::Unsafe),

        _ => None,
    }
}

fn single_argument_to_int_call<'a>(
    call: &'a ExprCall,
    semantic: &SemanticModel,
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
fn round_applicability(arguments: &Arguments, semantic: &SemanticModel) -> Option<Applicability> {
    let (_rounded, rounded_value, ndigits_value) = rounded_and_ndigits(arguments, semantic)?;

    match (rounded_value, ndigits_value) {
        // ```python
        // int(round(2, -1))
        // int(round(2, 0))
        // int(round(2))
        // int(round(2, None))
        // ```
        (
            RoundedValue::Int(InferredType::Equivalent),
            NdigitsValue::LiteralInt { .. }
            | NdigitsValue::Int(InferredType::Equivalent)
            | NdigitsValue::NotGivenOrNone,
        ) => Some(Applicability::Safe),

        // ```python
        // int(round(2.0))
        // int(round(2.0, None))
        // ```
        (RoundedValue::Float(InferredType::Equivalent), NdigitsValue::NotGivenOrNone) => {
            Some(Applicability::Safe)
        }

        // ```python
        // a: int = 2 # or True
        // int(round(a, -2))
        // int(round(a, 1))
        // int(round(a))
        // int(round(a, None))
        // ```
        (
            RoundedValue::Int(InferredType::AssignableTo),
            NdigitsValue::LiteralInt { .. }
            | NdigitsValue::Int(InferredType::Equivalent)
            | NdigitsValue::NotGivenOrNone,
        ) => Some(Applicability::Unsafe),

        // ```python
        // int(round(2.0))
        // int(round(2.0, None))
        // int(round(x))
        // int(round(x, None))
        // ```
        (
            RoundedValue::Float(InferredType::AssignableTo) | RoundedValue::Other,
            NdigitsValue::NotGivenOrNone,
        ) => Some(Applicability::Unsafe),

        _ => None,
    }
}

/// Returns `true` if the given [`Expr`] has its own parentheses (e.g., `()`, `[]`, `{}`).
fn has_own_parentheses(expr: &Expr, comment_ranges: &CommentRanges, source: &str) -> bool {
    match expr {
        Expr::ListComp(_)
        | Expr::SetComp(_)
        | Expr::DictComp(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_) => true,
        Expr::Call(call_expr) => {
            // A call where the function and parenthesized
            // argument(s) appear on separate lines
            // requires outer parentheses. That is:
            // ```
            // (f
            // (10))
            // ```
            // is different than
            // ```
            // f
            // (10)
            // ```
            let func_end = parenthesized_range(
                call_expr.func.as_ref().into(),
                call_expr.into(),
                comment_ranges,
                source,
            )
            .unwrap_or(call_expr.func.range())
            .end();
            lines_after_ignoring_trivia(func_end, source) == 0
        }
        Expr::Subscript(subscript_expr) => {
            // Same as above
            let subscript_end = parenthesized_range(
                subscript_expr.value.as_ref().into(),
                subscript_expr.into(),
                comment_ranges,
                source,
            )
            .unwrap_or(subscript_expr.value.range())
            .end();
            lines_after_ignoring_trivia(subscript_end, source) == 0
        }
        Expr::Generator(generator) => generator.parenthesized,
        Expr::Tuple(tuple) => tuple.parenthesized,
        _ => false,
    }
}
