use crate::{
    checkers::ast::Checker,
    renamer::{Renamer, ShadowedKind},
};
use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{Binding, ResolvedReferenceId, TypingOnlyBindingsStatus};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::Ranged;

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
/// However, neither a diagnostic nor a fix will be emitted for "sunder" (`_T_`) or "dunder"
/// (`__T__`) type parameter names as these are not considered private.
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
/// If the name without an underscore would shadow a builtin or another variable, would be a
/// keyword, or would otherwise be an invalid identifier, a fix will not be available. In these
/// situations, you can consider using a trailing underscore or a different name entirely to satisfy
/// the lint rule.
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
/// [non-pep695-generic-function]: https://docs.astral.sh/ruff/rules/non-pep695-generic-function
/// [non-pep695-generic-class]: https://docs.astral.sh/ruff/rules/non-pep695-generic-class
/// [unused-private-type-var]: https://docs.astral.sh/ruff/rules/unused-private-type-var
#[derive(ViolationMetadata)]
pub(crate) struct PrivateTypeParameter {
    kind: ParamKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ParamKind {
    Class,
    Function,
}

impl ParamKind {
    const fn as_str(self) -> &'static str {
        match self {
            ParamKind::Class => "class",
            ParamKind::Function => "function",
        }
    }
}

impl Violation for PrivateTypeParameter {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Generic {} uses private type parameters",
            self.kind.as_str()
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Rename type parameter to remove leading underscores".to_string())
    }
}

/// UP049
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

    if !old_name.starts_with('_') {
        return None;
    }

    // Sunder `_T_`, dunder `__T__`, and all all-under `_` or `__` cases should all be skipped, as
    // these are not "private" names
    if old_name.ends_with('_') {
        return None;
    }

    let mut diagnostic = Diagnostic::new(PrivateTypeParameter { kind }, binding.range);

    let new_name = old_name.trim_start_matches('_');

    // if the new name would shadow another variable, keyword, or builtin, emit a diagnostic without
    // a suggested fix
    if ShadowedKind::new(new_name, checker, binding.scope).shadows_any() {
        return Some(diagnostic);
    }

    diagnostic.try_set_optional_fix(|| {
        let source = checker.source();

        // Copied from PYI019
        let reference_is_complex = |id: ResolvedReferenceId| -> bool {
            let reference = semantic.reference(id);
            let in_source = &source[reference.range()];

            in_source != old_name
        };

        if !is_identifier(new_name) {
            return Ok(None);
        }

        let existing_binding_for_new_name = semantic.simulate_runtime_load_at_location_in_scope(
            new_name,
            binding.range,
            binding.scope,
            TypingOnlyBindingsStatus::Disallowed,
        );

        if let Some(binding) = existing_binding_for_new_name.map(|id| semantic.binding(id)) {
            if binding.kind.is_type_param() {
                return Ok(None);
            }
        }

        let (first, rest) = Renamer::rename(
            old_name,
            new_name,
            &semantic.scopes[binding.scope],
            checker.semantic(),
            checker.stylist(),
        )?;

        let applicability = if binding.references().any(reference_is_complex) {
            Applicability::DisplayOnly
        } else {
            Applicability::Safe
        };

        Ok(Some(Fix::applicable_edits(first, rest, applicability)))
    });

    Some(diagnostic)
}
