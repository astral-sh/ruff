use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchClass;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: &PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchClass {
            range: _,
            cls,
            arguments,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(
            f,
            [
                cls.format(),
                dangling_comments(dangling),
                arguments.format()
            ]
        )
    }
}

impl NeedsParentheses for PatternMatchClass {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // If there are any comments outside of the class parentheses, break:
        // ```python
        // case (
        //     Pattern
        //     # dangling
        //     (...)
        // ): ...
        // ```
        if context.comments().has_dangling(self) {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Never
        }
    }
}
