use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::StmtClassDef;
use ruff_python_ast::{Expr, ExprSubscript};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

use super::{check_type_vars, in_nested_context, DisplayTypeVars, TypeVarReferenceVisitor};

/// ## What it does
///
/// Checks for use of standalone type variables and parameter specifications in generic classes.
///
/// ## Why is this bad?
///
/// Special type parameter syntax was introduced in Python 3.12 by [PEP 695] for defining generic
/// classes. This syntax is easier to read and provides cleaner support for generics.
///
/// ## Known problems
///
/// The rule currently skips generic classes with multiple base classes. It also skips
/// generic classes nested inside of other
/// functions or classes. Finally, this rule skips type parameters with the `default` argument
/// introduced in [PEP 696] and implemented in Python 3.13.
///
/// This rule can only offer a fix if all of the generic types in the class definition are defined
/// in the current module. For external type parameters, a diagnostic is emitted without a suggested
/// fix.
///
/// ## Fix safety
///
/// This fix is marked as unsafe, as [PEP 695] uses inferred variance for type parameters, instead
/// of the `covariant` and `contravariant` keywords used by `TypeVar` variables. As such, replacing
/// a `TypeVar` variable with an inline type parameter may change its variance.
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
/// This rule replaces standalone type variables in classes but doesn't remove
/// the corresponding type variables even if they are unused after the fix. See
/// [`unused-private-type-var`](unused-private-type-var.md) for a rule to clean up unused
/// private type variables.
///
/// This rule only applies to generic classes and does not include generic functions. See
/// [`non-pep695-generic-function`](non-pep695-generic-function.md) for the function version.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [PEP 696]: https://peps.python.org/pep-0696/
#[derive(ViolationMetadata)]
pub(crate) struct NonPEP695GenericClass {
    name: String,
}

impl Violation for NonPEP695GenericClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP695GenericClass { name } = self;
        format!("Generic class `{name}` uses `Generic` subclass instead of type parameters")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use type parameters".to_string())
    }
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

    let mut diagnostic = Diagnostic::new(
        NonPEP695GenericClass {
            name: name.to_string(),
        },
        *range,
    );

    let mut visitor = TypeVarReferenceVisitor {
        vars: vec![],
        semantic: checker.semantic(),
        any_skipped: false,
    };
    visitor.visit_expr(slice);

    // if any of the parameters have been skipped, this indicates that we could not resolve the type
    // to a `TypeVar`, `TypeVarTuple`, or `ParamSpec`, and thus our fix would remove it from the
    // signature incorrectly. We can still offer the diagnostic created above without a Fix. For
    // example,
    //
    // ```python
    // from somewhere import SomethingElse
    //
    // T = TypeVar("T")
    //
    // class Class(Generic[T, SomethingElse]): ...
    // ```
    //
    // should not be converted to
    //
    // ```python
    // class Class[T]: ...
    // ```
    //
    // just because we can't confirm that `SomethingElse` is a `TypeVar`
    if !visitor.any_skipped {
        let Some(type_vars) = check_type_vars(visitor.vars) else {
            return;
        };

        // build the fix as a String to avoid removing comments from the entire function body
        let type_params = DisplayTypeVars {
            type_vars: &type_vars,
            source: checker.source(),
        };

        diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
            type_params.to_string(),
            name.end(),
            arguments.end(),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
