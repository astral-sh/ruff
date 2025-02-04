use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Stmt;
use ruff_python_semantic::Binding;

use crate::{
    checkers::ast::Checker,
    renamer::{try_shadowed_kind, Renamer},
};

/// ## What it does
///
/// Checks for use of [PEP 695] type parameters with leading underscores in generic classes and
/// functions.
///
/// ## Why is this bad?
///
/// [PEP 695] type parameters are already restricted in scope to the class or function in which they
/// appear, so leading underscores just hurt readability without the usual privacy benefits.
///
/// ## Example
///
/// ```python
/// class GenericClass[_T]:
///     var: _T
///
///
/// def generic_function[_T](var: _T) -> list[_T]:
///     return var[0]
/// ```
///
/// Use instead:
///
/// ```python
/// class GenericClass[T]:
///     var: T
///
///
/// def generic_function[T](var: T) -> list[T]:
///     return var[0]
/// ```
///
/// ## Fix availability
///
/// This rule avoids shadowing builtins, keywords, and other variables with new type parameter names
/// and will offer a diagnostic but not a fix in the case of a conflict.
///
/// ## See also
///
/// This rule renames private [PEP 695] type parameters but doesn't convert pre-[PEP 695] generics
/// to the new format. See [`non-pep695-generic-function`] and [`non-pep695-generic-class`] for
/// rules that will make this transformation. Those rules do not remove unused type variables after
/// their changes, so you may also want to consider enabling [`unused-private-type-var`] to complete
/// the transition to [PEP 695] generics.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [`non-pep695-generic-function`]: https://docs.astral.sh/ruff/rules/non-pep695-generic-function
/// [`non-pep695-generic-class`]: https://docs.astral.sh/ruff/rules/non-pep695-generic-class
/// [`unused-private-type-var`]: https://docs.astral.sh/ruff/rules/unused-private-type-var
#[derive(ViolationMetadata)]
pub(crate) struct PrivateTypeParameter {
    kind: ParamKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ParamKind {
    Class,
    Function,
}

impl Violation for PrivateTypeParameter {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let kind = match self.kind {
            ParamKind::Class => "class",
            ParamKind::Function => "function",
        };
        format!("Generic {kind} uses private type parameters")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove the leading underscores".to_string())
    }
}

/// UP051
pub(crate) fn private_type_parameter(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let semantic = checker.semantic();
    let stmt = binding.statement(semantic)?;
    if !binding.kind.is_type_param() {
        return None;
    }

    let kind = match stmt {
        Stmt::FunctionDef(_) => ParamKind::Function,
        Stmt::ClassDef(_) => ParamKind::Class,
        _ => return None,
    };

    let old_name = binding.name(checker.source());

    // this covers the sunder `_T_`, dunder `__T__`, and all all-under `_` or `__` cases
    // that should be skipped
    if !old_name.starts_with('_') || old_name.ends_with('_') {
        return None;
    }

    let mut diagnostic = Diagnostic::new(PrivateTypeParameter { kind }, binding.range);

    // if the new name would shadow another variable, keyword, or builtin, emit a diagnostic without
    // a suggested fix
    if try_shadowed_kind(old_name, checker, binding.scope).shadows_any() {
        return Some(diagnostic);
    }

    let new_name = old_name.trim_start_matches('_');

    diagnostic.try_set_fix(|| {
        let (first, rest) = Renamer::rename(
            old_name,
            new_name,
            &semantic.scopes[binding.scope],
            checker.semantic(),
            checker.stylist(),
        )?;

        Ok(Fix::safe_edits(first, rest))
    });

    Some(diagnostic)
}
