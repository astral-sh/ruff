use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::StmtFunctionDef;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

use super::{check_type_vars, in_nested_context, DisplayTypeVars, TypeVarReferenceVisitor};

/// ## What it does
///
/// Checks for use of standalone type variables and parameter specifications in generic functions.
///
/// ## Why is this bad?
///
/// Special type parameter syntax was introduced in Python 3.12 by [PEP 695] for defining generic
/// functions. This syntax is easier to read and provides cleaner support for generics.
///
/// ## Known problems
///
/// The rule currently skips generic functions nested inside of other functions or classes and those
/// with type parameters containing the `default` argument introduced in [PEP 696] and implemented
/// in Python 3.13.
///
/// Not all type checkers fully support PEP 695 yet, so even valid fixes suggested by this rule may
/// cause type checking to fail.
///
/// ## Fix safety
///
/// This fix is marked unsafe, as [PEP 695] uses inferred variance for type parameters, instead of
/// the `covariant` and `contravariant` keywords used by `TypeVar` variables. As such, replacing a
/// `TypeVar` variable with an inline type parameter may change its variance.
///
/// Additionally, if the rule cannot determine whether a parameter annotation corresponds to a type
/// variable (e.g. for a type imported from another module), it will not add the type to the generic
/// type parameter list. This causes the function to have a mix of old-style type variables and
/// new-style generic type parameters, which will be rejected by type checkers.
///
/// ## Example
///
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T")
///
///
/// def generic_function(var: T) -> T:
///     return var
/// ```
///
/// Use instead:
///
/// ```python
/// def generic_function[T](var: T) -> T:
///     return var
/// ```
///
/// ## See also
///
/// This rule replaces standalone type variables in function signatures but doesn't remove
/// the corresponding type variables even if they are unused after the fix. See
/// [`unused-private-type-var`](unused-private-type-var.md) for a rule to clean up unused
/// private type variables.
///
/// This rule only applies to generic functions and does not include generic classes. See
/// [`non-pep695-generic-class`](non-pep695-generic-class.md) for the class version.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [PEP 696]: https://peps.python.org/pep-0696/
#[derive(ViolationMetadata)]
pub(crate) struct NonPEP695GenericFunction {
    name: String,
}

impl Violation for NonPEP695GenericFunction {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP695GenericFunction { name } = self;
        format!("Generic function `{name}` should use type parameters")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use type parameters".to_string())
    }
}

/// UP047
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
                    any_skipped: false,
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
            NonPEP695GenericFunction {
                name: name.to_string(),
            },
            TextRange::new(name.start(), parameters.end()),
        )
        .with_fix(Fix::unsafe_edit(Edit::insertion(
            type_params.to_string(),
            name.end(),
        ))),
    );
}
