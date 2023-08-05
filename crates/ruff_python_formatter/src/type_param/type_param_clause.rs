use crate::builders::PyFormatterExtensions;
use crate::context::PyFormatContext;
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use ruff_formatter::{Format, FormatResult};
use ruff_python_ast::TypeParam;
use ruff_text_size::TextSize;

pub(crate) struct FormatTypeParamsClause<'a> {
    pub(crate) sequence_end: TextSize,
    pub(crate) type_params: &'a Vec<TypeParam>,
}

/// Formats a sequence of [`TypeParam`] nodes.
impl Format<PyFormatContext<'_>> for FormatTypeParamsClause<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if self.type_params.is_empty() {
            return Ok(());
        }

        let items = format_with(|f| {
            f.join_comma_separated(self.sequence_end)
                .nodes(self.type_params.iter())
                .finish()
        });

        parenthesized("[", &items, "]").fmt(f)
    }
}
