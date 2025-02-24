use rustc_hash::FxHashSet;
use std::iter;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, Expr, ExprStarred, ExprSubscript, ExprTuple, StmtClassDef, TypeParams,
};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::pyupgrade::rules::pep695::{
    expr_name_to_type_var, find_generic, DisplayTypeVars, TypeParamKind, TypeVar,
};
use ruff_python_ast::PythonVersion;

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
/// Additionally, comments within the fix range will not be preserved.
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

/// RUF053
pub(crate) fn class_with_mixed_type_vars(checker: &Checker, class_def: &StmtClassDef) {
    if checker.target_version() < PythonVersion::PY312 {
        return;
    }

    let StmtClassDef {
        type_params,
        arguments,
        ..
    } = class_def;

    let Some(type_params) = type_params else {
        return;
    };

    let Some(arguments) = arguments else {
        return;
    };

    let Some((generic_base, old_style_type_vars)) =
        typing_generic_base_and_arguments(arguments, checker.semantic())
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(ClassWithMixedTypeVars, generic_base.range);

    diagnostic.try_set_optional_fix(|| {
        convert_type_vars(
            generic_base,
            old_style_type_vars,
            type_params,
            arguments,
            checker,
        )
    });

    checker.report_diagnostic(diagnostic);
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
) -> anyhow::Result<Option<Fix>> {
    let mut type_vars: Vec<_> = type_params.type_params.iter().map(TypeVar::from).collect();

    let semantic = checker.semantic();
    let converted_type_vars = match old_style_type_vars {
        Expr::Tuple(ExprTuple { elts, .. }) => {
            generic_arguments_to_type_vars(elts.iter(), type_params, semantic)
        }
        expr @ (Expr::Subscript(_) | Expr::Name(_)) => {
            generic_arguments_to_type_vars(iter::once(expr), type_params, semantic)
        }
        _ => None,
    };

    let Some(converted_type_vars) = converted_type_vars else {
        return Ok(None);
    };

    type_vars.extend(converted_type_vars);

    let source = checker.source();
    let new_type_params = DisplayTypeVars {
        type_vars: &type_vars,
        source,
    };

    let remove_generic_base =
        remove_argument(generic_base, class_arguments, Parentheses::Remove, source)?;
    let replace_type_params =
        Edit::range_replacement(new_type_params.to_string(), type_params.range);

    Ok(Some(Fix::unsafe_edits(
        remove_generic_base,
        [replace_type_params],
    )))
}

/// Returns the type variables `exprs` represent.
///
/// If at least one of them cannot be converted to [`TypeVar`],
/// `None` is returned.
fn generic_arguments_to_type_vars<'a>(
    exprs: impl Iterator<Item = &'a Expr>,
    existing_type_params: &TypeParams,
    semantic: &'a SemanticModel,
) -> Option<Vec<TypeVar<'a>>> {
    let mut type_vars = vec![];
    let mut encountered: FxHashSet<&str> = existing_type_params
        .iter()
        .map(|tp| tp.name().as_str())
        .collect();

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

        if !encountered.insert(name.id.as_str()) {
            continue;
        }

        let type_var = expr_name_to_type_var(semantic, name)?;

        if !type_var_is_valid(&type_var, unpacked) {
            return None;
        }

        // TODO: Type parameter defaults
        if type_var.default.is_some() {
            return None;
        }

        type_vars.push(type_var);
    }

    Some(type_vars)
}

/// Returns true in the following cases:
///
/// * If `type_var` is a `TypeVar`:
///     * It must not be unpacked
/// * If `type_var` is a `TypeVarTuple`:
///     * It must be unpacked
///     * It must not have any restrictions
/// * If `type_var` is a `ParamSpec`:
///     * It must not be unpacked
///     * It must not have any restrictions
fn type_var_is_valid(type_var: &TypeVar, unpacked: bool) -> bool {
    let is_type_var_tuple = matches!(&type_var.kind, TypeParamKind::TypeVarTuple);

    if is_type_var_tuple && !unpacked || !is_type_var_tuple && unpacked {
        return false;
    }

    if !matches!(&type_var.kind, TypeParamKind::TypeVar) && type_var.restriction.is_some() {
        return false;
    }

    true
}
