//! Shared code for [`use_pep695_type_alias`] (UP040) and [`use_pep695_type_parameter`] (UP046)

use std::fmt::Display;

use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Expr, ExprCall, ExprName, ExprSubscript, Identifier, Stmt, StmtAssign, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

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
enum TypeParamKind {
    TypeVar,
    TypeVarTuple,
    ParamSpec,
}

#[derive(Debug)]
struct TypeVar<'a> {
    name: &'a ExprName,
    restriction: Option<TypeVarRestriction<'a>>,
    kind: TypeParamKind,
}

/// Wrapper for formatting a sequence of [`TypeVar`]s for use as a generic type parameter (e.g. `[T,
/// *Ts, **P]`). See [`DisplayTypeVar`] for further details.
struct DisplayTypeVars<'a> {
    type_vars: &'a [TypeVar<'a>],
    source: &'a str,
}

impl Display for DisplayTypeVars<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        let nvars = self.type_vars.len();
        for (i, tv) in self.type_vars.iter().enumerate() {
            write!(f, "{}", tv.display(self.source))?;
            if i < nvars - 1 {
                f.write_str(", ")?;
            }
        }
        f.write_str("]")?;

        Ok(())
    }
}

/// Used for displaying `type_var`. `source` is the whole file, which will be sliced to recover the
/// `TypeVarRestriction` values for generic bounds and constraints.
struct DisplayTypeVar<'a> {
    type_var: &'a TypeVar<'a>,
    source: &'a str,
}

impl TypeVar<'_> {
    fn display<'a>(&'a self, source: &'a str) -> DisplayTypeVar<'a> {
        DisplayTypeVar {
            type_var: self,
            source,
        }
    }
}

impl Display for DisplayTypeVar<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.type_var.kind {
            TypeParamKind::TypeVar => {}
            TypeParamKind::TypeVarTuple => f.write_str("*")?,
            TypeParamKind::ParamSpec => f.write_str("**")?,
        }
        f.write_str(&self.type_var.name.id)?;
        if let Some(restriction) = &self.type_var.restriction {
            f.write_str(": ")?;
            match restriction {
                TypeVarRestriction::Bound(bound) => {
                    f.write_str(&self.source[bound.range()])?;
                }
                TypeVarRestriction::Constraint(vec) => {
                    let len = vec.len();
                    f.write_str("(")?;
                    for (i, v) in vec.iter().enumerate() {
                        f.write_str(&self.source[v.range()])?;
                        if i < len - 1 {
                            f.write_str(", ")?;
                        }
                    }
                    f.write_str(")")?;
                }
            }
        }

        Ok(())
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
            TypeParamKind::TypeVar => {
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
            TypeParamKind::TypeVarTuple => TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
                range: TextRange::default(),
                name: Identifier::new(name.id.clone(), TextRange::default()),
                default: None,
            }),
            TypeParamKind::ParamSpec => TypeParam::ParamSpec(TypeParamParamSpec {
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
                    kind: TypeParamKind::TypeVar,
                });
            }
        }
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            let kind = if semantic.match_typing_expr(func, "TypeVar") {
                TypeParamKind::TypeVar
            } else if semantic.match_typing_expr(func, "TypeVarTuple") {
                TypeParamKind::TypeVarTuple
            } else if semantic.match_typing_expr(func, "ParamSpec") {
                TypeParamKind::ParamSpec
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
