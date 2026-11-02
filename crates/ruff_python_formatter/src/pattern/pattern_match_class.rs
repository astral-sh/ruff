use ruff_formatter::write;
use ruff_python_ast::{PatternMatchClass, SyntaxNode};

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<ruff_python_ast::kind::PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: PatternMatchClass<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let cls = item.cls();
        let arguments = item.arguments();

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

impl NeedsParentheses for PatternMatchClass<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
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
