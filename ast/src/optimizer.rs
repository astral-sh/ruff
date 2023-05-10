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
    #[inline]
    fn map_user(&mut self, user: U) -> Result<Self::TargetU, Self::Error> {
        Ok(user)
    }
    fn fold_expr(&mut self, node: crate::Expr<U>) -> Result<crate::Expr<U>, Self::Error> {
        match node.node {
            crate::ExprKind::Tuple(crate::ExprTuple { elts, ctx }) => {
                let elts = elts
                    .into_iter()
                    .map(|x| self.fold_expr(x))
                    .collect::<Result<Vec<_>, _>>()?;
                let expr = if elts
                    .iter()
                    .all(|e| matches!(e.node, crate::ExprKind::Constant { .. }))
                {
                    let tuple = elts
                        .into_iter()
                        .map(|e| match e.node {
                            crate::ExprKind::Constant(crate::ExprConstant { value, .. }) => value,
                            _ => unreachable!(),
                        })
                        .collect();
                    crate::ExprKind::Constant(crate::ExprConstant {
                        value: Constant::Tuple(tuple),
                        kind: None,
                    })
                } else {
                    crate::ExprKind::Tuple(crate::ExprTuple { elts, ctx })
                };
                Ok(crate::Expr {
                    node: expr,
                    custom: node.custom,
                    range: node.range,
                })
            }
            _ => crate::fold::fold_expr(self, node),
        }
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use rustpython_parser_core::text_size::TextRange;

    #[cfg(feature = "constant-optimization")]
    #[test]
    fn test_constant_opt() {
        use crate::{fold::Fold, *};

        let range = TextRange::default();
        #[allow(clippy::let_unit_value)]
        let custom = ();
        let ast = Attributed {
            range,
            custom,
            node: ExprTuple {
                ctx: ExprContext::Load,
                elts: vec![
                    Attributed {
                        range,
                        custom,
                        node: ExprConstant {
                            value: BigInt::from(1).into(),
                            kind: None,
                        }
                        .into(),
                    },
                    Attributed {
                        range,
                        custom,
                        node: ExprConstant {
                            value: BigInt::from(2).into(),
                            kind: None,
                        }
                        .into(),
                    },
                    Attributed {
                        range,
                        custom,
                        node: ExprTuple {
                            ctx: ExprContext::Load,
                            elts: vec![
                                Attributed {
                                    range,
                                    custom,
                                    node: ExprConstant {
                                        value: BigInt::from(3).into(),
                                        kind: None,
                                    }
                                    .into(),
                                },
                                Attributed {
                                    range,
                                    custom,
                                    node: ExprConstant {
                                        value: BigInt::from(4).into(),
                                        kind: None,
                                    }
                                    .into(),
                                },
                                Attributed {
                                    range,
                                    custom,
                                    node: ExprConstant {
                                        value: BigInt::from(5).into(),
                                        kind: None,
                                    }
                                    .into(),
                                },
                            ],
                        }
                        .into(),
                    },
                ],
            }
            .into(),
        };
        let new_ast = ConstantOptimizer::new()
            .fold_expr(ast)
            .unwrap_or_else(|e| match e {});
        assert_eq!(
            new_ast,
            Attributed {
                range,
                custom,
                node: ExprConstant {
                    value: Constant::Tuple(vec![
                        BigInt::from(1).into(),
                        BigInt::from(2).into(),
                        Constant::Tuple(vec![
                            BigInt::from(3).into(),
                            BigInt::from(4).into(),
                            BigInt::from(5).into(),
                        ])
                    ]),
                    kind: None
                }
                .into(),
            }
        );
    }
}
