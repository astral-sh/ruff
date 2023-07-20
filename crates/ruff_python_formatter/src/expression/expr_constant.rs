use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{Constant, ExprConstant, Ranged};

use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::str::is_implicit_concatenation;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::expression::string::{FormatString, StringPrefix, StringQuotes};
use crate::prelude::*;
use crate::{not_yet_implemented_custom_text, verbatim_text, FormatNodeRule};

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
            Constant::Str(_) => FormatString::new(item).fmt(f),
            Constant::Bytes(_) => {
                not_yet_implemented_custom_text(r#"b"NOT_YET_IMPLEMENTED_BYTE_STRING""#).fmt(f)
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
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_str() {
            let contents = context.locator().slice(self.range());
            // Don't wrap triple quoted strings
            if is_multiline_string(self, context.source()) || !is_implicit_concatenation(contents) {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::Multiline
            }
        } else {
            OptionalParentheses::Never
        }
    }
}

pub(super) fn is_multiline_string(constant: &ExprConstant, source: &str) -> bool {
    if constant.value.is_str() {
        let contents = &source[constant.range()];
        let prefix = StringPrefix::parse(contents);
        let quotes =
            StringQuotes::parse(&contents[TextRange::new(prefix.text_len(), contents.text_len())]);

        quotes.map_or(false, StringQuotes::is_triple) && contents.contains(['\n', '\r'])
    } else {
        false
    }
}
