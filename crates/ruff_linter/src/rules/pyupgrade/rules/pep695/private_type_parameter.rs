use anyhow::bail;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
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

impl AlwaysFixableViolation for PrivateTypeParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let kind = match self.kind {
            ParamKind::Class => "class",
            ParamKind::Function => "function",
        };
        format!("Generic {kind} uses private type parameters")
    }

    fn fix_title(&self) -> String {
        "Remove the leading underscores".to_string()
    }
}

/// UP051
pub(crate) fn private_type_parameter(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let semantic = checker.semantic();
    let stmt = binding.statement(semantic)?;
    if !binding.kind.is_type_param() {
        return None;
    }

    let (kind, type_params) = match stmt {
        Stmt::FunctionDef(StmtFunctionDef {
            type_params: Some(type_params),
            ..
        }) => (ParamKind::Function, type_params),
        Stmt::ClassDef(StmtClassDef {
            type_params: Some(type_params),
            ..
        }) => (ParamKind::Class, type_params),
        _ => return None,
    };

    // this rule is a bit of a hack. we're in a binding-based rule, but we only use the binding to
    // obtain the scope for the rename and then loop over all of the type parameters anyway instead
    // of limiting the fix to the single binding that initially triggered the rule. the upside of
    // this is that we can give a single diagnostic for all of the affected type parameters instead
    // of one diagnostic per parameter. additionally, this avoids issues with the `Binding::name`
    // method, which returns `_T: str` for a type parameter with a bound, for example.
    let to_rename: Vec<_> = type_params
        .iter()
        .flat_map(|tp| {
            let name = tp.name().as_str();
            // this covers the sunder `_T_`, dunder `__T__`, and all all-under `_` or `__` cases
            // that should be skipped
            if name.starts_with('_') && !name.ends_with('_') {
                let new_name = name.trim_start_matches('_');
                if semantic.is_available(new_name) {
                    Some((name, new_name))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if to_rename.is_empty() {
        return None;
    }

    // TODO this could be a good place for multi-range diagnostics to mark only the affected params
    let mut diagnostic = Diagnostic::new(PrivateTypeParameter { kind }, type_params.range);

    diagnostic.try_set_fix(|| {
        let mut edits = Vec::new();
        for (old_name, new_name) in to_rename {
            let rest = Renamer::rename(
                dbg!(old_name),
                dbg!(new_name),
                &semantic.scopes[binding.scope],
                checker.semantic(),
                checker.stylist(),
            );
            edits.extend(rest);
        }

        if edits.is_empty() {
            bail!("No variables found to rename");
        }

        Ok(Fix::safe_edits(edits.swap_remove(0), edits))
    });

    Some(diagnostic)
}
