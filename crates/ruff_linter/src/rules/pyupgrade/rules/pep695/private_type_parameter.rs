use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_semantic::Binding;

use crate::{checkers::ast::Checker, renamer::Renamer};

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
/// ## Known problems
///
/// TODO, none yet
///
/// ## Fix safety
///
/// TODO none yet, likely the usual caveats around comments since line breaks are allowed in the
/// type parameters
///
/// ## Example
///
/// ```python
/// class GenericClass[_T]:
///     var: _T
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
/// def generic_function[T](var: T) -> list[T]:
///     return var[0]
/// ```
///
/// ## See also
///
/// This rule renames private [PEP 695] type parameters but doesn't convert pre-[PEP 695] generics
/// to the new format. See [`non-pep695-generic-function`](non-pep695-generic-function.md) and
/// [`non-pep695-generic-class`](non-pep695-generic-class.md) for rules that will make this
/// transformation. Those rules do not remove unused type variables after their changes, so you may
/// also want to consider enabling [`unused-private-type-var`](unused-private-type-var.md) to
/// complete the transition to [PEP 695] generics.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
#[derive(ViolationMetadata)]
pub(crate) struct PrivateTypeParameter {
    kind: ParamKind,
}

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

    let (kind, range) = match stmt {
        Stmt::FunctionDef(StmtFunctionDef {
            type_params: Some(type_params),
            ..
        }) => (ParamKind::Function, type_params.range),
        Stmt::ClassDef(StmtClassDef {
            type_params: Some(type_params),
            ..
        }) => (ParamKind::Class, type_params.range),
        _ => return None,
    };

    let old_name = binding.name(&checker.source());
    if !old_name.starts_with('_') {
        return None;
    }

    let new_name = old_name.trim_start_matches('_');

    let mut diagnostic = Diagnostic::new(PrivateTypeParameter { kind }, range);

    diagnostic.try_set_fix(|| {
        let (first, rest) = Renamer::rename(
            &old_name,
            new_name,
            &semantic.scopes[binding.scope],
            checker.semantic(),
            checker.stylist(),
        )?;

        Ok(Fix::applicable_edits(
            first,
            rest,
            Applicability::Safe, // TODO unsafe when comments present (at least)
        ))
    });

    Some(diagnostic)
}
