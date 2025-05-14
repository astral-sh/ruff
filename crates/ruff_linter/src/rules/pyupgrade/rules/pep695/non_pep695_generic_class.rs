use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{ExprSubscript, StmtClassDef};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use ruff_python_ast::PythonVersion;

use super::{
    check_type_vars, find_generic, in_nested_context, DisplayTypeVars, TypeVarReferenceVisitor,
};

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
/// The rule currently skips generic classes nested inside of other functions or classes. It also
/// skips type parameters with the `default` argument introduced in [PEP 696] and implemented in
/// Python 3.13.
///
/// This rule can only offer a fix if all of the generic types in the class definition are defined
/// in the current module. For external type parameters, a diagnostic is emitted without a suggested
/// fix.
///
/// Not all type checkers fully support PEP 695 yet, so even valid fixes suggested by this rule may
/// cause type checking to fail.
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
/// [`unused-private-type-var`][PYI018] for a rule to clean up unused
/// private type variables.
///
/// This rule will not rename private type variables to remove leading underscores, even though the
/// new type parameters are restricted in scope to their associated class. See
/// [`private-type-parameter`][UP049] for a rule to update these names.
///
/// This rule will correctly handle classes with multiple base classes, as long as the single
/// `Generic` base class is at the end of the argument list, as checked by
/// [`generic-not-last-base-class`][PYI059]. If a `Generic` base class is
/// found outside of the last position, a diagnostic is emitted without a suggested fix.
///
/// This rule only applies to generic classes and does not include generic functions. See
/// [`non-pep695-generic-function`][UP047] for the function version.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [PEP 696]: https://peps.python.org/pep-0696/
/// [PYI018]: https://docs.astral.sh/ruff/rules/unused-private-type-var/
/// [PYI059]: https://docs.astral.sh/ruff/rules/generic-not-last-base-class/
/// [UP047]: https://docs.astral.sh/ruff/rules/non-pep695-generic-function/
/// [UP049]: https://docs.astral.sh/ruff/rules/private-type-parameter/
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
pub(crate) fn non_pep695_generic_class(checker: &Checker, class_def: &StmtClassDef) {
    // PEP-695 syntax is only available on Python 3.12+
    if checker.target_version() < PythonVersion::PY312 {
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

    let Some((generic_idx, generic_expr @ ExprSubscript { slice, range, .. })) =
        find_generic(arguments, checker.semantic())
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        NonPEP695GenericClass {
            name: name.to_string(),
        },
        *range,
    );

    // only handle the case where Generic is at the end of the argument list, in line with PYI059
    // (generic-not-last-base-class). If it comes elsewhere, it results in a runtime error. In stubs
    // it's not *strictly* necessary for `Generic` to come last in the bases tuple, but it would
    // cause more complication for us to handle stubs specially, and probably isn't worth the
    // bother. we still offer a diagnostic here but not a fix
    //
    // because `find_generic` also finds the *first* Generic argument, this has the additional
    // benefit of bailing out with a diagnostic if multiple Generic arguments are present
    if generic_idx != arguments.len() - 1 {
        checker.report_diagnostic(diagnostic);
        return;
    }

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

        diagnostic.try_set_fix(|| {
            let removal_edit = remove_argument(
                generic_expr,
                arguments,
                Parentheses::Remove,
                checker.source(),
            )?;
            Ok(Fix::unsafe_edits(
                Edit::insertion(type_params.to_string(), name.end()),
                [removal_edit],
            ))
        });
    }

    checker.report_diagnostic(diagnostic);
}
