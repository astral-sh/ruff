use std::fmt::{Display, Formatter};

use ruff_python_ast::{
    Expr, ExprBinOp, ExprSubscript, ExprTuple, Operator, ParameterWithDefault, Parameters, Stmt,
    StmtClassDef, StmtFunctionDef,
};
use smallvec::SmallVec;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_semantic::{analyze::visibility::is_overload, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for incorrect function signatures on `__exit__` and `__aexit__`
/// methods.
///
/// ## Why is this bad?
/// Improperly annotated `__exit__` and `__aexit__` methods can cause
/// unexpected behavior when interacting with type checkers.
///
/// ## Example
///
/// ```pyi
/// from types import TracebackType
///
/// class Foo:
///     def __exit__(
///         self, typ: BaseException, exc: BaseException, tb: TracebackType
///     ) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// from types import TracebackType
///
/// class Foo:
///     def __exit__(
///         self,
///         typ: type[BaseException] | None,
///         exc: BaseException | None,
///         tb: TracebackType | None,
///     ) -> None: ...
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BadExitAnnotation {
    func_kind: FuncKind,
    error_kind: ErrorKind,
}

impl Violation for BadExitAnnotation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

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
            ErrorKind::UnrecognizedExitOverload => format!(
                "Annotations for a three-argument `{method_name}` overload (excluding `self`) \
                should either be `None, None, None` or `type[BaseException], BaseException, types.TracebackType`"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        if matches!(self.error_kind, ErrorKind::StarArgsNotAnnotated) {
            Some("Annotate star-args with `object`".to_string())
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, is_macro::Is)]
enum FuncKind {
    Sync,
    Async,
}

impl FuncKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Async => "__aexit__",
            Self::Sync => "__exit__",
        }
    }
}

impl Display for FuncKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
    UnrecognizedExitOverload,
}

/// PYI036
pub(crate) fn bad_exit_annotation(checker: &Checker, function: &StmtFunctionDef) {
    let StmtFunctionDef {
        is_async,
        decorator_list,
        name,
        parameters,
        ..
    } = function;

    let func_kind = match name.as_str() {
        "__exit__" if !is_async => FuncKind::Sync,
        "__aexit__" if *is_async => FuncKind::Async,
        _ => return,
    };

    let semantic = checker.semantic();

    let Some(Stmt::ClassDef(parent_class_def)) = semantic.current_statement_parent() else {
        return;
    };

    let non_self_positional_args: SmallVec<[&ParameterWithDefault; 3]> = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .skip(1)
        .collect();

    if is_overload(decorator_list, semantic) {
        check_positional_args_for_overloaded_method(
            checker,
            &non_self_positional_args,
            func_kind,
            parent_class_def,
            parameters.range(),
        );
        return;
    }

    // If there are less than three positional arguments, at least one of them must be a star-arg,
    // and it must be annotated with `object`.
    if non_self_positional_args.len() < 3 {
        check_short_args_list(checker, parameters, func_kind);
    }

    // Every positional argument (beyond the first four) must have a default.
    for parameter in non_self_positional_args
        .iter()
        .skip(3)
        .filter(|parameter| parameter.default.is_none())
    {
        checker.report_diagnostic(Diagnostic::new(
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
        checker.report_diagnostic(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::AllKwargsMustHaveDefault,
            },
            parameter.range(),
        ));
    }

    check_positional_args_for_non_overloaded_method(checker, &non_self_positional_args, func_kind);
}

/// Determine whether a "short" argument list (i.e., an argument list with less than four elements)
/// contains a star-args argument annotated with `object`. If not, report an error.
fn check_short_args_list(checker: &Checker, parameters: &Parameters, func_kind: FuncKind) {
    if let Some(varargs) = &parameters.vararg {
        if let Some(annotation) = varargs
            .annotation()
            .filter(|ann| !is_object_or_unused(ann, checker.semantic()))
        {
            let mut diagnostic = Diagnostic::new(
                BadExitAnnotation {
                    func_kind,
                    error_kind: ErrorKind::StarArgsNotAnnotated,
                },
                annotation.range(),
            );

            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                    "object",
                    annotation.start(),
                    checker.semantic(),
                )?;
                let binding_edit = Edit::range_replacement(binding, annotation.range());
                Ok(Fix::safe_edits(binding_edit, import_edit))
            });

            checker.report_diagnostic(diagnostic);
        }
    } else {
        checker.report_diagnostic(Diagnostic::new(
            BadExitAnnotation {
                func_kind,
                error_kind: ErrorKind::MissingArgs,
            },
            parameters.range(),
        ));
    }
}

/// Determines whether the positional arguments of an `__exit__` or `__aexit__` method
/// (that is not decorated with `@typing.overload`) are annotated correctly.
fn check_positional_args_for_non_overloaded_method(
    checker: &Checker,
    non_self_positional_params: &[&ParameterWithDefault],
    kind: FuncKind,
) {
    // For each argument, define the predicate against which to check the annotation.
    type AnnotationValidator = fn(&Expr, &SemanticModel) -> bool;

    let validations: [(ErrorKind, AnnotationValidator); 3] = [
        (ErrorKind::FirstArgBadAnnotation, is_base_exception_type),
        (ErrorKind::SecondArgBadAnnotation, |expr, semantic| {
            semantic.match_builtin_expr(expr, "BaseException")
        }),
        (ErrorKind::ThirdArgBadAnnotation, is_traceback_type),
    ];

    for (param, (error_info, predicate)) in
        non_self_positional_params.iter().take(3).zip(validations)
    {
        let Some(annotation) = param.annotation() else {
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

        checker.report_diagnostic(Diagnostic::new(
            BadExitAnnotation {
                func_kind: kind,
                error_kind: error_info,
            },
            annotation.range(),
        ));
    }
}

/// Determines whether the positional arguments of an `__exit__` or `__aexit__` method
/// overload are annotated correctly.
fn check_positional_args_for_overloaded_method(
    checker: &Checker,
    non_self_positional_args: &[&ParameterWithDefault],
    kind: FuncKind,
    parent_class_def: &StmtClassDef,
    parameters_range: TextRange,
) {
    fn parameter_annotation_loosely_matches_predicate(
        parameter: &ParameterWithDefault,
        predicate: impl FnOnce(&Expr) -> bool,
        semantic: &SemanticModel,
    ) -> bool {
        parameter.annotation().is_none_or(|annotation| {
            predicate(annotation) || is_object_or_unused(annotation, semantic)
        })
    }

    let semantic = checker.semantic();

    // Collect all the overloads for this method into a SmallVec
    let function_overloads: SmallVec<[&StmtFunctionDef; 2]> = parent_class_def
        .body
        .iter()
        .filter_map(|stmt| {
            let func_def = stmt.as_function_def_stmt()?;
            if &func_def.name == kind.as_str() && is_overload(&func_def.decorator_list, semantic) {
                Some(func_def)
            } else {
                None
            }
        })
        .collect();

    // If the number of overloads for this method is not exactly 2, don't do any checking
    if function_overloads.len() != 2 {
        return;
    }

    for function_def in &function_overloads {
        let StmtFunctionDef {
            is_async,
            parameters,
            ..
        } = function_def;

        // If any overloads are an unexpected sync/async colour, don't do any checking
        if *is_async != kind.is_async() {
            return;
        }

        // If any overloads have any variadic arguments, don't do any checking
        let Parameters {
            range: _,
            posonlyargs,
            args,
            vararg: None,
            kwonlyargs,
            kwarg: None,
        } = &**parameters
        else {
            return;
        };

        // If any overloads have any keyword-only arguments, don't do any checking
        if !kwonlyargs.is_empty() {
            return;
        }

        // If the number of non-keyword-only arguments is not exactly equal to 4
        // for any overloads, don't do any checking
        if posonlyargs.len() + args.len() != 4 {
            return;
        }
    }

    debug_assert!(
        function_overloads.contains(&semantic.current_statement().as_function_def_stmt().unwrap())
    );

    // We've now established that no overloads for this method have any variadic parameters,
    // no overloads have any keyword-only parameters, all overloads are the expected
    // sync/async colour, and all overloads have exactly 3 non-`self` non-keyword-only parameters.
    // The method we're currently looking at is one of those overloads.
    // It therefore follows that, in order for it to be correctly annotated, it must be
    // one of the following two possible overloads:
    //
    // ```
    // @overload
    // def __(a)exit__(self, typ: None, exc: None, tb: None) -> None: ...
    // @overload
    // def __(a)exit__(self, typ: type[BaseException], exc: BaseException, tb: TracebackType) -> None: ...
    // ```
    //
    // We'll allow small variations on either of these (if, e.g. a parameter is unannotated,
    // annotated with `object` or `_typeshed.Unused`). *Basically*, though, the rule is:
    // - If the function overload matches *either* of those, it's okay.
    // - If not: emit a diagnostic.

    // Start by checking the first possibility:
    if non_self_positional_args.iter().all(|parameter| {
        parameter_annotation_loosely_matches_predicate(
            parameter,
            Expr::is_none_literal_expr,
            semantic,
        )
    }) {
        return;
    }

    // Now check the second:
    if parameter_annotation_loosely_matches_predicate(
        non_self_positional_args[0],
        |annotation| is_base_exception_type(annotation, semantic),
        semantic,
    ) && parameter_annotation_loosely_matches_predicate(
        non_self_positional_args[1],
        |annotation| semantic.match_builtin_expr(annotation, "BaseException"),
        semantic,
    ) && parameter_annotation_loosely_matches_predicate(
        non_self_positional_args[2],
        |annotation| is_traceback_type(annotation, semantic),
        semantic,
    ) {
        return;
    }

    // Okay, neither of them match...
    checker.report_diagnostic(Diagnostic::new(
        BadExitAnnotation {
            func_kind: kind,
            error_kind: ErrorKind::UnrecognizedExitOverload,
        },
        parameters_range,
    ));
}

/// Return the non-`None` annotation element of a PEP 604-style union or `Optional` annotation.
fn non_none_annotation_element<'a>(
    annotation: &'a Expr,
    semantic: &SemanticModel,
) -> Option<&'a Expr> {
    // E.g., `typing.Union` or `typing.Optional`
    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = annotation {
        let qualified_name = semantic.resolve_qualified_name(value)?;

        if semantic.match_typing_qualified_name(&qualified_name, "Optional") {
            return if slice.is_none_literal_expr() {
                None
            } else {
                Some(slice)
            };
        }

        if !semantic.match_typing_qualified_name(&qualified_name, "Union") {
            return None;
        }

        let ExprTuple { elts, .. } = slice.as_tuple_expr()?;

        let [left, right] = elts.as_slice() else {
            return None;
        };

        return match (left.is_none_literal_expr(), right.is_none_literal_expr()) {
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
        if !left.is_none_literal_expr() {
            return Some(left);
        }

        if !right.is_none_literal_expr() {
            return Some(right);
        }

        return None;
    }

    None
}

/// Return `true` if the [`Expr`] is the `object` builtin or the `_typeshed.Unused` type.
fn is_object_or_unused(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins", "object"] | ["_typeshed", "Unused"]
            )
        })
}

/// Return `true` if the [`Expr`] is the `types.TracebackType` type.
fn is_traceback_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["types", "TracebackType"])
        })
}

/// Return `true` if the [`Expr`] is, e.g., `Type[BaseException]`.
fn is_base_exception_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr else {
        return false;
    };

    if semantic.match_typing_expr(value, "Type") || semantic.match_builtin_expr(value, "type") {
        semantic.match_builtin_expr(slice, "BaseException")
    } else {
        false
    }
}
