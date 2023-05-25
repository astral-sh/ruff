use crate::builtin::Constant;

#[non_exhaustive]
#[derive(Default)]
pub struct ConstantOptimizer {}

impl ConstantOptimizer {
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(feature = "constant-optimization")]
impl<U> crate::fold::Fold<U> for ConstantOptimizer {
    type TargetU = U;
    type Error = std::convert::Infallible;
    type UserContext = ();

    #[inline(always)]
    fn will_map_user(&mut self, _user: &U) -> Self::UserContext {}
    #[inline]
    fn map_user(&mut self, user: U, _context: ()) -> Result<Self::TargetU, Self::Error> {
        Ok(user)
    }
    fn fold_expr(&mut self, node: crate::Expr<U>) -> Result<crate::Expr<U>, Self::Error> {
        match node {
            crate::Expr::Tuple(crate::ExprTuple { elts, ctx, range }) => {
                let elts = elts
                    .into_iter()
                    .map(|x| self.fold_expr(x))
                    .collect::<Result<Vec<_>, _>>()?;
                let expr = if elts.iter().all(|e| e.is_constant_expr()) {
                    let tuple = elts
                        .into_iter()
                        .map(|e| match e {
                            crate::Expr::Constant(crate::ExprConstant { value, .. }) => value,
                            _ => unreachable!(),
                        })
                        .collect();
                    crate::Expr::Constant(crate::ExprConstant {
                        value: Constant::Tuple(tuple),
                        kind: None,
                        range,
                    })
                } else {
                    crate::Expr::Tuple(crate::ExprTuple { elts, ctx, range })
                };
                Ok(expr)
            }
            _ => crate::fold::fold_expr(self, node),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bigint::BigInt;
    use rustpython_parser_core::text_size::TextRange;

    #[cfg(feature = "constant-optimization")]
    #[test]
    fn test_constant_opt() {
        use crate::{fold::Fold, *};

        let range = TextRange::default();
        let ast = ExprTuple {
            ctx: ExprContext::Load,
            elts: vec![
                ExprConstant {
                    value: BigInt::from(1).into(),
                    kind: None,
                    range,
                }
                .into(),
                ExprConstant {
                    value: BigInt::from(2).into(),
                    kind: None,
                    range,
                }
                .into(),
                ExprTuple {
                    ctx: ExprContext::Load,
                    elts: vec![
                        ExprConstant {
                            value: BigInt::from(3).into(),
                            kind: None,
                            range,
                        }
                        .into(),
                        ExprConstant {
                            value: BigInt::from(4).into(),
                            kind: None,
                            range,
                        }
                        .into(),
                        ExprConstant {
                            value: BigInt::from(5).into(),
                            kind: None,
                            range,
                        }
                        .into(),
                    ],
                    range,
                }
                .into(),
            ],
            range,
        };
        let new_ast = ConstantOptimizer::new()
            .fold_expr(ast.into())
            .unwrap_or_else(|e| match e {});
        assert_eq!(
            new_ast,
            ExprConstant {
                value: Constant::Tuple(vec![
                    BigInt::from(1).into(),
                    BigInt::from(2).into(),
                    Constant::Tuple(vec![
                        BigInt::from(3).into(),
                        BigInt::from(4).into(),
                        BigInt::from(5).into(),
                    ])
                ]),
                kind: None,
                range,
            }
            .into(),
        );
    }
}
