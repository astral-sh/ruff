use std::fmt::{Display, Formatter};

use ruff_python_ast::{
    Expr, ExprBinOp, ExprSubscript, ExprTuple, Identifier, Operator, ParameterWithDefault,
    Parameters,
};
use smallvec::SmallVec;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for incorrect function signatures on `__exit__` and `__aexit__`
/// methods.
///
/// ## Why is this bad?
/// Improperly-annotated `__exit__` and `__aexit__` methods can cause
/// unexpected behavior when interacting with type checkers.
///
/// ## Example
/// ```python
/// class Foo:
///     def __exit__(self, typ, exc, tb, extra_arg) -> None:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __exit__(
///         self,
///         typ: type[BaseException] | None,
///         exc: BaseException | None,
///         tb: TracebackType | None,
///         extra_arg: int = 0,
///     ) -> None:
///         ...
/// ```
#[violation]
pub struct BadExitAnnotation {
    func_kind: FuncKind,
    error_kind: ErrorKind,
}

impl Violation for BadExitAnnotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let method_name = self.func_kind.to_string();
        match self.error_kind {
            ErrorKind::StarArgsNotAnnotated => format!("Star-args in `{method_name}` should be annotated with `object`"),
            ErrorKind::MissingArgs => format!("If there are no star-args, `{method_name}` should have at least 3 non-keyword-only args (excluding `self`)"),
            ErrorKind::ArgsAfterFirstFourMustHaveDefault => format!("All arguments after the first four in `{method_name}` must have a default value"),
            ErrorKind::AllKwargsMustHaveDefault => format!("All keyword-only arguments in `{method_name}` must have a default value"),
            ErrorKind::FirstArgBadAnnotation => format!("The first argument in `{method_name}` should be annotated with `object` or `type[BaseException] | None`"),
            ErrorKind::SecondArgBadAnnotation => format!("The second argument in `{method_name}` should be annotated with `object` or `BaseException | None`"),
            ErrorKind::ThirdArgBadAnnotation => format!("The third argument in `{method_name}` should be annotated with `object` or `types.TracebackType | None`"),
        }
    }

    fn autofix_title(&self) -> Option<String> {
        if matches!(self.error_kind, ErrorKind::StarArgsNotAnnotated) {
            Some("Annotate star-args with `object`".to_string())
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum FuncKind {
    Sync,
    Async,
}

impl Display for FuncKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FuncKind::Sync => write!(f, "__exit__"),
            FuncKind::Async => write!(f, "__aexit__"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ErrorKind {
    StarArgsNotAnnotated,
    MissingArgs,
    FirstArgBadAnnotation,
    SecondArgBadAnnotation,
    ThirdArgBadAnnotation,
    ArgsAfterFirstFourMustHaveDefault,
    AllKwargsMustHaveDefault,
}

/// PYI036
pub(crate) fn bad_exit_annotation(
    checker: &mut Checker,
    is_async: bool,
    name: &Identifier,
    parameters: &Parameters,
) {
    let func_kind = match name.as_str() {
        "__exit__" if !is_async => FuncKind::Sync,
        "__aexit__" if is_async => FuncKind::Async,
        _ => return,
    };

    let positional_args = parameters
        .args
        .iter()
        .chain(parameters.posonlyargs.iter())
        .collect::<SmallVec<[&ParameterWithDefault; 4]>>();

    // If there are less than three positional arguments, at least one of them must be a star-arg,
    // and it must be annotated with `object`.
    if positional_args.len() < 4 {
        check_short_args_list(checker, parameters, func_kind);
    }

    // Every positional argument (beyond the first four) must have a default.
    for parameter in positional_args
        .iter()
        .skip(4)
        .filter(|parameter| parameter.default.is_none())
    {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::ArgsAfterFirstFourMustHaveDefault,
            },
            parameter.range(),
        ));
    }

    // ...as should all keyword-only arguments.
    for parameter in parameters
        .kwonlyargs
        .iter()
        .filter(|arg| arg.default.is_none())
    {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::AllKwargsMustHaveDefault,
            },
            parameter.range(),
        ));
    }

    check_positional_args(checker, &positional_args, func_kind);
}

/// Determine whether a "short" argument list (i.e., an argument list with less than four elements)
/// contains a star-args argument annotated with `object`. If not, report an error.
fn check_short_args_list(checker: &mut Checker, parameters: &Parameters, func_kind: FuncKind) {
    if let Some(varargs) = &parameters.vararg {
        if let Some(annotation) = varargs
            .annotation
            .as_ref()
            .filter(|ann| !is_object_or_unused(ann, checker.semantic()))
        {
            let mut diagnostic = Diagnostic::new(
                BadExitAnnotation {
                    func_kind,
                    error_kind: ErrorKind::StarArgsNotAnnotated,
                },
                annotation.range(),
            );

            if checker.patch(diagnostic.kind.rule()) {
                if checker.semantic().is_builtin("object") {
                    diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                        "object".to_string(),
                        annotation.range(),
                    )));
                }
            }

            checker.diagnostics.push(diagnostic);
        }
    } else {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::MissingArgs,
            },
            parameters.range(),
        ));
    }
}

/// Determines whether the positional arguments of an `__exit__` or `__aexit__` method are
/// annotated correctly.
fn check_positional_args(
    checker: &mut Checker,
    positional_args: &[&ParameterWithDefault],
    kind: FuncKind,
) {
    // For each argument, define the predicate against which to check the annotation.
    type AnnotationValidator = fn(&Expr, &SemanticModel) -> bool;

    let validations: [(ErrorKind, AnnotationValidator); 3] = [
        (ErrorKind::FirstArgBadAnnotation, is_base_exception_type),
        (ErrorKind::SecondArgBadAnnotation, is_base_exception),
        (ErrorKind::ThirdArgBadAnnotation, is_traceback_type),
    ];

    for (arg, (error_info, predicate)) in positional_args.iter().skip(1).take(3).zip(validations) {
        let Some(annotation) = arg.parameter.annotation.as_ref() else {
            continue;
        };

        if is_object_or_unused(annotation, checker.semantic()) {
            continue;
        }

        // If there's an annotation that's not `object` or `Unused`, check that the annotated type
        // matches the predicate.
        if non_none_annotation_element(annotation, checker.semantic())
            .is_some_and(|elem| predicate(elem, checker.semantic()))
        {
            continue;
        }

        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind: kind,
                error_kind: error_info,
            },
            annotation.range(),
        ));
    }
}

/// Return the non-`None` annotation element of a PEP 604-style union or `Optional` annotation.
fn non_none_annotation_element<'a>(
    annotation: &'a Expr,
    semantic: &SemanticModel,
) -> Option<&'a Expr> {
    // E.g., `typing.Union` or `typing.Optional`
    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = annotation {
        if semantic.match_typing_expr(value, "Optional") {
            return if is_const_none(slice) {
                None
            } else {
                Some(slice)
            };
        }

        if !semantic.match_typing_expr(value, "Union") {
            return None;
        }

        let Expr::Tuple(ExprTuple { elts, .. }) = slice.as_ref() else {
            return None;
        };

        let [left, right] = elts.as_slice() else {
            return None;
        };

        return match (is_const_none(left), is_const_none(right)) {
            (false, true) => Some(left),
            (true, false) => Some(right),
            (true, true) => None,
            (false, false) => None,
        };
    }

    // PEP 604-style union (e.g., `int | None`)
    if let Expr::BinOp(ExprBinOp {
        op: Operator::BitOr,
        left,
        right,
        ..
    }) = annotation
    {
        if !is_const_none(left) {
            return Some(left);
        }

        if !is_const_none(right) {
            return Some(right);
        }

        return None;
    }

    None
}

/// Return `true` if the [`Expr`] is the `object` builtin or the `_typeshed.Unused` type.
fn is_object_or_unused(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["" | "builtins", "object"] | ["_typeshed", "Unused"]
            )
        })
}

/// Return `true` if the [`Expr`] is `BaseException`.
fn is_base_exception(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["" | "builtins", "BaseException"]))
}

/// Return `true` if the [`Expr`] is the `types.TracebackType` type.
fn is_traceback_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["types", "TracebackType"]))
}

/// Return `true` if the [`Expr`] is, e.g., `Type[BaseException]`.
fn is_base_exception_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr else {
        return false;
    };

    if semantic.match_typing_expr(value, "Type")
        || semantic
            .resolve_call_path(value)
            .as_ref()
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["" | "builtins", "type"]))
    {
        is_base_exception(slice, semantic)
    } else {
        false
    }
}
