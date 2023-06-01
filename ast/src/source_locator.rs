use crate::Fold;
use rustpython_parser_core::{
    source_code::{LinearLocator, RandomLocator, SourceLocation, SourceRange},
    text_size::TextRange,
};
use std::{convert::Infallible, unreachable};

impl crate::fold::Fold<TextRange> for RandomLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;
    type UserContext = SourceLocation;

    fn will_map_user(&mut self, user: &TextRange) -> Self::UserContext {
        self.locate(user.start())
    }

    fn map_user(
        &mut self,
        user: TextRange,
        start: Self::UserContext,
    ) -> Result<Self::TargetU, Self::Error> {
        let end = self.locate(user.end());
        Ok((start..end).into())
    }
}

fn linear_locate_expr_joined_str(
    locator: &mut LinearLocator<'_>,
    node: crate::ExprJoinedStr<TextRange>,
    location: SourceRange,
) -> Result<crate::ExprJoinedStr<SourceRange>, Infallible> {
    let crate::ExprJoinedStr { range: _, values } = node;

    let mut located_values = Vec::with_capacity(values.len());
    for value in values.into_iter() {
        let located = match value {
            crate::Expr::Constant(constant) => {
                let node = crate::ExprConstant {
                    range: location,
                    value: constant.value,
                    kind: constant.kind,
                };
                crate::Expr::Constant(node)
            }
            crate::Expr::FormattedValue(formatted) => {
                let node = crate::ExprFormattedValue {
                    range: location,
                    value: locator.fold(formatted.value)?,
                    conversion: formatted.conversion,
                    format_spec: formatted
                        .format_spec
                        .map(|spec| match *spec {
                            crate::Expr::JoinedStr(joined_str) => {
                                let node =
                                    linear_locate_expr_joined_str(locator, joined_str, location)?;
                                Ok(crate::Expr::JoinedStr(node))
                            }
                            expr => locator.fold(expr),
                        })
                        .transpose()?
                        .map(Box::new),
                };
                crate::Expr::FormattedValue(node)
            }
            _ => unreachable!("missing expr type for joined_str?"),
        };
        located_values.push(located);
    }

    Ok(crate::ExprJoinedStr {
        range: location,
        values: located_values,
    })
}

impl crate::fold::Fold<TextRange> for LinearLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;
    type UserContext = SourceLocation;

    fn will_map_user(&mut self, user: &TextRange) -> Self::UserContext {
        self.locate(user.start())
    }

    fn map_user(
        &mut self,
        user: TextRange,
        start: Self::UserContext,
    ) -> Result<Self::TargetU, Self::Error> {
        let end = self.locate(user.end());
        Ok((start..end).into())
    }

    fn fold_expr_dict(
        &mut self,
        node: crate::ExprDict<TextRange>,
    ) -> Result<crate::ExprDict<Self::TargetU>, Self::Error> {
        let crate::ExprDict {
            range,
            keys,
            values,
        } = node;
        let context = self.will_map_user(&range);
        assert_eq!(keys.len(), values.len());
        let mut located_keys = Vec::with_capacity(keys.len());
        let mut located_values = Vec::with_capacity(values.len());
        for (key, value) in keys.into_iter().zip(values.into_iter()) {
            located_keys.push(self.fold(key)?);
            located_values.push(self.fold(value)?);
        }
        let range = self.map_user(range, context)?;
        Ok(crate::ExprDict {
            range,
            keys: located_keys,
            values: located_values,
        })
    }

    fn fold_expr_if_exp(
        &mut self,
        node: crate::ExprIfExp<TextRange>,
    ) -> Result<crate::ExprIfExp<Self::TargetU>, Self::Error> {
        let crate::ExprIfExp {
            range,
            test,
            body,
            orelse,
        } = node;
        let context = self.will_map_user(&range);
        let body = self.fold(body)?;
        let test = self.fold(test)?;
        let orelse = self.fold(orelse)?;
        let range = self.map_user(range, context)?;
        Ok(crate::ExprIfExp {
            range,
            test,
            body,
            orelse,
        })
    }

    fn fold_stmt_class_def(
        &mut self,
        node: crate::StmtClassDef<TextRange>,
    ) -> Result<crate::StmtClassDef<Self::TargetU>, Self::Error> {
        let crate::StmtClassDef {
            name,
            bases,
            keywords,
            body,
            decorator_list,
            range,
        } = node;
        let decorator_list = self.fold(decorator_list)?;
        let context = self.will_map_user(&range);

        let name = self.fold(name)?;
        let bases = self.fold(bases)?;
        let keywords = self.fold(keywords)?;
        let body = self.fold(body)?;
        let range = self.map_user(range, context)?;
        Ok(crate::StmtClassDef {
            name,
            bases,
            keywords,
            body,
            decorator_list,
            range,
        })
    }
    fn fold_stmt_function_def(
        &mut self,
        node: crate::StmtFunctionDef<TextRange>,
    ) -> Result<crate::StmtFunctionDef<Self::TargetU>, Self::Error> {
        let crate::StmtFunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
            range,
        } = node;
        let decorator_list = self.fold(decorator_list)?;
        let context = self.will_map_user(&range);

        let name = self.fold(name)?;
        let args: Box<crate::Arguments<SourceRange>> = self.fold(args)?;
        let returns = self.fold(returns)?;
        let body = self.fold(body)?;
        let type_comment = self.fold(type_comment)?;
        let range = self.map_user(range, context)?;
        Ok(crate::StmtFunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
            range,
        })
    }
    fn fold_stmt_async_function_def(
        &mut self,
        node: crate::StmtAsyncFunctionDef<TextRange>,
    ) -> Result<crate::StmtAsyncFunctionDef<Self::TargetU>, Self::Error> {
        let crate::StmtAsyncFunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
            range,
        } = node;
        let decorator_list = self.fold(decorator_list)?;
        let context = self.will_map_user(&range);

        let name = self.fold(name)?;
        let args: Box<crate::Arguments<SourceRange>> = self.fold(args)?;
        let returns = self.fold(returns)?;
        let body = self.fold(body)?;
        let type_comment = self.fold(type_comment)?;
        let range = self.map_user(range, context)?;
        Ok(crate::StmtAsyncFunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
            range,
        })
    }
    fn fold_expr_joined_str(
        &mut self,
        node: crate::ExprJoinedStr<TextRange>,
    ) -> Result<crate::ExprJoinedStr<Self::TargetU>, Self::Error> {
        let start = self.locate(node.range.start());
        let end = self.locate_only(node.range.end());
        let location = SourceRange::new(start, end);
        linear_locate_expr_joined_str(self, node, location)
    }

    fn fold_expr_call(
        &mut self,
        node: crate::ExprCall<TextRange>,
    ) -> Result<crate::ExprCall<Self::TargetU>, Self::Error> {
        let crate::ExprCall {
            range,
            func,
            args,
            keywords,
        } = node;
        let context = self.will_map_user(&range);
        let func = self.fold(func)?;
        let keywords = LinearLookaheadLocator(self).fold(keywords)?;
        let args = self.fold(args)?;
        let range = self.map_user(range, context)?;
        Ok(crate::ExprCall {
            range,
            func,
            args,
            keywords,
        })
    }
}

struct LinearLookaheadLocator<'a, 'b>(&'b mut LinearLocator<'a>);

impl crate::fold::Fold<TextRange> for LinearLookaheadLocator<'_, '_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;
    type UserContext = SourceLocation;

    fn will_map_user(&mut self, user: &TextRange) -> Self::UserContext {
        self.0.locate_only(user.start())
    }

    fn map_user(
        &mut self,
        user: TextRange,
        start: Self::UserContext,
    ) -> Result<Self::TargetU, Self::Error> {
        let end = self.0.locate_only(user.end());
        Ok((start..end).into())
    }
}
