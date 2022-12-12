//! Intermediate types for `with` statement cover grammar.
//!
//! When we start parsing a `with` statement, we don't initially know
//! whether we're looking at a tuple or a Python 3.9+ parenthesized
//! collection of contexts:
//!
//! ```python
//! with (a, b, c) as t:  # tuple
//! with (a, b, c):  # withitems
//! ```
//!
//! Since LALRPOP requires us to commit to an output type before we
//! have enough information to decide, we build a cover grammar that's
//! convertible either way.  This module contains the necessary
//! intermediate data types.

use crate::ast::{self, Location};
use crate::error::{LexicalError, LexicalErrorType};
use crate::token::Tok;
use lalrpop_util::ParseError as LalrpopError;

/// Represents a parenthesized collection that we might later convert
/// to a tuple or to `with` items.
///
/// It can be converted to either `Expr` or `ExprOrWithitems` with
/// `.try_into()`.  The `Expr` conversion will fail if any `as`
/// variables are present.  The `ExprOrWithitems` conversion cannot
/// fail (but we need it to have the same interface so we can use
/// LALRPOP macros to declare the cover grammar without much code
/// duplication).
pub struct TupleOrWithitems {
    pub location: Location,
    pub end_location: Location,
    pub items: Vec<(ast::Expr, Option<Box<ast::Expr>>)>,
}

impl TryFrom<TupleOrWithitems> for ast::Expr {
    type Error = LalrpopError<Location, Tok, LexicalError>;
    fn try_from(tuple_or_withitems: TupleOrWithitems) -> Result<ast::Expr, Self::Error> {
        Ok(ast::Expr {
            location: tuple_or_withitems.location,
            end_location: Some(tuple_or_withitems.end_location),
            custom: (),
            node: ast::ExprKind::Tuple {
                elts: tuple_or_withitems
                    .items
                    .into_iter()
                    .map(|(expr, optional_vars)| {
                        if let Some(vars) = optional_vars {
                            Err(LexicalError {
                                error: LexicalErrorType::OtherError(
                                    "cannot use 'as' here".to_string(),
                                ),
                                location: vars.location,
                            })?
                        }
                        Ok(expr)
                    })
                    .collect::<Result<Vec<ast::Expr>, Self::Error>>()?,
                ctx: ast::ExprContext::Load,
            },
        })
    }
}

impl TryFrom<TupleOrWithitems> for ExprOrWithitems {
    type Error = LalrpopError<Location, Tok, LexicalError>;
    fn try_from(items: TupleOrWithitems) -> Result<ExprOrWithitems, Self::Error> {
        Ok(ExprOrWithitems::TupleOrWithitems(items))
    }
}

/// Represents either a non-tuple expression, or a parenthesized
/// collection that we might later convert to a tuple or to `with`
/// items.
///
/// It can be constructed from an `Expr` with `.into()`.  (The same
/// interface can be used to convert an `Expr` into itself, which is
/// also important for our LALRPOP macro setup.)
///
/// It can be converted to either `Expr` or `Vec<Withitem>` with
/// `.try_into()`.  The `Expr` conversion will fail if any `as`
/// clauses are present.  The `Vec<Withitem>` conversion will fail if
/// both `as` clauses and starred expressions are present.
pub enum ExprOrWithitems {
    Expr(ast::Expr),
    TupleOrWithitems(TupleOrWithitems),
}

impl From<ast::Expr> for ExprOrWithitems {
    fn from(expr: ast::Expr) -> ExprOrWithitems {
        ExprOrWithitems::Expr(expr)
    }
}

impl TryFrom<ExprOrWithitems> for ast::Expr {
    type Error = LalrpopError<Location, Tok, LexicalError>;
    fn try_from(expr_or_withitems: ExprOrWithitems) -> Result<ast::Expr, Self::Error> {
        match expr_or_withitems {
            ExprOrWithitems::Expr(expr) => Ok(expr),
            ExprOrWithitems::TupleOrWithitems(items) => items.try_into(),
        }
    }
}

impl TryFrom<ExprOrWithitems> for Vec<ast::Withitem> {
    type Error = LalrpopError<Location, Tok, LexicalError>;
    fn try_from(expr_or_withitems: ExprOrWithitems) -> Result<Vec<ast::Withitem>, Self::Error> {
        match expr_or_withitems {
            ExprOrWithitems::TupleOrWithitems(tuple_or_withitems)
                if !tuple_or_withitems.items.iter().any(|(context_expr, _)| {
                    matches!(context_expr.node, ast::ExprKind::Starred { .. })
                }) =>
            {
                Ok(tuple_or_withitems
                    .items
                    .into_iter()
                    .map(|(context_expr, optional_vars)| ast::Withitem {
                        context_expr: Box::new(context_expr),
                        optional_vars,
                    })
                    .collect())
            }
            _ => Ok(vec![ast::Withitem {
                context_expr: Box::new(expr_or_withitems.try_into()?),
                optional_vars: None,
            }]),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_program;

    #[test]
    fn test_with_statement() {
        let source = "\
with 0: pass
with 0 as x: pass
with 0, 1: pass
with 0 as x, 1 as y: pass
with 0 if 1 else 2: pass
with 0 if 1 else 2 as x: pass
with (): pass
with () as x: pass
with (0): pass
with (0) as x: pass
with (0,): pass
with (0,) as x: pass
with (0, 1): pass
with (0, 1) as x: pass
with (*a,): pass
with (*a,) as x: pass
with (0, *a): pass
with (0, *a) as x: pass
with (a := 0): pass
with (a := 0) as x: pass
with (a := 0, b := 1): pass
with (a := 0, b := 1) as x: pass
";
        insta::assert_debug_snapshot!(parse_program(source, "<test>").unwrap());
    }

    #[test]
    fn test_with_statement_invalid() {
        for source in [
            "with 0,: pass",
            "with 0 as x,: pass",
            "with 0 as *x: pass",
            "with *a: pass",
            "with *a as x: pass",
            "with (*a): pass",
            "with (*a) as x: pass",
            "with *a, 0 as x: pass",
            "with (*a, 0 as x): pass",
            "with 0 as x, *a: pass",
            "with (0 as x, *a): pass",
            "with (0 as x) as y: pass",
            "with (0 as x), 1: pass",
            "with ((0 as x)): pass",
            "with a := 0 as x: pass",
            "with (a := 0 as x): pass",
        ] {
            assert!(parse_program(source, "<test>").is_err());
        }
    }
}
