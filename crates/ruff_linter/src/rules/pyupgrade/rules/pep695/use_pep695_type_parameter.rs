use itertools::Itertools;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{visitor::Visitor, Expr, ExprSubscript};
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

use super::{DisplayTypeVars, TypeVar, TypeVarReferenceVisitor};

/// ## What it does
///
/// Checks for use of standalone type variables and parameter specifications in generic functions
/// and classes.
///
/// ## Why is this bad?
///
/// Special type parameter syntax was introduced in Python 3.12 by [PEP 695] for defining generic
/// functions and classes. This syntax is easier to read and provides cleaner support for generics.
///
/// ## Known problems
///
/// [PEP 695] uses inferred variance for type parameters, instead of the `covariant` and
/// `contravariant` keywords used by `TypeVar` variables. As such, rewriting a `TypeVar` variable to
/// an inline type parameter may change its variance.
///
/// The rule currently skips generic classes with multiple base classes, and skips
/// generic methods in generic or non-generic classes.
///
/// ## Example
///
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T")
///
///
/// class GenericClass(Generic[T]):
///     var: T
/// ```
///
/// Use instead:
///
/// ```python
/// class GenericClass[T]:
///     var: T
/// ```
///
/// ## See also
///
/// This rule replaces standalone type variables in class and function signatures but doesn't remove
/// the corresponding type variables even if they are unused after the fix. See
/// [`unused-private-type-var`](unused-private-type-var.md) for a rule to clean up unused type
/// variables.
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

/// Check if the current statement is nested within another [`StmtClassDef`] or [`StmtFunctionDef`].
fn in_nested_context(checker: &Checker) -> bool {
    checker
        .semantic()
        .current_statements()
        .skip(1) // skip the immediate parent, we only call this within a class or function
        .any(|stmt| matches!(stmt, Stmt::ClassDef(_) | Stmt::FunctionDef(_)))
}

/// UP046
pub(crate) fn non_pep695_generic_class(checker: &mut Checker, class_def: &StmtClassDef) {
    // PEP-695 syntax is only available on Python 3.12+
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    // don't try to handle generic classes inside other functions or classes
    if in_nested_context(checker) {
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
    let [Expr::Subscript(ExprSubscript {
        value,
        slice,
        range,
        ..
    })] = arguments.args.as_ref()
    else {
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

    let Some(type_vars) = check_type_vars(vars) else {
        return;
    };

    // build the fix as a String to avoid removing comments from the entire function body
    let type_params = DisplayTypeVars {
        type_vars: &type_vars,
        source: checker.source(),
    };

    checker.diagnostics.push(
        Diagnostic::new(
            NonPEP695TypeParameter {
                name: name.to_string(),
                generic_kind: GenericKind::GenericClass,
            },
            *range,
        )
        .with_fix(Fix::applicable_edit(
            Edit::replacement(type_params.to_string(), name.end(), arguments.end()),
            Applicability::Safe,
        )),
    );
}

/// Deduplicate `vars`, returning `None` if `vars` is empty or any duplicates are found.
fn check_type_vars(vars: Vec<TypeVar<'_>>) -> Option<Vec<TypeVar<'_>>> {
    if vars.is_empty() {
        return None;
    }

    // Type variables must be unique; filter while preserving order.
    let nvars = vars.len();
    let type_vars = vars
        .into_iter()
        .unique_by(|TypeVar { name, .. }| name.id.as_str())
        .collect::<Vec<_>>();

    // non-unique type variables are runtime errors, so just bail out here
    if type_vars.len() < nvars {
        return None;
    }

    Some(type_vars)
}

/// UP046
pub(crate) fn non_pep695_generic_function(checker: &mut Checker, function_def: &StmtFunctionDef) {
    // PEP-695 syntax is only available on Python 3.12+
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    // don't try to handle generic functions inside other functions or classes
    if in_nested_context(checker) {
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

    let Some(type_vars) = check_type_vars(type_vars) else {
        return;
    };

    // build the fix as a String to avoid removing comments from the entire function body
    let type_params = DisplayTypeVars {
        type_vars: &type_vars,
        source: checker.source(),
    };

    checker.diagnostics.push(
        Diagnostic::new(
            NonPEP695TypeParameter {
                name: name.to_string(),
                generic_kind: GenericKind::GenericFunction,
            },
            TextRange::new(name.start(), parameters.end()),
        )
        .with_fix(Fix::applicable_edit(
            Edit::insertion(type_params.to_string(), name.end()),
            Applicability::Safe,
        )),
    );
}
