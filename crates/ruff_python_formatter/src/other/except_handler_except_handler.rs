use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::write;
use ruff_python_ast::{ExceptHandlerExceptHandler, ExprKind, PythonVersion};

use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
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

impl FormatRuleWithOptions<ExceptHandlerExceptHandler<'_>, PyFormatContext<'_>>
    for FormatExceptHandlerExceptHandler
{
    type Options = FormatExceptHandlerExceptHandler;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.except_handler_kind = options.except_handler_kind;
        self.last_suite_in_statement = options.last_suite_in_statement;
        self
    }
}

impl FormatNodeRule<ruff_python_ast::kind::ExceptHandlerExceptHandler>
    for FormatExceptHandlerExceptHandler
{
    fn fmt_fields(
        &self,
        item: ExceptHandlerExceptHandler<'_>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let except_handler_kind = self.except_handler_kind;
        let type_ = item.type_();
        let name = item.name();
        let body = item.body();

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

                    match type_.map(ruff_python_ast::Expr::classify) {
                        // For tuples of exception types without an `as` name and on 3.14+, the
                        // parentheses are optional.
                        //
                        // ```py
                        // try:
                        //     ...
                        // except BaseException, Exception:  # Ok
                        //     ...
                        // ```
                        //
                        // Unless any component of the tuple is starred. This case is actually valid
                        // syntax on its own but is parsed as `except*`, not a tuple with a starred
                        // element:
                        //
                        // ```py
                        // try:
                        //     ...
                        // except *exceptions, BaseException:
                        //     ...
                        // ```
                        //
                        // And this case is an outright `SyntaxError`:
                        //
                        // ```py
                        // try:
                        //     ...
                        // except BaseException, *exceptions:  # SyntaxError
                        //     ...
                        // ```
                        Some(ExprKind::Tuple(tuple))
                            if f.options().target_version() >= PythonVersion::PY314
                                && name.is_none()
                                && !tuple
                                    .elts()
                                    .iter()
                                    .any(ruff_python_ast::Expr::is_starred_expr) =>
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
                            let type_ = type_.view();
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
