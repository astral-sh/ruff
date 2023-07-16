use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatContext};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprName;

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

        write!(f, [source_text_slice(*range, ContainsNewlines::No)])
    }
}

impl NeedsParentheses for ExprName {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_parser::ast::{ModModule, Ranged};
    use rustpython_parser::Parse;

    #[test]
    fn name_range_with_comments() {
        let source = ModModule::parse("a # comment", "file.py").unwrap();

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
