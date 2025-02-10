//! Shared code for [`non_pep695_type_alias`] (UP040),
//! [`non_pep695_generic_class`] (UP046), and [`non_pep695_generic_function`]
//! (UP047)

use std::fmt::Display;

use itertools::Itertools;
use ruff_python_ast::{
    self as ast,
    name::Name,
    visitor::{self, Visitor},
    Arguments, Expr, ExprCall, ExprName, ExprSubscript, Identifier, Stmt, StmtAssign, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

pub(crate) use non_pep695_generic_class::*;
pub(crate) use non_pep695_generic_function::*;
pub(crate) use non_pep695_type_alias::*;
pub(crate) use private_type_parameter::*;

use crate::checkers::ast::Checker;

mod non_pep695_generic_class;
mod non_pep695_generic_function;
mod non_pep695_type_alias;
mod private_type_parameter;

#[derive(Debug)]
pub(crate) enum TypeVarRestriction<'a> {
    /// A type variable with a bound, e.g., `TypeVar("T", bound=int)`.
    Bound(&'a Expr),
    /// A type variable with constraints, e.g., `TypeVar("T", int, str)`.
    Constraint(Vec<&'a Expr>),
    /// `AnyStr` is a special case: the only public `TypeVar` defined in the standard library,
    /// and thus the only one that we recognise when imported from another module.
    AnyStr,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeParamKind {
    TypeVar,
    TypeVarTuple,
    ParamSpec,
}

#[derive(Debug)]
pub(crate) struct TypeVar<'a> {
    pub(crate) name: &'a str,
    pub(crate) restriction: Option<TypeVarRestriction<'a>>,
    pub(crate) kind: TypeParamKind,
    pub(crate) default: Option<&'a Expr>,
}

/// Wrapper for formatting a sequence of [`TypeVar`]s for use as a generic type parameter (e.g. `[T,
/// *Ts, **P]`). See [`DisplayTypeVar`] for further details.
pub(crate) struct DisplayTypeVars<'a> {
    pub(crate) type_vars: &'a [TypeVar<'a>],
    pub(crate) source: &'a str,
}

impl Display for DisplayTypeVars<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nvars = self.type_vars.len();
        if nvars == 0 {
            return Ok(());
        }
        f.write_str("[")?;
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
pub(crate) struct DisplayTypeVar<'a> {
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
        f.write_str(self.type_var.name)?;
        if let Some(restriction) = &self.type_var.restriction {
            f.write_str(": ")?;
            match restriction {
                TypeVarRestriction::Bound(bound) => {
                    f.write_str(&self.source[bound.range()])?;
                }
                TypeVarRestriction::AnyStr => f.write_str("(bytes, str)")?,
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
            default: _, // TODO(brent) see below
        }: &'a TypeVar<'a>,
    ) -> Self {
        match kind {
            TypeParamKind::TypeVar => {
                TypeParam::TypeVar(TypeParamTypeVar {
                    range: TextRange::default(),
                    name: Identifier::new(*name, TextRange::default()),
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
                        Some(TypeVarRestriction::AnyStr) => {
                            Some(Box::new(Expr::Tuple(ast::ExprTuple {
                                range: TextRange::default(),
                                elts: vec![
                                    Expr::Name(ExprName {
                                        range: TextRange::default(),
                                        id: Name::from("str"),
                                        ctx: ast::ExprContext::Load,
                                    }),
                                    Expr::Name(ExprName {
                                        range: TextRange::default(),
                                        id: Name::from("bytes"),
                                        ctx: ast::ExprContext::Load,
                                    }),
                                ],
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
                name: Identifier::new(*name, TextRange::default()),
                default: None,
            }),
            TypeParamKind::ParamSpec => TypeParam::ParamSpec(TypeParamParamSpec {
                range: TextRange::default(),
                name: Identifier::new(*name, TextRange::default()),
                default: None,
            }),
        }
    }
}

impl<'a> From<&'a TypeParam> for TypeVar<'a> {
    fn from(param: &'a TypeParam) -> Self {
        let (kind, restriction) = match param {
            TypeParam::TypeVarTuple(_) => (TypeParamKind::TypeVarTuple, None),
            TypeParam::ParamSpec(_) => (TypeParamKind::ParamSpec, None),

            TypeParam::TypeVar(param) => {
                let restriction = match param.bound.as_deref() {
                    None => None,
                    Some(Expr::Tuple(constraints)) => Some(TypeVarRestriction::Constraint(
                        constraints.elts.iter().collect::<Vec<_>>(),
                    )),
                    Some(bound) => Some(TypeVarRestriction::Bound(bound)),
                };

                (TypeParamKind::TypeVar, restriction)
            }
        };

        Self {
            name: param.name(),
            kind,
            restriction,
            default: param.default(),
        }
    }
}

struct TypeVarReferenceVisitor<'a> {
    vars: Vec<TypeVar<'a>>,
    semantic: &'a SemanticModel<'a>,
    /// Tracks whether any non-TypeVars have been seen to avoid replacing generic parameters when an
    /// unknown `TypeVar` is encountered.
    any_skipped: bool,
}

/// Recursively collects the names of type variable references present in an expression.
impl<'a> Visitor<'a> for TypeVarReferenceVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        // special case for typing.AnyStr, which is a commonly-imported type variable in the
        // standard library with the definition:
        //
        // ```python
        // AnyStr = TypeVar('AnyStr', bytes, str)
        // ```
        //
        // As of 01/2025, this line hasn't been modified in 8 years, so hopefully there won't be
        // much to keep updated here. See
        // https://github.com/python/cpython/blob/383af395af828f40d9543ee0a8fdc5cc011d43db/Lib/typing.py#L2806
        //
        // to replace AnyStr with an annotation like [AnyStr: (bytes, str)], we also have to make
        // sure that `bytes` and `str` have their builtin values and have not been shadowed
        if self.semantic.match_typing_expr(expr, "AnyStr")
            && self.semantic.has_builtin_binding("bytes")
            && self.semantic.has_builtin_binding("str")
        {
            self.vars.push(TypeVar {
                name: "AnyStr",
                restriction: Some(TypeVarRestriction::AnyStr),
                kind: TypeParamKind::TypeVar,
                default: None,
            });
            return;
        }

        match expr {
            Expr::Name(name) if name.ctx.is_load() => {
                if let Some(var) = expr_name_to_type_var(self.semantic, name) {
                    self.vars.push(var);
                } else {
                    self.any_skipped = true;
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

pub(crate) fn expr_name_to_type_var<'a>(
    semantic: &'a SemanticModel,
    name: &'a ExprName,
) -> Option<TypeVar<'a>> {
    let StmtAssign { value, .. } = semantic
        .lookup_symbol(name.id.as_str())
        .and_then(|binding_id| semantic.binding(binding_id).source)
        .map(|node_id| semantic.statement(node_id))?
        .as_assign_stmt()?;

    match value.as_ref() {
        Expr::Subscript(ExprSubscript {
            value: ref subscript_value,
            ..
        }) => {
            if semantic.match_typing_expr(subscript_value, "TypeVar") {
                return Some(TypeVar {
                    name: &name.id,
                    restriction: None,
                    kind: TypeParamKind::TypeVar,
                    default: None,
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
                // TODO(brent) `default` was added in PEP 696 and Python 3.13 but can't be used in
                // generic type parameters before that
                //
                // ```python
                // T = TypeVar("T", default=Any, bound=str)
                // class slice(Generic[T]): ...
                // ```
                //
                // becomes
                //
                // ```python
                // class slice[T: str = Any]: ...
                // ```
                let default = arguments
                    .find_keyword("default")
                    .map(|default| &default.value);
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
                    name: &name.id,
                    restriction,
                    kind,
                    default,
                });
            }
        }
        _ => {}
    }
    None
}

/// Check if the current statement is nested within another [`StmtClassDef`] or [`StmtFunctionDef`].
fn in_nested_context(checker: &Checker) -> bool {
    checker
        .semantic()
        .current_statements()
        .skip(1) // skip the immediate parent, we only call this within a class or function
        .any(|stmt| matches!(stmt, Stmt::ClassDef(_) | Stmt::FunctionDef(_)))
}

/// Deduplicate `vars`, returning `None` if `vars` is empty or any duplicates are found.
fn check_type_vars(vars: Vec<TypeVar<'_>>) -> Option<Vec<TypeVar<'_>>> {
    if vars.is_empty() {
        return None;
    }

    // If any type variables were not unique, just bail out here. this is a runtime error and we
    // can't predict what the user wanted. also bail out if any Python 3.13+ default values are
    // found on the type parameters
    (vars
        .iter()
        .unique_by(|tvar| tvar.name)
        .filter(|tvar| tvar.default.is_none())
        .count()
        == vars.len())
    .then_some(vars)
}

/// Search `class_bases` for a `typing.Generic` base class. Returns the `Generic` expression (if
/// any), along with its index in the class's bases tuple.
pub(crate) fn find_generic<'a>(
    class_bases: &'a Arguments,
    semantic: &SemanticModel,
) -> Option<(usize, &'a ExprSubscript)> {
    class_bases.args.iter().enumerate().find_map(|(idx, expr)| {
        expr.as_subscript_expr().and_then(|sub_expr| {
            semantic
                .match_typing_expr(&sub_expr.value, "Generic")
                .then_some((idx, sub_expr))
        })
    })
}
