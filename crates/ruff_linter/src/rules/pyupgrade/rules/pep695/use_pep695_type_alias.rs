use itertools::Itertools;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast, visitor::Visitor, Expr, ExprCall, ExprName, Keyword, Stmt, StmtAnnAssign,
    StmtAssign, StmtTypeAlias, TypeParam,
};
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

use super::{expr_name_to_type_var, TypeVar, TypeVarKind, TypeVarReferenceVisitor};

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
/// `covariant` and `contravariant` keywords used by `TypeParam` variables. As
/// such, rewriting a `TypeParam` variable to a `type` alias may change its
/// variance.
///
/// Unlike `TypeParam` variables, [PEP 695]-style `type` aliases cannot be used
/// at runtime. For example, calling `isinstance` on a `type` alias will throw
/// a `TypeError`. As such, rewriting a `TypeParam` via the `type` keyword will
/// cause issues for parameters that are used for such runtime checks.
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
/// [PEP 695]: https://peps.python.org/pep-0695/
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
        match type_alias_kind {
            TypeAliasKind::TypeAlias | TypeAliasKind::TypeAliasType => format!(
                "Type alias `{name}` uses {type_alias_method} instead of the `type` keyword"
            ),
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.type_alias_kind {
            TypeAliasKind::TypeAlias | TypeAliasKind::TypeAliasType => {
                Some("Use the `type` keyword".to_string())
            }
        }
    }
}

/// UP040
pub(crate) fn non_pep695_type_alias_type(checker: &mut Checker, stmt: &StmtAssign) {
    if checker.settings.target_version < PythonVersion::Py312 {
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
                    name,
                    restriction: None,
                    kind: TypeVarKind::Var,
                })
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    checker.diagnostics.push(create_diagnostic(
        checker.generator(),
        stmt.range(),
        target_name.id.clone(),
        value,
        &vars,
        Applicability::Safe,
        TypeAliasKind::TypeAliasType,
    ));
}

/// UP040
pub(crate) fn non_pep695_type_alias(checker: &mut Checker, stmt: &StmtAnnAssign) {
    if checker.settings.target_version < PythonVersion::Py312 {
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

    // TODO(zanie): We should check for generic type variables used in the value and define them
    //              as type params instead
    let vars = {
        let mut visitor = TypeVarReferenceVisitor {
            vars: vec![],
            semantic: checker.semantic(),
        };
        visitor.visit_expr(value);
        visitor.vars
    };

    // Type variables must be unique; filter while preserving order.
    let vars = vars
        .into_iter()
        .unique_by(|TypeVar { name, .. }| name.id.as_str())
        .collect::<Vec<_>>();

    checker.diagnostics.push(create_diagnostic(
        checker.generator(),
        stmt.range(),
        name.clone(),
        value,
        &vars,
        // The fix is only safe in a type stub because new-style aliases have different runtime behavior
        // See https://github.com/astral-sh/ruff/issues/6434
        if checker.source_type.is_stub() {
            Applicability::Safe
        } else {
            Applicability::Unsafe
        },
        TypeAliasKind::TypeAlias,
    ));
}

/// Generate a [`Diagnostic`] for a non-PEP 695 type alias or type alias type.
fn create_diagnostic(
    generator: Generator,
    stmt_range: TextRange,
    name: Name,
    value: &Expr,
    vars: &[TypeVar],
    applicability: Applicability,
    type_alias_kind: TypeAliasKind,
) -> Diagnostic {
    Diagnostic::new(
        NonPEP695TypeAlias {
            name: name.to_string(),
            type_alias_kind,
        },
        stmt_range,
    )
    .with_fix(Fix::applicable_edit(
        Edit::range_replacement(
            generator.stmt(&Stmt::from(StmtTypeAlias {
                range: TextRange::default(),
                name: Box::new(Expr::Name(ExprName {
                    range: TextRange::default(),
                    id: name,
                    ctx: ast::ExprContext::Load,
                })),
                type_params: create_type_params(vars),
                value: Box::new(value.clone()),
            })),
            stmt_range,
        ),
        applicability,
    ))
}

fn create_type_params(vars: &[TypeVar]) -> Option<ruff_python_ast::TypeParams> {
    if vars.is_empty() {
        return None;
    }

    Some(ast::TypeParams {
        range: TextRange::default(),
        type_params: vars.iter().map(TypeParam::from).collect(),
    })
}
