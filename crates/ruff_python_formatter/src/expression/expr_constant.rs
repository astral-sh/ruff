use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::expression::string::{FormatString, StringLayout};
use crate::prelude::*;
use crate::{not_yet_implemented_custom_text, verbatim_text, FormatNodeRule};
use ruff_formatter::{write, FormatRuleWithOptions};
use rustpython_parser::ast::{Constant, ExprConstant};

#[derive(Default)]
pub struct FormatExprConstant {
    string_layout: StringLayout,
}

impl FormatRuleWithOptions<ExprConstant, PyFormatContext<'_>> for FormatExprConstant {
    type Options = StringLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.string_layout = options;
        self
    }
}

impl FormatNodeRule<ExprConstant> for FormatExprConstant {
    fn fmt_fields(&self, item: &ExprConstant, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprConstant {
            range: _,
            value,
            kind: _,
        } = item;

        match value {
            Constant::Ellipsis => text("...").fmt(f),
            Constant::None => text("None").fmt(f),
            Constant::Bool(value) => match value {
                true => text("True").fmt(f),
                false => text("False").fmt(f),
            },
            Constant::Int(_) | Constant::Float(_) | Constant::Complex { .. } => {
                write!(f, [verbatim_text(item)])
            }
            Constant::Str(_) => FormatString::new(item, self.string_layout).fmt(f),
            Constant::Bytes(_) => {
                not_yet_implemented_custom_text(r#"b"NOT_YET_IMPLEMENTED_BYTE_STRING""#).fmt(f)
            }
            Constant::Tuple(_) => {
                not_yet_implemented_custom_text("(NOT_YET_IMPLEMENTED_TUPLE,)").fmt(f)
            }
        }
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprConstant,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprConstant {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional if self.value.is_str() && parenthesize.is_if_breaks() => {
                // Custom handling that only adds parentheses for implicit concatenated strings.
                if parenthesize.is_if_breaks() {
                    Parentheses::Custom
                } else {
                    Parentheses::Optional
                }
            }
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
