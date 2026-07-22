use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, Decorator, Expr, Parameters};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::visibility;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::preview::is_boolean_type_hint_pos_arg_literal_enabled;
use crate::rules::flake8_boolean_trap::helpers::{
    add_liskov_substitution_principle_help, is_allowed_func_def,
};
use crate::settings::LinterSettings;

/// ## What it does
/// Checks for the use of boolean positional arguments in function definitions,
/// as determined by the presence of a type hint containing `bool` as an
/// evident subtype - e.g. `bool`, `bool | int`, `typing.Optional[bool]`, etc.
///
/// ## Why is this bad?
/// Calling a function with boolean positional arguments is confusing as the
/// meaning of the boolean value is not clear to the caller and to future
/// readers of the code.
///
/// The use of a boolean will also limit the function to only two possible
/// behaviors, which makes the function difficult to extend in the future.
///
/// Instead, consider refactoring into separate implementations for the
/// `True` and `False` cases, using an `Enum`, or making the argument a
/// keyword-only argument, to force callers to be explicit when providing
/// the argument.
///
/// Dunder methods that define operators are exempt from this rule, as are
/// setters and [`@override`][override] definitions.
///
/// ## Example
///
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number: float, up: bool) -> int:
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5, True)  # What does `True` mean?
/// round_number(1.5, False)  # What does `False` mean?
/// ```
///
/// Instead, refactor into separate implementations:
///
/// ```python
/// from math import ceil, floor
///
///
/// def round_up(number: float) -> int:
///     return ceil(number)
///
///
/// def round_down(number: float) -> int:
///     return floor(number)
///
///
/// round_up(1.5)
/// round_down(1.5)
/// ```
///
/// Or, refactor to use an `Enum`:
///
/// ```python
/// from enum import Enum
///
///
/// class RoundingMethod(Enum):
///     UP = 1
///     DOWN = 2
///
///
/// def round_number(value: float, method: RoundingMethod) -> float: ...
/// ```
///
/// Or, make the argument a keyword-only argument:
///
/// ```python
/// from math import ceil, floor
///
///
/// def round_number(number: float, *, up: bool) -> int:
///     return ceil(number) if up else floor(number)
///
///
/// round_number(1.5, up=True)
/// round_number(1.5, up=False)
/// ```
///
/// ## Preview
/// When [preview] is enabled, this rule also flags `typing.Literal` annotations that
/// include a boolean literal as a variant - e.g. `Literal[True, False]`.
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
///
/// [override]: https://docs.python.org/3/library/typing.html#typing.override
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.127")]
pub(crate) struct BooleanTypeHintPositionalArgument;

impl Violation for BooleanTypeHintPositionalArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Boolean-typed positional argument in function definition".to_string()
    }
}

/// FBT001
pub(crate) fn boolean_type_hint_positional_argument(
    checker: &Checker,
    name: &str,
    decorator_list: &[Decorator],
    parameters: &Parameters,
) {
    // https://github.com/astral-sh/ruff/issues/14535
    if checker.source_type.is_stub() {
        return;
    }
    // Allow Boolean type hints in explicitly-allowed functions.
    if is_allowed_func_def(name) {
        return;
    }

    for parameter in parameters.posonlyargs.iter().chain(&parameters.args) {
        let Some(annotation) = parameter.annotation() else {
            continue;
        };
        if !match_annotation_to_complex_bool(annotation, checker.semantic(), checker.settings()) {
            continue;
        }

        // Allow Boolean type hints in setters.
        if decorator_list.iter().any(|decorator| {
            UnqualifiedName::from_expr(&decorator.expression)
                .is_some_and(|unqualified_name| unqualified_name.segments() == [name, "setter"])
        }) {
            return;
        }

        // Allow Boolean defaults in `@override` methods, since they're required to adhere to
        // the parent signature.
        if visibility::is_override(decorator_list, checker.semantic()) {
            return;
        }

        // If `bool` isn't actually a reference to the `bool` built-in, return.
        if !checker.semantic().has_builtin_binding("bool") {
            return;
        }

        let mut diagnostic =
            checker.report_diagnostic(BooleanTypeHintPositionalArgument, parameter.identifier());

        add_liskov_substitution_principle_help(&mut diagnostic, name, decorator_list, checker);
    }
}

/// Returns `true` if the annotation is a boolean type hint (e.g., `bool`), or a type hint that
/// includes boolean as a variant (e.g., `bool | int`).
fn match_annotation_to_complex_bool(
    annotation: &Expr,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> bool {
    match annotation {
        // Ex) `bool`
        Expr::Name(name) => &name.id == "bool",
        // Ex) `"bool"`
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value == "bool",
        // Ex) `bool | int`
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: ast::Operator::BitOr,
            right,
            ..
        }) => {
            match_annotation_to_complex_bool(left, semantic, settings)
                || match_annotation_to_complex_bool(right, semantic, settings)
        }
        // Ex) `typing.Union[bool, int]`
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            // If the typing modules were never imported, we'll never match below.
            if !semantic.seen_typing() {
                return false;
            }

            let qualified_name = semantic.resolve_qualified_name(value);
            if qualified_name.as_ref().is_some_and(|qualified_name| {
                semantic.match_typing_qualified_name(qualified_name, "Union")
            }) {
                if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                    elts.iter()
                        .any(|elt| match_annotation_to_complex_bool(elt, semantic, settings))
                } else {
                    // Union with a single type is an invalid type annotation
                    false
                }
            } else if qualified_name.as_ref().is_some_and(|qualified_name| {
                semantic.match_typing_qualified_name(qualified_name, "Optional")
            }) {
                match_annotation_to_complex_bool(slice, semantic, settings)
            } else if is_boolean_type_hint_pos_arg_literal_enabled(settings)
                && qualified_name.as_ref().is_some_and(|qualified_name| {
                    semantic.match_typing_qualified_name(qualified_name, "Literal")
                })
            {
                // Ex) `typing.Literal[True, False]`, `typing.Literal[True, None]`
                match slice.as_ref() {
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        let mut seen_bool = false;
                        for elt in elts {
                            match elt {
                                Expr::BooleanLiteral(_) => seen_bool = true,
                                Expr::NoneLiteral(_) => {}
                                _ => return false,
                            }
                        }
                        seen_bool
                    }
                    Expr::BooleanLiteral(_) => true,
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}
