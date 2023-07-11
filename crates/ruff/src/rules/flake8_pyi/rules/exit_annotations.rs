use std::fmt::{Display, Formatter};

use ruff_text_size::TextRange;
use rustpython_parser::ast::{
    ArgWithDefault, Arguments, Expr, ExprBinOp, ExprContext, ExprName, ExprSubscript, ExprTuple,
    Identifier, Operator, Ranged,
};
use smallvec::SmallVec;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

type AnnotationValidator = fn(&SemanticModel, &Expr) -> bool;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum FuncKind {
    Sync,
    Async,
}

impl Display for FuncKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let method_name = match self {
            FuncKind::Sync => "__exit__",
            FuncKind::Async => "__aexit__",
        };
        write!(f, "{method_name}")
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ErrorKind {
    StarArgsNotAnnotated,
    MissingArgs,
    FirstArgBadAnnotation,
    SecondArgBadAnnotation,
    ThirdArgBadAnnotation,
    ArgsAfterFirstFourMustHaveDefault,
    AllKwargsMustHaveDefault,
}

/// ## What it does
/// Checks for incorrect arguments and annotations in `__exit__` and `__aexit__` methods.
///
/// ## Why is this bad?
/// Incorrect arguments can cause runtime exceptions when a context manager is exited. Incorrect
/// annotations harm readability and can cause type checkers to report errors.
///
/// ## Example
/// ```python
/// class Foo:
///     def __exit__(self, typ, exc, tb, weird_extra_arg) -> None: ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, weird_extra_arg: int = ...) -> None: ...
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
            ErrorKind::StarArgsNotAnnotated => format!("Star-args in {method_name} should be annotated with `object`"),
            ErrorKind::MissingArgs => format!("If there are no star-args, {method_name} should have at least 3 non-keyword-only args (excluding self)"),
            ErrorKind::ArgsAfterFirstFourMustHaveDefault => format!("All arguments after the first four in `{method_name}` must have a default value"),
            ErrorKind::AllKwargsMustHaveDefault => format!("All keyword-only arguments in `{method_name}` must have a default value"),
            ErrorKind::FirstArgBadAnnotation => format!("The first argument in `{method_name}` should be annotated with `object` or `type[BaseException] | None`"),
            ErrorKind::SecondArgBadAnnotation => format!("The second argument in `{method_name}` should be annotated with `object` or `BaseException | None`"),
            ErrorKind::ThirdArgBadAnnotation => format!("The third argument in `{method_name}` should be annotated with `object` or `types.TracebackType | None`"),
        }
    }

    fn autofix_title(&self) -> Option<String> {
        if self.error_kind == ErrorKind::StarArgsNotAnnotated {
            Some("Annotate star-args with `object`".to_string())
        } else {
            None
        }
    }
}

fn is_object_or_unused(semantic: &SemanticModel, expr: &Expr) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .map_or(false, |cp| {
            matches!(
                cp.as_slice(),
                ["" | "builtins", "object"] | ["_typeshed", "Unused"]
            )
        })
}

fn is_base_exception(semantic: &SemanticModel, expr: &Expr) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .map_or(false, |cp| {
            matches!(cp.as_slice(), ["" | "builtins", "BaseException"])
        })
}

fn is_traceback_type(semantic: &SemanticModel, expr: &Expr) -> bool {
    semantic
        .resolve_call_path(expr)
        .as_ref()
        .map_or(false, |cp| {
            matches!(cp.as_slice(), ["types", "TracebackType"])
        })
}

fn is_base_exception_type(semantic: &SemanticModel, expr: &Expr) -> bool {
    let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr else {
        return false;
    };

    let is_type = semantic.match_typing_expr(value, "Type")
        || semantic
            .resolve_call_path(value)
            .as_ref()
            .map_or(false, |cp| {
                matches!(cp.as_slice(), ["" | "builtins", "type"])
            });

    is_type && is_base_exception(semantic, slice)
}

/// Returns the non-None annotation element of a union or typing.Optional annotation.
fn non_none_annotation_element<'a>(
    semantic: &SemanticModel,
    annotation: &'a Expr,
) -> Option<&'a Expr> {
    // typing.Union or typing.Optional
    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = annotation {
        if semantic.match_typing_expr(value, "Optional") {
            return Some(slice);
        }

        let Expr::Tuple(ExprTuple { elts, .. }) = slice.as_ref() else {
            return None;
        };

        if elts.len() != 2 {
            return None;
        }

        if !semantic.match_typing_expr(value, "Union") {
            return None;
        }

        return elts.iter().find(|e| !is_const_none(e));
    }

    // PEP 604 unions
    if let Expr::BinOp(ExprBinOp {
        op: Operator::BitOr,
        left,
        right,
        ..
    }) = annotation
    {
        return [left.as_ref(), right.as_ref()]
            .into_iter()
            .find(|e| !is_const_none(e));
    }

    None
}

pub(crate) fn bad_exit_annotation(
    checker: &mut Checker,
    is_async: bool,
    name: &Identifier,
    args: &Arguments,
) {
    let func_kind = match name.as_str() {
        "__exit__" if !is_async => FuncKind::Sync,
        "__aexit__" if is_async => FuncKind::Async,
        _ => return,
    };

    let positional_args = args
        .args
        .iter()
        .chain(args.posonlyargs.iter())
        .collect::<SmallVec<[&ArgWithDefault; 4]>>();

    // If there are less than three positional arguments, at least one of them must be a star-arg,
    // and it must be annotated with `object`
    if positional_args.len() < 4 {
        check_short_args_list(checker, args, func_kind);
    }

    // All positional args beyond first 4 must have a default...
    for arg in positional_args
        .iter()
        .skip(4)
        .filter(|arg| arg.default.is_none())
    {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::ArgsAfterFirstFourMustHaveDefault,
            },
            arg.range(),
        ));
    }

    // ...and so should all keyword-only args
    for kwarg_missing_default in args.kwonlyargs.iter().filter(|arg| arg.default.is_none()) {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::AllKwargsMustHaveDefault,
            },
            kwarg_missing_default.range(),
        ));
    }

    check_positional_args(checker, &positional_args, func_kind);
}

fn check_positional_args(
    checker: &mut Checker,
    positional_args: &[&ArgWithDefault],
    func_kind: FuncKind,
) {
    // For each argument we set up the extra predicate that the annotation needs to be checked
    // against
    let validations: [(ErrorKind, AnnotationValidator); 3] = [
        (ErrorKind::FirstArgBadAnnotation, is_base_exception_type),
        (ErrorKind::SecondArgBadAnnotation, is_base_exception),
        (ErrorKind::ThirdArgBadAnnotation, is_traceback_type),
    ];

    for (arg, (error_info, predicate)) in positional_args
        .iter()
        .skip(1)
        .take(3)
        .zip(validations.into_iter())
    {
        let Some(annotation) = arg.def.annotation.as_ref() else {
            continue;
        };

        if is_object_or_unused(checker.semantic(), annotation) {
            continue;
        }

        // If there's an annotation that's not `object` or `Unused`, check that the annotated type
        // matches the predicate. This in addition to matching None, which is checked inside
        // `possibly_non_none_annotation_element`).
        if non_none_annotation_element(checker.semantic(), annotation)
            .map_or(false, |elem| predicate(checker.semantic(), elem))
        {
            continue;
        }

        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: error_info,
            },
            annotation.range(),
        ));
    }
}

// Short args lists (less than four elements) must have a star-args argument annotated with `object`
fn check_short_args_list(checker: &mut Checker, args: &Arguments, func_kind: FuncKind) {
    let Some(varargs) = &args.vararg else {
        checker.diagnostics.push(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::MissingArgs,
            },
            args.range(),
        ));
        return;
    };

    if let Some(annotation) = varargs
        .annotation
        .as_ref()
        .filter(|ann| !is_object_or_unused(checker.semantic(), ann))
    {
        let mut diagnostic = Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::StarArgsNotAnnotated,
            },
            annotation.range(),
        );

        if checker.patch(diagnostic.kind.rule()) {
            let obj_expr = Expr::Name(ExprName {
                id: "object".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            });

            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                checker.generator().expr(&obj_expr),
                annotation.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}
