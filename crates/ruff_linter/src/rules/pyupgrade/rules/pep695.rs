//! Shared code for [`use_pep695_type_alias`] (UP040) and [`use_pep695_type_parameter`] (UP046)

use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Expr, ExprCall, ExprName, ExprSubscript, Identifier, Stmt, StmtAssign, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

pub(crate) use use_pep695_type_alias::*;
pub(crate) use use_pep695_type_parameter::*;

mod use_pep695_type_alias;
mod use_pep695_type_parameter;

#[derive(Debug)]
enum TypeVarRestriction<'a> {
    /// A type variable with a bound, e.g., `TypeVar("T", bound=int)`.
    Bound(&'a Expr),
    /// A type variable with constraints, e.g., `TypeVar("T", int, str)`.
    Constraint(Vec<&'a Expr>),
}

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TypeVarKind {
    Var,
    Tuple,
    ParamSpec,
}

#[derive(Debug)]
struct TypeVar<'a> {
    name: &'a ExprName,
    restriction: Option<TypeVarRestriction<'a>>,
    kind: TypeVarKind,
}

/// Format a sequence of [`TypeVar`]s for use as a generic type parameter (e.g. `[T, *Ts, **P]`).
/// See [`TypeVar::fmt_into`] for further details.
fn fmt_type_vars(type_vars: &[TypeVar], checker: &Checker) -> String {
    let nvars = type_vars.len();
    let mut type_params = String::from("[");
    for (i, tv) in type_vars.iter().enumerate() {
        tv.fmt_into(&mut type_params, checker.source());
        if i < nvars - 1 {
            type_params.push_str(", ");
        }
    }
    type_params.push(']');

    type_params
}

impl TypeVar<'_> {
    /// Format `self` into `s`, where `source` is the whole file, which will be sliced to recover
    /// the `TypeVarRestriction` values for generic bounds and constraints.
    fn fmt_into(&self, s: &mut String, source: &str) {
        match self.kind {
            TypeVarKind::Var => {}
            TypeVarKind::Tuple => s.push('*'),
            TypeVarKind::ParamSpec => s.push_str("**"),
        }
        s.push_str(&self.name.id);
        if let Some(restriction) = &self.restriction {
            s.push_str(": ");
            match restriction {
                TypeVarRestriction::Bound(bound) => {
                    s.push_str(&source[bound.range()]);
                }
                TypeVarRestriction::Constraint(vec) => {
                    let len = vec.len();
                    s.push('(');
                    for (i, v) in vec.iter().enumerate() {
                        s.push_str(&source[v.range()]);
                        if i < len - 1 {
                            s.push_str(", ");
                        }
                    }
                    s.push(')');
                }
            }
        }
    }
}

impl<'a> From<&'a TypeVar<'a>> for TypeParam {
    fn from(
        TypeVar {
            name,
            restriction,
            kind,
        }: &'a TypeVar<'a>,
    ) -> Self {
        match kind {
            TypeVarKind::Var => {
                TypeParam::TypeVar(TypeParamTypeVar {
                    range: TextRange::default(),
                    name: Identifier::new(name.id.clone(), TextRange::default()),
                    bound: match restriction {
                        Some(TypeVarRestriction::Bound(bound)) => Some(Box::new((*bound).clone())),
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
            }
            TypeVarKind::Tuple => TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
                range: TextRange::default(),
                name: Identifier::new(name.id.clone(), TextRange::default()),
                default: None,
            }),
            TypeVarKind::ParamSpec => TypeParam::ParamSpec(TypeParamParamSpec {
                range: TextRange::default(),
                name: Identifier::new(name.id.clone(), TextRange::default()),
                default: None,
            }),
        }
    }
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
                    kind: TypeVarKind::Var,
                });
            }
        }
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            let kind = if semantic.match_typing_expr(func, "TypeVar") {
                TypeVarKind::Var
            } else if semantic.match_typing_expr(func, "TypeVarTuple") {
                TypeVarKind::Tuple
            } else if semantic.match_typing_expr(func, "ParamSpec") {
                TypeVarKind::ParamSpec
            } else {
                return None;
            };

            if arguments
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

                return Some(TypeVar {
                    name,
                    restriction,
                    kind,
                });
            }
        }
        _ => {}
    }
    None
}
