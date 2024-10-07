use ruff_formatter::{FormatRuleWithOptions, GroupId};
use ruff_python_ast::{AnyNodeRef, ExprStringLiteral, StringLike};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::string_literal::StringLiteralKind;
use crate::prelude::*;
use crate::string::{FormatImplicitConcatenatedString, StringLikeExtensions};

#[derive(Default)]
pub struct FormatExprStringLiteral {
    layout: ExprStringLiteralLayout,
}

#[derive(Default)]
pub struct ExprStringLiteralLayout {
    pub kind: StringLiteralKind,
    pub implicit_group_id: Option<GroupId>,
}

impl ExprStringLiteralLayout {
    pub const fn docstring() -> Self {
        Self {
            kind: StringLiteralKind::Docstring,
            implicit_group_id: None,
        }
    }
}

impl FormatRuleWithOptions<ExprStringLiteral, PyFormatContext<'_>> for FormatExprStringLiteral {
    type Options = ExprStringLiteralLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprStringLiteral> for FormatExprStringLiteral {
    fn fmt_fields(&self, item: &ExprStringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprStringLiteral { value, .. } = item;

        match value.as_slice() {
            [string_literal] => string_literal
                .format()
                .with_options(self.layout.kind)
                .fmt(f),
            _ => {
                // This is just a sanity check because [`DocstringStmt::try_from_statement`]
                // ensures that the docstring is a *single* string literal.
                assert!(!self.layout.kind.is_docstring());

                match self.layout.implicit_group_id {
                    Some(group_id) => group(&FormatImplicitConcatenatedString::new(item))
                        .with_group_id(Some(group_id))
                        .fmt(f),
                    None => in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item))
                        .fmt(f),
                }
            }
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
        } else if StringLike::from(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
