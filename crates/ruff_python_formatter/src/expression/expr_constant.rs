use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::{not_yet_implemented_custom_text, verbatim_text, FormatNodeRule};
use ruff_formatter::write;
use rustpython_parser::ast::{Constant, ExprConstant};

#[derive(Default)]
pub struct FormatExprConstant;

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
            Constant::Str(_) => {
                not_yet_implemented_custom_text(r#""NOT_YET_IMPLEMENTED_STRING""#).fmt(f)
            }
            Constant::Bytes(_) => {
                not_yet_implemented_custom_text(r#"b"NOT_YET_IMPLEMENTED_BYTE_STRING""#).fmt(f)
            }
            Constant::Tuple(_) => {
                not_yet_implemented_custom_text("(NOT_YET_IMPLEMENTED_TUPLE,)").fmt(f)
            }
        }
    }
}

impl NeedsParentheses for ExprConstant {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source)
    }
}
