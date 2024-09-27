use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::{AnyNodeRef, ExprStringLiteral};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::string_literal::{FormatStringLiteral, StringLiteralKind};
use crate::prelude::*;
use crate::string::{AnyString, FormatImplicitConcatenatedString};

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

                in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item))
            }
            .fmt(f),
        }
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
        } else if AnyString::String(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
