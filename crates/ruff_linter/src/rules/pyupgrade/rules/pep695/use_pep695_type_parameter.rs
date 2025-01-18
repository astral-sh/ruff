use itertools::Itertools;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{visitor::Visitor, Expr, ExprSubscript};
use ruff_python_ast::{StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

use super::{fmt_type_vars, TypeVar, TypeVarReferenceVisitor};

/// ## What it does
///
/// Checks for use of `TypeParam`, `TypeParamTuple`, and `ParamSpec` annotations on generic
/// functions and classes.
///
/// ## Why is this bad?
///
/// Special type parameter syntax was introduced in Python 3.12 by [PEP 695] for defining generic
/// functions and classes. This syntax is easier to read and provides cleaner support for generics.
///
/// ## Known problems
///
/// [PEP 695] uses inferred variance for type parameters, instead of the `covariant` and
/// `contravariant` keywords used by `TypeParam` variables. As such, rewriting a `TypeParam`
/// variable to an in-line type parameter may change its variance.
///
/// Unlike `TypeParam` variables, [PEP 695]-style type parameters cannot be used at runtime. For
/// example, calling `isinstance(x, T)` with type parameter `T` will raise a `TypeError`. As such,
/// rewriting a `TypeParam` as a type parameter will cause issues for parameters that are used for
/// such runtime checks.
///
/// ## Example
/// ```python
/// T = TypeVar("T")
/// class GenericClass(Generic[T]):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// class GenericClass[T]:
///     ...
/// ```
///
/// [PEP 695]: https://peps.python.org/pep-0695/
#[derive(ViolationMetadata)]
pub(crate) struct NonPEP695TypeParameter {
    name: String,
    generic_kind: GenericKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum GenericKind {
    GenericClass,
    GenericFunction,
}

impl Violation for NonPEP695TypeParameter {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP695TypeParameter { name, generic_kind } = self;
        match generic_kind {
            GenericKind::GenericClass => {
                format!("Generic class `{name}` uses `Generic` subclass instead of type parameters")
            }
            GenericKind::GenericFunction => {
                format!("Generic function `{name}` should use type parameters")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use type parameters".to_string())
    }
}

/// UP046
pub(crate) fn non_pep695_generic_class(checker: &mut Checker, class_def: &StmtClassDef) {
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    let StmtClassDef {
        name,
        type_params,
        arguments,
        ..
    } = class_def;

    // it's a runtime error to mix type_params and Generic, so bail out early if we see existing
    // type_params
    if type_params.is_some() {
        return;
    }

    let Some(arguments) = arguments.as_ref() else {
        return;
    };

    // TODO(brent) only accept a single, Generic argument for now. I think it should be fine to have
    // other arguments, but this simplifies the fix just to delete the argument list for now
    let [Expr::Subscript(ExprSubscript { value, slice, .. })] = arguments.args.as_ref() else {
        return;
    };

    if !checker.semantic().match_typing_expr(value, "Generic") {
        return;
    }

    let vars = {
        let mut visitor = TypeVarReferenceVisitor {
            vars: vec![],
            semantic: checker.semantic(),
        };
        visitor.visit_expr(slice);
        visitor.vars
    };

    // Type variables must be unique; filter while preserving order.
    let mut type_vars = vars
        .into_iter()
        .unique_by(|TypeVar { name, .. }| name.id.as_str())
        .collect::<Vec<_>>();

    if type_vars.is_empty() {
        return;
    }

    // generally preserve order, but sort by kind so that the order will be TypeVar...,
    // TypeVarTuple..., ParamSpec...
    type_vars.sort_by_key(|tv| tv.kind);

    // build the fix as a String to avoid removing comments from the entire function body
    let type_params = fmt_type_vars(&type_vars, checker);

    checker.diagnostics.push(
        Diagnostic::new(
            NonPEP695TypeParameter {
                name: name.to_string(),
                generic_kind: GenericKind::GenericClass,
            },
            TextRange::new(name.start(), arguments.end()),
        )
        .with_fix(Fix::applicable_edit(
            Edit::replacement(type_params, name.end(), arguments.end()),
            Applicability::Safe,
        )),
    );
}

/// UP046
pub(crate) fn non_pep695_generic_function(checker: &mut Checker, function_def: &StmtFunctionDef) {
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    let StmtFunctionDef {
        name,
        type_params,
        parameters,
        ..
    } = function_def;

    // TODO(brent) handle methods, for now return early in a class body. For example, an additional
    // generic parameter on the method needs to be handled separately from one already on the class
    //
    // ```python
    // T = TypeVar("T")
    // S = TypeVar("S")
    //
    // class Foo(Generic[T]):
    //     def bar(self, x: T, y: S) -> S: ...
    //
    //
    // class Foo[T]:
    //     def bar[S](self, x: T, y: S) -> S: ...
    // ```
    if checker.semantic().current_scope().kind.is_class() {
        return;
    }

    // invalid to mix old-style and new-style generics
    if type_params.is_some() {
        return;
    }

    let mut type_vars = Vec::new();
    for parameter in parameters {
        if let Some(annotation) = parameter.annotation() {
            let vars = {
                let mut visitor = TypeVarReferenceVisitor {
                    vars: vec![],
                    semantic: checker.semantic(),
                };
                visitor.visit_expr(annotation);
                visitor.vars
            };
            type_vars.extend(vars);
        }
    }

    // Type variables must be unique; filter while preserving order.
    let mut type_vars = type_vars
        .into_iter()
        .unique_by(|TypeVar { name, .. }| name.id.as_str())
        .collect::<Vec<_>>();

    if type_vars.is_empty() {
        return;
    }

    // generally preserve order, but sort by kind so that the order will be TypeVar...,
    // TypeVarTuple..., ParamSpec...
    type_vars.sort_by_key(|tv| tv.kind);

    // build the fix as a String to avoid removing comments from the entire function body
    let type_params = fmt_type_vars(&type_vars, checker);

    checker.diagnostics.push(
        Diagnostic::new(
            NonPEP695TypeParameter {
                name: name.to_string(),
                generic_kind: GenericKind::GenericFunction,
            },
            TextRange::new(name.start(), parameters.end()),
        )
        .with_fix(Fix::applicable_edit(
            Edit::insertion(type_params, name.end()),
            Applicability::Safe,
        )),
    );
}
