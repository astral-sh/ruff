use crate::checkers::ast::Checker;
use crate::Locator;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprNumberLiteral, Number};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::find_newline;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `round()` calls that have no effect on the input.
///
/// ## Why is this bad?
/// Rounding a value that's already an integer is unnecessary.
/// It's clearer to use the value directly.
///
/// ## Example
///
/// ```python
/// a = round(1, 0)
/// ```
///
/// Use instead:
///
/// ```python
/// a = 1
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryRound;

impl AlwaysFixableViolation for UnnecessaryRound {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Value being rounded is already an integer".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary `round` call".to_string()
    }
}

/// RUF057
pub(crate) fn unnecessary_round(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().match_builtin_expr(&call.func, "round") {
        return;
    }

    let Some((rounded, rounded_value, ndigits_value)) =
        rounded_and_ndigits(&call.arguments, checker.semantic())
    else {
        return;
    };

    if !matches!(
        ndigits_value,
        NdigitsValue::NotGivenOrNone | NdigitsValue::LiteralInt { is_negative: false }
    ) {
        return;
    }

    let mut applicability = match rounded_value {
        // ```python
        // some_int: int
        //
        // rounded(1)
        // rounded(1, None)
        // rounded(1, 42)
        // rounded(1, 4 + 2)
        // rounded(1, some_int)
        // ```
        RoundedValue::Int(InferredType::Equivalent) => Applicability::Safe,

        // ```python
        // some_int: int
        // some_other_int: int
        //
        // rounded(some_int)
        // rounded(some_int, None)
        // rounded(some_int, 42)
        // rounded(some_int, 4 + 2)
        // rounded(some_int, some_other_int)
        // ```
        RoundedValue::Int(InferredType::AssignableTo) => Applicability::Unsafe,

        _ => return,
    };

    if checker.comment_ranges().intersects(call.range()) {
        applicability = Applicability::Unsafe;
    }

    let edit = unwrap_round_call(call, rounded, checker.semantic(), checker.locator());
    let fix = Fix::applicable_edit(edit, applicability);

    let diagnostic = Diagnostic::new(UnnecessaryRound, call.range());

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InferredType {
    /// The value is an exact instance of the type in question.
    Equivalent,
    /// The value is an instance of the type in question or a subtype thereof.
    AssignableTo,
}

/// The type of the first argument to `round()`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RoundedValue {
    Int(InferredType),
    Float(InferredType),
    Other,
}

/// The type of the second argument to `round()`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NdigitsValue {
    NotGivenOrNone,
    LiteralInt { is_negative: bool },
    Int(InferredType),
    Other,
}

/// Extracts the rounded and `ndigits` values from `arguments`.
///
/// Returns a tripled where the first element is the rounded value's expression, the second is the rounded value,
///and the third is the `ndigits` value.
pub(super) fn rounded_and_ndigits<'a>(
    arguments: &'a Arguments,
    semantic: &'a SemanticModel,
) -> Option<(&'a Expr, RoundedValue, NdigitsValue)> {
    if arguments.len() > 2 {
        return None;
    }

    let rounded = arguments.find_argument_value("number", 0)?;
    let ndigits = arguments.find_argument_value("ndigits", 1);

    let rounded_kind = match rounded {
        Expr::Name(name) => match semantic.only_binding(name).map(|id| semantic.binding(id)) {
            Some(binding) if typing::is_int(binding, semantic) => {
                RoundedValue::Int(InferredType::AssignableTo)
            }
            Some(binding) if typing::is_float(binding, semantic) => {
                RoundedValue::Float(InferredType::AssignableTo)
            }
            _ => RoundedValue::Other,
        },

        _ => match ResolvedPythonType::from(rounded) {
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer)) => {
                RoundedValue::Int(InferredType::Equivalent)
            }
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float)) => {
                RoundedValue::Float(InferredType::Equivalent)
            }
            _ => RoundedValue::Other,
        },
    };

    let ndigits_kind = match ndigits {
        None | Some(Expr::NoneLiteral(_)) => NdigitsValue::NotGivenOrNone,

        Some(Expr::Name(name)) => {
            match semantic.only_binding(name).map(|id| semantic.binding(id)) {
                Some(binding) if typing::is_int(binding, semantic) => {
                    NdigitsValue::Int(InferredType::AssignableTo)
                }
                _ => NdigitsValue::Other,
            }
        }

        Some(Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(int),
            ..
        })) => match int.as_i64() {
            None => NdigitsValue::Int(InferredType::Equivalent),
            Some(value) => NdigitsValue::LiteralInt {
                is_negative: value < 0,
            },
        },

        Some(ndigits) => match ResolvedPythonType::from(ndigits) {
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer)) => {
                NdigitsValue::Int(InferredType::Equivalent)
            }
            _ => NdigitsValue::Other,
        },
    };

    Some((rounded, rounded_kind, ndigits_kind))
}

fn unwrap_round_call(
    call: &ExprCall,
    rounded: &Expr,
    semantic: &SemanticModel,
    locator: &Locator,
) -> Edit {
    let rounded_expr = locator.slice(rounded.range());
    let has_parent_expr = semantic.current_expression_parent().is_some();
    let new_content =
        if has_parent_expr || rounded.is_named_expr() || find_newline(rounded_expr).is_some() {
            format!("({rounded_expr})")
        } else {
            rounded_expr.to_string()
        };

    Edit::range_replacement(new_content, call.range)
}
