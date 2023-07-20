use ruff_text_size::{TextRange, TextSize};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize};
use crate::{not_yet_implemented, FormatNodeRule, PyFormatter, AsFormat};
use ruff_formatter::{write, Buffer, FormatResult, Format, FormatError, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::{ExprYield, Ranged};
use ruff_formatter::prelude::{space, text};
use ruff_python_whitespace::{SimpleTokenizer, TokenKind};
use crate::expression::maybe_parenthesize_expression;

#[derive(Default)]
pub struct FormatExprYield{
    parentheses: Option<Parentheses>,
}
impl FormatRuleWithOptions<ExprYield, PyFormatContext<'_>> for FormatExprYield {
    type Options = Option<Parentheses>;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<ExprYield> for FormatExprYield {
    fn fmt_fields(&self, item: &ExprYield, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprYield {
            range: _,
            value
        } = item;

        if let Some(val) = value {
            write!(
                f,
                [&text("yield"), space(), maybe_parenthesize_expression(val, item, Parenthesize::IfRequired)]
            )?;
        } else {
            write!(
                f,
                [&text("yield")]
            )?;
        }
        Ok(())
    }
}

impl NeedsParentheses for ExprYield {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if _parent.is_stmt_return() || _parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
