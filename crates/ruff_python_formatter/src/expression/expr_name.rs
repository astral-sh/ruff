use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprName;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprName;

impl FormatNodeRule<ExprName> for FormatExprName {
    fn fmt_fields(&self, item: &ExprName, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprName {
            id: _,
            range,
            ctx: _,
        } = item;
        write!(f, [source_text_slice(*range)])
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
    use ruff_python_parser::parse_module;
    use ruff_text_size::{Ranged, TextRange, TextSize};

    #[test]
    fn name_range_with_comments() {
        let module = parse_module("a # comment").unwrap();

        let expression_statement = module
            .suite()
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
