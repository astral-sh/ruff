use itertools::Itertools;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::Name;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, ExprCall, ExprName, Keyword, StmtAnnAssign, StmtAssign, StmtRef};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

use super::{
    expr_name_to_type_var, DisplayTypeVars, TypeParamKind, TypeVar, TypeVarReferenceVisitor,
};

/// ## What it does
/// Checks for use of `TypeAlias` annotations and `TypeAliasType` assignments
/// for declaring type aliases.
///
/// ## Why is this bad?
/// The `type` keyword was introduced in Python 3.12 by [PEP 695] for defining
/// type aliases. The `type` keyword is easier to read and provides cleaner
/// support for generics.
///
/// ## Known problems
/// [PEP 695] uses inferred variance for type parameters, instead of the
/// `covariant` and `contravariant` keywords used by `TypeVar` variables. As
/// such, rewriting a type alias using a PEP-695 `type` statement may change
/// the variance of the alias's type parameters.
///
/// Unlike type aliases that use simple assignments, definitions created using
/// [PEP 695] `type` statements cannot be used as drop-in replacements at
/// runtime for the value on the right-hand side of the statement. This means
/// that while for some simple old-style type aliases you can use them as the
/// second argument to an `isinstance()` call (for example), doing the same
/// with a [PEP 695] `type` statement will always raise `TypeError` at
/// runtime.
///
/// ## Example
/// ```python
/// ListOfInt: TypeAlias = list[int]
/// PositiveInt = TypeAliasType("PositiveInt", Annotated[int, Gt(0)])
/// ```
///
/// Use instead:
/// ```python
/// type ListOfInt = list[int]
/// type PositiveInt = Annotated[int, Gt(0)]
/// ```
///
/// ## Fix safety
///
/// This fix is marked unsafe for `TypeAlias` assignments outside of stub files because of the
/// runtime behavior around `isinstance()` calls noted above. The fix is also unsafe for
/// `TypeAliasType` assignments if there are any comments in the replacement range that would be
/// deleted.
///
/// ## See also
///
/// This rule only applies to `TypeAlias`es and `TypeAliasType`s. See
/// [`non-pep695-generic-class`][UP046] and [`non-pep695-generic-function`][UP047] for similar
/// transformations for generic classes and functions.
///
/// This rule replaces standalone type variables in aliases but doesn't remove the corresponding
/// type variables even if they are unused after the fix. See [`unused-private-type-var`][PYI018]
/// for a rule to clean up unused private type variables.
///
/// This rule will not rename private type variables to remove leading underscores, even though the
/// new type parameters are restricted in scope to their associated aliases. See
/// [`private-type-parameter`][UP049] for a rule to update these names.
///
/// [PEP 695]: https://peps.python.org/pep-0695/
/// [PYI018]: https://docs.astral.sh/ruff/rules/unused-private-type-var/
/// [UP046]: https://docs.astral.sh/ruff/rules/non-pep695-generic-class/
/// [UP047]: https://docs.astral.sh/ruff/rules/non-pep695-generic-function/
/// [UP049]: https://docs.astral.sh/ruff/rules/private-type-parameter/
#[derive(ViolationMetadata)]
pub(crate) struct NonPEP695TypeAlias {
    name: String,
    type_alias_kind: TypeAliasKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TypeAliasKind {
    TypeAlias,
    TypeAliasType,
}

impl Violation for NonPEP695TypeAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP695TypeAlias {
            name,
            type_alias_kind,
        } = self;
        let type_alias_method = match type_alias_kind {
            TypeAliasKind::TypeAlias => "`TypeAlias` annotation",
            TypeAliasKind::TypeAliasType => "`TypeAliasType` assignment",
        };
        format!("Type alias `{name}` uses {type_alias_method} instead of the `type` keyword")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use the `type` keyword".to_string())
    }
}

/// UP040
pub(crate) fn non_pep695_type_alias_type(checker: &Checker, stmt: &StmtAssign) {
    if checker.target_version() < PythonVersion::PY312 {
        return;
    }

    let StmtAssign { targets, value, .. } = stmt;

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    let [Expr::Name(target_name)] = targets.as_slice() else {
        return;
    };

    let [Expr::StringLiteral(name), value] = arguments.args.as_ref() else {
        return;
    };

    if &name.value != target_name.id.as_str() {
        return;
    }

    let type_params = match arguments.keywords.as_ref() {
        [] => &[],
        [Keyword {
            arg: Some(name),
            value: Expr::Tuple(type_params),
            ..
        }] if name.as_str() == "type_params" => type_params.elts.as_slice(),
        _ => return,
    };

    if !checker
        .semantic()
        .match_typing_expr(func.as_ref(), "TypeAliasType")
    {
        return;
    }

    let Some(vars) = type_params
        .iter()
        .map(|expr| {
            expr.as_name_expr().map(|name| {
                expr_name_to_type_var(checker.semantic(), name).unwrap_or(TypeVar {
                    name: &name.id,
                    restriction: None,
                    kind: TypeParamKind::TypeVar,
                    default: None,
                })
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    checker.report_diagnostic(create_diagnostic(
        checker,
        stmt.into(),
        &target_name.id,
        value,
        &vars,
        TypeAliasKind::TypeAliasType,
    ));
}

/// UP040
pub(crate) fn non_pep695_type_alias(checker: &Checker, stmt: &StmtAnnAssign) {
    if checker.target_version() < PythonVersion::PY312 {
        return;
    }

    let StmtAnnAssign {
        target,
        annotation,
        value,
        ..
    } = stmt;

    if !checker
        .semantic()
        .match_typing_expr(annotation, "TypeAlias")
    {
        return;
    }

    let Expr::Name(ExprName { id: name, .. }) = target.as_ref() else {
        return;
    };

    let Some(value) = value else {
        return;
    };

    let vars = {
        let mut visitor = TypeVarReferenceVisitor {
            vars: vec![],
            semantic: checker.semantic(),
            any_skipped: false,
        };
        visitor.visit_expr(value);
        visitor.vars
    };

    // Type variables must be unique; filter while preserving order.
    let vars = vars
        .into_iter()
        .unique_by(|tvar| tvar.name)
        .collect::<Vec<_>>();

    // TODO(brent) handle `default` arg for Python 3.13+
    if vars.iter().any(|tv| tv.default.is_some()) {
        return;
    }

    checker.report_diagnostic(create_diagnostic(
        checker,
        stmt.into(),
        name,
        value,
        &vars,
        TypeAliasKind::TypeAlias,
    ));
}

/// Generate a [`Diagnostic`] for a non-PEP 695 type alias or type alias type.
fn create_diagnostic(
    checker: &Checker,
    stmt: StmtRef,
    name: &Name,
    value: &Expr,
    type_vars: &[TypeVar],
    type_alias_kind: TypeAliasKind,
) -> Diagnostic {
    let source = checker.source();
    let comment_ranges = checker.comment_ranges();

    let range_with_parentheses =
        parenthesized_range(value.into(), stmt.into(), comment_ranges, source)
            .unwrap_or(value.range());

    let content = format!(
        "type {name}{type_params} = {value}",
        type_params = DisplayTypeVars { type_vars, source },
        value = &source[range_with_parentheses]
    );
    let edit = Edit::range_replacement(content, stmt.range());

    let applicability =
        if type_alias_kind == TypeAliasKind::TypeAlias && !checker.source_type.is_stub() {
            // The fix is always unsafe in non-stubs
            // because new-style aliases have different runtime behavior.
            // See https://github.com/astral-sh/ruff/issues/6434
            Applicability::Unsafe
        } else {
            // In stub files, or in non-stub files for `TypeAliasType` assignments,
            // the fix is only unsafe if it would delete comments.
            //
            // it would be easier to check for comments in the whole `stmt.range`, but because
            // `create_diagnostic` uses the full source text of `value`, comments within `value` are
            // actually preserved. thus, we have to check for comments in `stmt` but outside of `value`
            let pre_value = TextRange::new(stmt.start(), range_with_parentheses.start());
            let post_value = TextRange::new(range_with_parentheses.end(), stmt.end());

            if comment_ranges.intersects(pre_value) || comment_ranges.intersects(post_value) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            }
        };

    Diagnostic::new(
        NonPEP695TypeAlias {
            name: name.to_string(),
            type_alias_kind,
        },
        stmt.range(),
    )
    .with_fix(Fix::applicable_edit(edit, applicability))
}
