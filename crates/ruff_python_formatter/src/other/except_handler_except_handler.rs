use ruff_formatter::write;
use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::ExceptHandlerExceptHandler;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};

#[derive(Copy, Clone, Default)]
pub enum ExceptHandlerKind {
    #[default]
    Regular,
    Starred,
}

#[derive(Default)]
pub struct FormatExceptHandlerExceptHandler {
    except_handler_kind: ExceptHandlerKind,
}

impl FormatRuleWithOptions<ExceptHandlerExceptHandler, PyFormatContext<'_>>
    for FormatExceptHandlerExceptHandler
{
    type Options = ExceptHandlerKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.except_handler_kind = options;
        self
    }
}

impl FormatNodeRule<ExceptHandlerExceptHandler> for FormatExceptHandlerExceptHandler {
    fn fmt_fields(
        &self,
        item: &ExceptHandlerExceptHandler,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let ExceptHandlerExceptHandler {
            range: _,
            type_,
            name,
            body,
        } = item;

        let comments_info = f.context().comments().clone();
        let dangling_comments = comments_info.dangling(item);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::ExceptHandler(item),
                    dangling_comments,
                    &format_with(|f| {
                        write!(
                            f,
                            [
                                token("except"),
                                match self.except_handler_kind {
                                    ExceptHandlerKind::Regular => None,
                                    ExceptHandlerKind::Starred => Some(token("*")),
                                }
                            ]
                        )?;

                        if let Some(type_) = type_ {
                            write!(
                                f,
                                [
                                    space(),
                                    maybe_parenthesize_expression(
                                        type_,
                                        item,
                                        Parenthesize::IfBreaks
                                    )
                                ]
                            )?;
                            if let Some(name) = name {
                                write!(f, [space(), token("as"), space(), name.format()])?;
                            }
                        }

                        Ok(())
                    }),
                ),
                clause_body(body, dangling_comments),
            ]
        )
    }
}
