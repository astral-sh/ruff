use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprDict;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, _item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "{NOT_IMPLEMENTED_dict_key: NOT_IMPLEMENTED_dict_value}"
            )]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprDict, _f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(konstin): Reactivate when string formatting works, currently a source of unstable
        // formatting, e.g.
        // ```python
        // coverage_ignore_c_items = {
        // #    'cfunction': [...]
        // }
        // ```
        Ok(())
    }
}

impl NeedsParentheses for ExprDict {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
