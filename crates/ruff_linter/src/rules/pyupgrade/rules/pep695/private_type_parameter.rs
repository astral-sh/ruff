use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_semantic::Binding;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::Ranged;

use crate::{Applicability, Fix, FixAvailability, Violation};
use crate::{
    checkers::ast::Checker,
    renamer::{Renamer, ShadowedKind},
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
/// to the new format. See [`non-pep695-generic-function`][UP047] and
/// [`non-pep695-generic-class`][UP046] for rules that will make this transformation.
/// Those rules do not remove unused type variables after their changes,
/// so you may also want to consider enabling [`unused-private-type-var`][PYI018] to complete
/// the transition to [PEP 695] generics.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [UP047]: https://docs.astral.sh/ruff/rules/non-pep695-generic-function
/// [UP046]: https://docs.astral.sh/ruff/rules/non-pep695-generic-class
/// [PYI018]: https://docs.astral.sh/ruff/rules/unused-private-type-var
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.12.0")]
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
pub(crate) fn private_type_parameter(checker: &Checker, binding: &Binding) {
    let semantic = checker.semantic();
    let Some(stmt) = binding.statement(semantic) else {
        return;
    };
    if !binding.kind.is_type_param() {
        return;
    }

    let kind = match stmt {
        Stmt::FunctionDef(_) => ParamKind::Function,
        Stmt::ClassDef(_) => ParamKind::Class,
        _ => return,
    };

    let old_name = binding.name(checker.source());

    if !old_name.starts_with('_') {
        return;
    }

    // Sunder `_T_`, dunder `__T__`, and all all-under `_` or `__` cases should all be skipped, as
    // these are not "private" names
    if old_name.ends_with('_') {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(PrivateTypeParameter { kind }, binding.range);

    let new_name = old_name.trim_start_matches('_');

    // if the new name would shadow another variable, keyword, or builtin, emit a diagnostic without
    // a suggested fix
    if ShadowedKind::new(binding, new_name, checker).shadows_any() {
        return;
    }

    if !is_identifier(new_name) {
        return;
    }

    let source = checker.source();

    diagnostic.try_set_fix(|| {
        let (first, rest) = Renamer::rename(
            old_name,
            new_name,
            &semantic.scopes[binding.scope],
            semantic,
            checker.stylist(),
        )?;

        let applicability = if binding
            .references()
            .any(|id| &source[semantic.reference(id).range()] != old_name)
        {
            Applicability::DisplayOnly
        } else {
            Applicability::Safe
        };

        let fix_isolation = Checker::isolation(binding.source);
        Ok(Fix::applicable_edits(first, rest, applicability).isolate(fix_isolation))
    });
}
