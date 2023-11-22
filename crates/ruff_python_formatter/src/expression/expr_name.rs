use ruff_formatter::{write, FormatContext};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprName;

use crate::comments::SourceComment;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprName;

impl FormatNodeRule<ExprName> for FormatExprName {
    fn fmt_fields(&self, item: &ExprName, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprName { id, range, ctx: _ } = item;

        debug_assert_eq!(
            id.as_str(),
            f.context()
                .source_code()
                .slice(*range)
                .text(f.context().source_code())
        );

        write!(f, [source_text_slice(*range)])
    }

    fn fmt_dangling_comments(
        &self,
        dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Node cannot have dangling comments
        debug_assert!(dangling_comments.is_empty());
        Ok(())
    }
}

impl NeedsParentheses for ExprName {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::parse_program;
    use ruff_text_size::{Ranged, TextRange, TextSize};

    #[test]
    fn name_range_with_comments() {
        let source = parse_program("a # comment", "file.py").unwrap();

        let expression_statement = source
            .body
            .first()
            .expect("Expected non-empty body")
            .as_expr_stmt()
            .unwrap();
        let name = expression_statement
            .value
            .as_name_expr()
            .expect("Expected name expression");

        assert_eq!(
            name.range(),
            TextRange::at(TextSize::new(0), TextSize::new(1))
        );
    }
}
