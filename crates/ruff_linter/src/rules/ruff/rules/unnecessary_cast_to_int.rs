use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, CmpOp, Expr, ExprBinOp, ExprCall, ExprCompare, ExprIf, ExprNamed, ExprNumberLiteral,
    ExprUnaryOp, Number, Operator, UnaryOp,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::{BindingKind, SemanticModel};
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, is_macro::Is)]
pub(crate) enum IsStrictlyInt {
    /// The value is known with absolute certainty to be a strict instance of `int`.
    True,
    /// The value is known with absolute certainty to *not* be a strict instance of `int`.
    False,
    /// The evaluation context has a high chance of producing a strict instance of `int`.
    Likely,
    /// It is not possible to statically determine that the value
    /// is or is not a strict instance of `int`.
    Maybe,
}

/// RUF046
pub(crate) fn unnecessary_cast_to_int(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    let Some(argument) = single_argument_to_int_call(semantic, call) else {
        return;
    };

    let applicability = match expr_is_strictly_int(semantic, argument) {
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

pub(crate) fn expr_is_strictly_int(semantic: &SemanticModel, expr: &Expr) -> IsStrictlyInt {
    match expr {
        Expr::BoolOp(_) => IsStrictlyInt::Maybe,
        Expr::Await(_) => IsStrictlyInt::Maybe,
        Expr::Attribute(_) => IsStrictlyInt::Maybe,
        Expr::Subscript(_) => IsStrictlyInt::Maybe,

        Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
            Number::Int(_) => IsStrictlyInt::True,
            Number::Float(_) => IsStrictlyInt::False,
            Number::Complex { .. } => IsStrictlyInt::False,
        },

        Expr::Compare(ExprCompare {
            left,
            ops,
            comparators,
            ..
        }) => {
            let ([only_op], [right]) = (ops.as_ref(), comparators.as_ref()) else {
                return IsStrictlyInt::Maybe;
            };

            match only_op {
                CmpOp::Is => return IsStrictlyInt::False,
                CmpOp::IsNot => return IsStrictlyInt::False,
                _ => {}
            };

            let left_is_strictly_int = expr_is_strictly_int(semantic, left);
            let right_is_strictly_int = expr_is_strictly_int(semantic, right);

            match (left_is_strictly_int, right_is_strictly_int) {
                (IsStrictlyInt::True, IsStrictlyInt::True) => IsStrictlyInt::False,
                (_, IsStrictlyInt::True) => match only_op {
                    CmpOp::In => IsStrictlyInt::False,
                    CmpOp::NotIn => IsStrictlyInt::False,
                    _ => IsStrictlyInt::Maybe,
                },
                _ => IsStrictlyInt::Maybe,
            }
        }

        Expr::Named(ExprNamed { value, .. }) => expr_is_strictly_int(semantic, value),

        Expr::UnaryOp(ExprUnaryOp { op, operand, .. }) => {
            if matches!(op, UnaryOp::Not) {
                return IsStrictlyInt::False;
            }

            expr_is_strictly_int(semantic, operand)
        }

        Expr::BinOp(ExprBinOp {
            left, op, right, ..
        }) => {
            let left_is_strictly_int = expr_is_strictly_int(semantic, left);
            let right_is_strictly_int = expr_is_strictly_int(semantic, right);

            match (left_is_strictly_int, right_is_strictly_int) {
                (IsStrictlyInt::True, IsStrictlyInt::True) => match op {
                    Operator::Div => IsStrictlyInt::False,
                    Operator::MatMult => IsStrictlyInt::False,
                    _ => IsStrictlyInt::True,
                },
                (IsStrictlyInt::Likely, IsStrictlyInt::Likely) => match op {
                    Operator::Div => IsStrictlyInt::Maybe,
                    Operator::MatMult => IsStrictlyInt::Maybe,
                    _ => IsStrictlyInt::Likely,
                },
                _ => IsStrictlyInt::Maybe,
            }
        }

        Expr::If(ExprIf { body, orelse, .. }) => {
            let body_is_strictly_int = expr_is_strictly_int(semantic, body);
            let else_is_strictly_int = expr_is_strictly_int(semantic, orelse);

            match (body_is_strictly_int, else_is_strictly_int) {
                (IsStrictlyInt::True, IsStrictlyInt::True) => IsStrictlyInt::True,
                (IsStrictlyInt::Likely, IsStrictlyInt::Likely) => IsStrictlyInt::Likely,
                (IsStrictlyInt::False, IsStrictlyInt::False) => IsStrictlyInt::False,
                _ => IsStrictlyInt::Maybe,
            }
        }

        Expr::Name(name) => {
            let Some(binding_id) = semantic.only_binding(name) else {
                return IsStrictlyInt::Maybe;
            };
            let binding = semantic.binding(binding_id);

            if typing::is_int(binding, semantic) {
                return IsStrictlyInt::Maybe;
            }

            match binding.kind {
                // Already handled by typing::is_int/typing::check_type
                BindingKind::Assignment => IsStrictlyInt::Maybe,
                BindingKind::NamedExprAssignment => IsStrictlyInt::Maybe,
                BindingKind::WithItemVar => IsStrictlyInt::Maybe,
                BindingKind::Argument => IsStrictlyInt::Maybe,
                BindingKind::Annotation => IsStrictlyInt::Maybe,

                BindingKind::Import(_) => IsStrictlyInt::Maybe,
                BindingKind::FromImport(_) => IsStrictlyInt::Maybe,
                BindingKind::SubmoduleImport(_) => IsStrictlyInt::Maybe,
                BindingKind::Deletion => IsStrictlyInt::Maybe,
                BindingKind::ConditionalDeletion(_) => IsStrictlyInt::Maybe,
                BindingKind::LoopVar => IsStrictlyInt::Maybe,
                BindingKind::Global(_) => IsStrictlyInt::Maybe,
                BindingKind::Nonlocal(_, _) => IsStrictlyInt::Maybe,

                _ => IsStrictlyInt::False,
            }
        }

        Expr::Call(call) => call_strictly_returns_int(semantic, call),

        _ => IsStrictlyInt::False,
    }
}

fn call_strictly_returns_int(semantic: &SemanticModel, call: &ExprCall) -> IsStrictlyInt {
    let (func, arguments) = (&call.func, &call.arguments);

    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return IsStrictlyInt::Maybe;
    };

    match qualified_name.segments() {
        ["" | "builtins", "len" | "id" | "hash" | "ord" | "int"]
        | ["math", "comb" | "factorial" | "gcd" | "lcm" | "isqrt" | "perm"] => IsStrictlyInt::True,

        // Depends on `__ceil__`/`__floor__`/`__trunc__`
        ["math", "ceil" | "floor" | "trunc"] => IsStrictlyInt::Likely,

        // Depends on `ndigits` and `number.__round__`
        ["" | "builtins", "round"] => round_call_strictly_returns_int(semantic, arguments),

        _ => IsStrictlyInt::Maybe,
    }
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

fn round_call_strictly_returns_int(
    semantic: &SemanticModel,
    arguments: &Arguments,
) -> IsStrictlyInt {
    let Some((number, ndigits)) = round_number_and_ndigits(arguments) else {
        return IsStrictlyInt::Maybe;
    };

    let number_kind = match number {
        Expr::Name(name) => match semantic.only_binding(name).map(|id| semantic.binding(id)) {
            Some(binding) if typing::is_int(binding, semantic) => Rounded::InferredInt,
            Some(binding) if typing::is_float(binding, semantic) => Rounded::InferredFloat,
            _ => Rounded::Other,
        },

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

    match (number_kind, ndigits_kind) {
        (Rounded::LiteralInt, Ndigits::LiteralInt)
        | (Rounded::LiteralInt | Rounded::LiteralFloat, Ndigits::NotGiven | Ndigits::LiteralNone) => {
            IsStrictlyInt::True
        }

        (Rounded::InferredInt, Ndigits::LiteralInt)
        | (
            Rounded::InferredInt | Rounded::InferredFloat | Rounded::Other,
            Ndigits::NotGiven | Ndigits::LiteralNone,
        ) => IsStrictlyInt::Likely,

        _ => IsStrictlyInt::Maybe,
    }
}

fn round_number_and_ndigits(arguments: &Arguments) -> Option<(&Expr, Option<&Expr>)> {
    if arguments.len() > 2 {
        return None;
    }

    let number = arguments.find_argument("number", 0)?;
    let ndigits = arguments.find_argument("ndigits", 1);

    Some((number, ndigits))
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
