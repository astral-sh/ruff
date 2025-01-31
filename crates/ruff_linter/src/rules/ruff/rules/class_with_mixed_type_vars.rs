use std::iter;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, Expr, ExprStarred, ExprSubscript, ExprTuple, StmtClassDef, TypeParams,
};
use ruff_python_semantic::{Binding, BindingKind, SemanticModel};

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::pyupgrade::rules::pep695::{
    expr_name_to_type_var, find_generic, DisplayTypeVars, TypeParamKind, TypeVar,
};
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for classes that have [PEP 695] [type parameter lists]
/// while also inheriting from `typing.Generic` or `typing_extensions.Generic`.
///
/// ## Why is this bad?
/// Such classes cause errors at runtime:
///
/// ```python
/// from typing import Generic, TypeVar
///
/// U = TypeVar("U")
///
/// # TypeError: Cannot inherit from Generic[...] multiple times.
/// class C[T](Generic[U]): ...
/// ```
///
/// ## Example
///
/// ```python
/// from typing import Generic, ParamSpec, TypeVar, TypeVarTuple
///
/// U = TypeVar("U")
/// P = ParamSpec("P")
/// Ts = TypeVarTuple("Ts")
///
///
/// class C[T](Generic[U, P, *Ts]): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class C[T, U, **P, *Ts]: ...
/// ```
///
/// ## Fix safety
/// As the fix changes runtime behaviour, it is always marked as unsafe.
/// Additionally, it might remove comments.
///
/// ## References
/// - [Python documentation: User-defined generic types](https://docs.python.org/3/library/typing.html#user-defined-generic-types)
/// - [Python documentation: type parameter lists](https://docs.python.org/3/reference/compound_stmts.html#type-params)
/// - [PEP 695 - Type Parameter Syntax](https://peps.python.org/pep-0695/)
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [type parameter lists]: https://docs.python.org/3/reference/compound_stmts.html#type-params
#[derive(ViolationMetadata)]
pub(crate) struct ClassWithMixedTypeVars;

impl Violation for ClassWithMixedTypeVars {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Class with type parameter list inherits from `Generic`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `Generic` base class".to_string())
    }
}

/// RUF060
pub(crate) fn class_with_mixed_type_vars(checker: &mut Checker, class_def: &StmtClassDef) {
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    let semantic = checker.semantic();
    let StmtClassDef {
        type_params,
        arguments,
        ..
    } = class_def;

    let Some(type_params) = type_params.as_deref() else {
        return;
    };

    let Some(arguments) = arguments else {
        return;
    };

    let Some((generic_base, old_style_type_vars)) =
        typing_generic_base_and_arguments(arguments, semantic)
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(ClassWithMixedTypeVars, generic_base.range);

    if let Some(fix) = convert_type_vars(
        generic_base,
        old_style_type_vars,
        type_params,
        arguments,
        checker,
    ) {
        diagnostic.set_fix(fix);
    }

    checker.diagnostics.push(diagnostic);
}

fn typing_generic_base_and_arguments<'a>(
    class_arguments: &'a Arguments,
    semantic: &SemanticModel,
) -> Option<(&'a ExprSubscript, &'a Expr)> {
    let (_, base @ ExprSubscript { slice, .. }) = find_generic(class_arguments, semantic)?;

    Some((base, slice.as_ref()))
}

fn convert_type_vars(
    generic_base: &ExprSubscript,
    old_style_type_vars: &Expr,
    type_params: &TypeParams,
    class_arguments: &Arguments,
    checker: &Checker,
) -> Option<Fix> {
    let mut type_vars = type_params
        .type_params
        .iter()
        .map(TypeVar::from)
        .collect::<Vec<_>>();

    let mut converted_type_vars = match old_style_type_vars {
        Expr::Tuple(ExprTuple { elts, .. }) => {
            generic_arguments_to_type_vars(elts.iter(), type_params, checker)?
        }
        expr @ (Expr::Subscript(_) | Expr::Name(_)) => {
            generic_arguments_to_type_vars(iter::once(expr), type_params, checker)?
        }
        _ => return None,
    };

    type_vars.append(&mut converted_type_vars);

    let source = checker.source();
    let new_type_params = DisplayTypeVars {
        type_vars: &type_vars,
        source,
    };

    let remove_generic_base =
        remove_argument(generic_base, class_arguments, Parentheses::Remove, source).ok()?;
    let replace_type_params =
        Edit::range_replacement(new_type_params.to_string(), type_params.range);

    Some(Fix::unsafe_edits(
        remove_generic_base,
        [replace_type_params],
    ))
}

fn generic_arguments_to_type_vars<'a>(
    exprs: impl Iterator<Item = &'a Expr>,
    existing_type_params: &TypeParams,
    checker: &'a Checker,
) -> Option<Vec<TypeVar<'a>>> {
    let is_existing_param_of_same_class = |binding: &Binding| {
        // This first check should have been unnecessary,
        // as a type parameter list can only contain type-parameter bindings.
        // Named expressions, for example, are syntax errors.
        // However, Ruff doesn't know that yet (#11118),
        // so here it shall remain.
        matches!(binding.kind, BindingKind::TypeParam)
            && existing_type_params.range.contains_range(binding.range)
    };

    let semantic = checker.semantic();

    let mut type_vars = vec![];
    let mut encountered: Vec<&str> = vec![];

    for expr in exprs {
        let (name, unpacked) = match expr {
            Expr::Name(name) => (name, false),
            Expr::Starred(ExprStarred { value, .. }) => (value.as_name_expr()?, true),

            Expr::Subscript(ExprSubscript { value, slice, .. }) => {
                if !semantic.match_typing_expr(value, "Unpack") {
                    return None;
                }

                (slice.as_name_expr()?, true)
            }

            _ => return None,
        };

        let binding = semantic.only_binding(name).map(|id| semantic.binding(id))?;
        let name_as_str = name.id.as_str();

        if is_existing_param_of_same_class(binding) || encountered.contains(&name_as_str) {
            continue;
        }

        encountered.push(name_as_str);

        let type_var = expr_name_to_type_var(semantic, name)?;

        match (&type_var.kind, unpacked, &type_var.restriction) {
            (TypeParamKind::TypeVarTuple, false, _) => return None,
            (TypeParamKind::TypeVar, true, _) => return None,
            (TypeParamKind::ParamSpec, true, _) => return None,

            (TypeParamKind::TypeVarTuple, _, Some(_)) => return None,
            (TypeParamKind::ParamSpec, _, Some(_)) => return None,

            _ => {}
        }

        // TODO: Type parameter defaults
        if type_var.default.is_some() {
            return None;
        }

        type_vars.push(type_var);
    }

    Some(type_vars)
}
