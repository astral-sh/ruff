use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprNumberLiteral, Number};
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
            if let Some(fix) = replace_with_round(checker, outer_range, inner_range, arguments) {
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

/// The type of the first argument to `round()`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rounded {
    InferredInt,
    InferredFloat,
    LiteralInt,
    LiteralFloat,
    Other,
}

/// The type of the second argument to `round()`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ndigits {
    NotGiven,
    LiteralInt,
    LiteralNone,
    Other,
}

fn replace_with_round(
    checker: &Checker,
    outer_range: TextRange,
    inner_range: TextRange,
    arguments: &Arguments,
) -> Option<Fix> {
    if arguments.len() > 2 {
        return None;
    }

    let number = arguments.find_argument("number", 0)?;
    let ndigits = arguments.find_argument("ndigits", 1);

    let number_kind = match number {
        Expr::Name(name) => {
            let semantic = checker.semantic();

            match semantic.only_binding(name).map(|id| semantic.binding(id)) {
                Some(binding) if typing::is_int(binding, semantic) => Rounded::InferredInt,
                Some(binding) if typing::is_float(binding, semantic) => Rounded::InferredFloat,
                _ => Rounded::Other,
            }
        }

        Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
            Number::Int(..) => Rounded::LiteralInt,
            Number::Float(..) => Rounded::LiteralFloat,
            Number::Complex { .. } => Rounded::Other,
        },

        _ => Rounded::Other,
    };

    let ndigits_kind = match ndigits {
        None => Ndigits::NotGiven,
        Some(Expr::NoneLiteral(_)) => Ndigits::LiteralNone,

        Some(Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(..),
            ..
        })) => Ndigits::LiteralInt,

        _ => Ndigits::Other,
    };

    let applicability = match (number_kind, ndigits_kind) {
        (Rounded::LiteralInt, Ndigits::LiteralInt)
        | (Rounded::LiteralInt | Rounded::LiteralFloat, Ndigits::NotGiven | Ndigits::LiteralNone) => {
            Applicability::Safe
        }

        (Rounded::InferredInt, Ndigits::LiteralInt)
        | (
            Rounded::InferredInt | Rounded::InferredFloat | Rounded::Other,
            Ndigits::NotGiven | Ndigits::LiteralNone,
        ) => Applicability::Unsafe,

        _ => return None,
    };

    let edit = replace_with_inner(checker, outer_range, inner_range);

    Some(Fix::applicable_edit(edit, applicability))
}

fn replace_with_inner(checker: &Checker, outer_range: TextRange, inner_range: TextRange) -> Edit {
    let inner_expr = checker.locator().slice(inner_range);

    Edit::range_replacement(inner_expr.to_string(), outer_range)
}
