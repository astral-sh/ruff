use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::{AnyNodeRef, ExprStringLiteral};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::SourceComment;
use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::string_literal::{FormatStringLiteral, StringLiteralKind};
use crate::prelude::*;
use crate::string::{AnyString, FormatStringContinuation, StringPrefix, StringQuotes};

#[derive(Default)]
pub struct FormatExprStringLiteral {
    kind: ExprStringLiteralKind,
}

#[derive(Default, Copy, Clone, Debug)]
pub enum ExprStringLiteralKind {
    #[default]
    String,
    Docstring,
}

impl ExprStringLiteralKind {
    const fn string_literal_kind(self) -> StringLiteralKind {
        match self {
            ExprStringLiteralKind::String => StringLiteralKind::String,
            ExprStringLiteralKind::Docstring => StringLiteralKind::Docstring,
        }
    }

    const fn is_docstring(self) -> bool {
        matches!(self, ExprStringLiteralKind::Docstring)
    }
}

impl FormatRuleWithOptions<ExprStringLiteral, PyFormatContext<'_>> for FormatExprStringLiteral {
    type Options = ExprStringLiteralKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.kind = options;
        self
    }
}

impl FormatNodeRule<ExprStringLiteral> for FormatExprStringLiteral {
    fn fmt_fields(&self, item: &ExprStringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprStringLiteral { value, .. } = item;

        match value.as_slice() {
            [string_literal] => {
                FormatStringLiteral::new(string_literal, self.kind.string_literal_kind()).fmt(f)
            }
            _ => {
                // This is just a sanity check because [`DocstringStmt::try_from_statement`]
                // ensures that the docstring is a *single* string literal.
                assert!(!self.kind.is_docstring());

                in_parentheses_only_group(&FormatStringContinuation::new(&AnyString::String(item)))
            }
            .fmt(f),
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprStringLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if is_multiline_string(self.into(), context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}

pub(super) fn is_multiline_string(expr: AnyNodeRef, source: &str) -> bool {
    if expr.is_expr_string_literal() || expr.is_expr_bytes_literal() {
        let contents = &source[expr.range()];
        let prefix = StringPrefix::parse(contents);
        let quotes =
            StringQuotes::parse(&contents[TextRange::new(prefix.text_len(), contents.text_len())]);

        quotes.is_some_and(StringQuotes::is_triple)
            && memchr::memchr2(b'\n', b'\r', contents.as_bytes()).is_some()
    } else {
        false
    }
}
