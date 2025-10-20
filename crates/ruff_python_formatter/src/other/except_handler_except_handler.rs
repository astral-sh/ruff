use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::write;
use ruff_python_ast::{ExceptHandlerExceptHandler, Expr, PythonVersion};

use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::preview::is_remove_parens_around_except_types_enabled;
use crate::statement::clause::{ClauseHeader, clause};
use crate::statement::suite::SuiteKind;

#[derive(Copy, Clone, Default)]
pub(crate) enum ExceptHandlerKind {
    #[default]
    Regular,
    Starred,
}

#[derive(Default)]
pub struct FormatExceptHandlerExceptHandler {
    pub(crate) except_handler_kind: ExceptHandlerKind,
    pub(crate) last_suite_in_statement: bool,
}

impl FormatRuleWithOptions<ExceptHandlerExceptHandler, PyFormatContext<'_>>
    for FormatExceptHandlerExceptHandler
{
    type Options = FormatExceptHandlerExceptHandler;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.except_handler_kind = options.except_handler_kind;
        self.last_suite_in_statement = options.last_suite_in_statement;
        self
    }
}

impl FormatNodeRule<ExceptHandlerExceptHandler> for FormatExceptHandlerExceptHandler {
    fn fmt_fields(
        &self,
        item: &ExceptHandlerExceptHandler,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let except_handler_kind = self.except_handler_kind;
        let ExceptHandlerExceptHandler {
            range: _,
            node_index: _,
            type_,
            name,
            body,
        } = item;

        let comments_info = f.context().comments().clone();
        let dangling_comments = comments_info.dangling(item);

        write!(
            f,
            [clause(
                ClauseHeader::ExceptHandler(item),
                &format_with(|f: &mut PyFormatter| {
                    write!(
                        f,
                        [
                            token("except"),
                            match except_handler_kind {
                                ExceptHandlerKind::Regular => None,
                                ExceptHandlerKind::Starred => Some(token("*")),
                            }
                        ]
                    )?;

                    match type_.as_deref() {
                        // For tuples of exception types without an `as` name and on 3.14+, the
                        // parentheses are optional.
                        //
                        // ```py
                        // try:
                        //     ...
                        // except BaseException, Exception:  # Ok
                        //     ...
                        // ```
                        Some(Expr::Tuple(tuple))
                            if f.options().target_version() >= PythonVersion::PY314
                                && is_remove_parens_around_except_types_enabled(f.context())
                                && name.is_none() =>
                        {
                            write!(
                                f,
                                [
                                    space(),
                                    tuple.format().with_options(TupleParentheses::NeverPreserve)
                                ]
                            )?;
                        }
                        Some(type_) => {
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
                        _ => {}
                    }

                    Ok(())
                }),
                dangling_comments,
                body,
                SuiteKind::other(self.last_suite_in_statement),
            )]
        )
    }
}
