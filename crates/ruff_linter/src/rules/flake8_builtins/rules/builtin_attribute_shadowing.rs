use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::{BindingKind, Scope, ScopeId};
use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_builtins::helpers::shadows_builtin;

/// ## What it does
/// Checks for class attributes and methods that use the same names as
/// Python builtins.
///
/// ## Why is this bad?
/// Reusing a builtin name for the name of an attribute increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the attribute for the
/// builtin and vice versa.
///
/// Since methods and class attributes typically cannot be referenced directly
/// from outside the class scope, this rule only applies to those methods
/// and attributes that both shadow a builtin _and_ are referenced from within
/// the class scope, as in the following example, where the `list[int]` return
/// type annotation resolves to the `list` method, rather than the builtin:
///
/// ```python
/// class Class:
///     @staticmethod
///     def list() -> None:
///         pass
///
///     @staticmethod
///     def repeat(value: int, times: int) -> list[int]:
///         return [value] * times
/// ```
///
/// Builtins can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.builtins-ignorelist`] configuration option, or
/// converted to the appropriate dunder method. Methods decorated with
/// `@typing.override` or `@typing_extensions.override` are also
/// ignored.
///
/// ## Example
/// ```python
/// class Class:
///     @staticmethod
///     def list() -> None:
///         pass
///
///     @staticmethod
///     def repeat(value: int, times: int) -> list[int]:
///         return [value] * times
/// ```
///
/// ## Options
/// - `lint.flake8-builtins.builtins-ignorelist`
#[violation]
pub struct BuiltinAttributeShadowing {
    kind: Kind,
    name: String,
    row: SourceRow,
}

impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing { kind, name, row } = self;
        match kind {
            Kind::Attribute => {
                format!("Python builtin is shadowed by class attribute `{name}` from {row}")
            }
            Kind::Method => {
                format!("Python builtin is shadowed by method `{name}` from {row}")
            }
        }
    }
}

/// A003
pub(crate) fn builtin_attribute_shadowing(
    checker: &Checker,
    scope_id: ScopeId,
    scope: &Scope,
    class_def: &ast::StmtClassDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, binding_id) in scope.all_bindings() {
        let binding = checker.semantic().binding(binding_id);

        // We only care about methods and attributes.
        let kind = match binding.kind {
            BindingKind::Assignment | BindingKind::Annotation => Kind::Attribute,
            BindingKind::FunctionDefinition(_) => Kind::Method,
            _ => continue,
        };

        if shadows_builtin(
            name,
            checker.source_type,
            &checker.settings.flake8_builtins.builtins_ignorelist,
            checker.settings.target_version,
        ) {
            // Ignore explicit overrides.
            if class_def.decorator_list.iter().any(|decorator| {
                checker
                    .semantic()
                    .match_typing_expr(&decorator.expression, "override")
            }) {
                return;
            }

            // Class scopes are special, in that you can only reference a binding defined in a
            // class scope from within the class scope itself. As such, we can safely ignore
            // methods that weren't referenced from within the class scope. In other words, we're
            // only trying to identify shadowing as in:
            // ```python
            // class Class:
            //     @staticmethod
            //     def list() -> None:
            //         pass
            //
            //     @staticmethod
            //     def repeat(value: int, times: int) -> list[int]:
            //         return [value] * times
            // ```
            for reference in binding
                .references
                .iter()
                .map(|reference_id| checker.semantic().reference(*reference_id))
                .filter(|reference| {
                    checker
                        .semantic()
                        .first_non_type_parent_scope_id(reference.scope_id())
                        == Some(scope_id)
                })
            {
                diagnostics.push(Diagnostic::new(
                    BuiltinAttributeShadowing {
                        kind,
                        name: name.to_string(),
                        row: checker.compute_source_row(binding.start()),
                    },
                    reference.range(),
                ));
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Kind {
    Attribute,
    Method,
}
