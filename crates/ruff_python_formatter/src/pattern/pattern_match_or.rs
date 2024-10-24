use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchOr;

use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space, NeedsParentheses,
    OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchOr;

impl FormatNodeRule<PatternMatchOr> for FormatPatternMatchOr {
    fn fmt_fields(&self, item: &PatternMatchOr, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchOr { range: _, patterns } = item;
        let inner = format_with(|f: &mut PyFormatter| {
            let mut patterns = patterns.iter();
            let comments = f.context().comments().clone();

            let Some(first) = patterns.next() else {
                return Ok(());
            };

            first.format().fmt(f)?;

            for pattern in patterns {
                let leading_value_comments = comments.leading(pattern);
                // Format the expressions leading comments **before** the operator
                if leading_value_comments.is_empty() {
                    write!(f, [in_parentheses_only_soft_line_break_or_space()])?;
                } else {
                    write!(
                        f,
                        [hard_line_break(), leading_comments(leading_value_comments)]
                    )?;
                }
                write!(f, [token("|"), space(), pattern.format()])?;
            }

            Ok(())
        });

        in_parentheses_only_group(&inner).fmt(f)
    }
}

impl NeedsParentheses for PatternMatchOr {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
