use itertools::Itertools;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Expr, ExprCall, ExprName, ExprSubscript, Identifier, Keyword, Stmt, StmtAnnAssign, StmtAssign,
    StmtTypeAlias, TypeParam, TypeParamTypeVar,
};
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

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
#[violation]
pub struct NonPEP695TypeAlias {
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
    let type_params = if vars.is_empty() {
        None
    } else {
        Some(ast::TypeParams {
            range: TextRange::default(),
            type_params: vars
                .iter()
                .map(|TypeVar { name, restriction }| {
                    TypeParam::TypeVar(TypeParamTypeVar {
                        range: TextRange::default(),
                        name: Identifier::new(name.id.clone(), TextRange::default()),
                        bound: match restriction {
                            Some(TypeVarRestriction::Bound(bound)) => {
                                Some(Box::new((*bound).clone()))
                            }
                            Some(TypeVarRestriction::Constraint(constraints)) => {
                                Some(Box::new(Expr::Tuple(ast::ExprTuple {
                                    range: TextRange::default(),
                                    elts: constraints.iter().map(|expr| (*expr).clone()).collect(),
                                    ctx: ast::ExprContext::Load,
                                    parenthesized: true,
                                })))
                            }
                            None => None,
                        },
                        // We don't handle defaults here yet. Should perhaps be a different rule since
                        // defaults are only valid in 3.13+.
                        default: None,
                    })
                })
                .collect(),
        })
    };

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
                type_params,
                value: Box::new(value.clone()),
            })),
            stmt_range,
        ),
        applicability,
    ))
}

#[derive(Debug)]
enum TypeVarRestriction<'a> {
    /// A type variable with a bound, e.g., `TypeVar("T", bound=int)`.
    Bound(&'a Expr),
    /// A type variable with constraints, e.g., `TypeVar("T", int, str)`.
    Constraint(Vec<&'a Expr>),
}

#[derive(Debug)]
struct TypeVar<'a> {
    name: &'a ExprName,
    restriction: Option<TypeVarRestriction<'a>>,
}

struct TypeVarReferenceVisitor<'a> {
    vars: Vec<TypeVar<'a>>,
    semantic: &'a SemanticModel<'a>,
}

/// Recursively collects the names of type variable references present in an expression.
impl<'a> Visitor<'a> for TypeVarReferenceVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) if name.ctx.is_load() => {
                self.vars.extend(expr_name_to_type_var(self.semantic, name));
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn expr_name_to_type_var<'a>(
    semantic: &'a SemanticModel,
    name: &'a ExprName,
) -> Option<TypeVar<'a>> {
    let Some(Stmt::Assign(StmtAssign { value, .. })) = semantic
        .lookup_symbol(name.id.as_str())
        .and_then(|binding_id| {
            semantic
                .binding(binding_id)
                .source
                .map(|node_id| semantic.statement(node_id))
        })
    else {
        return None;
    };

    match value.as_ref() {
        Expr::Subscript(ExprSubscript {
            value: ref subscript_value,
            ..
        }) => {
            if semantic.match_typing_expr(subscript_value, "TypeVar") {
                return Some(TypeVar {
                    name,
                    restriction: None,
                });
            }
        }
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            if semantic.match_typing_expr(func, "TypeVar")
                && arguments
                    .args
                    .first()
                    .is_some_and(Expr::is_string_literal_expr)
            {
                let restriction = if let Some(bound) = arguments.find_keyword("bound") {
                    Some(TypeVarRestriction::Bound(&bound.value))
                } else if arguments.args.len() > 1 {
                    Some(TypeVarRestriction::Constraint(
                        arguments.args.iter().skip(1).collect(),
                    ))
                } else {
                    None
                };

                return Some(TypeVar { name, restriction });
            }
        }
        _ => {}
    }
    None
}
