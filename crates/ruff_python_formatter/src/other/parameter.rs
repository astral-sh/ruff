use crate::prelude::*;
use ruff_python_ast::Parameter;
use ruff_python_ast::flat;

#[derive(Default)]
pub struct FormatParameter;

impl FormatNodeRule<Parameter> for FormatParameter {
    fn fmt_fields(&self, item: flat::View<'_, Parameter>, f: &mut PyFormatter) -> FormatResult<()> {
        let name = item.name();
        let annotation = item.annotation();

        name.format().fmt(f)?;

        if let Some(annotation) = annotation {
            token(":").fmt(f)?;

            if f.context().comments().has_leading(annotation)
                && !f.context().is_expression_parenthesized(annotation)
            {
                hard_line_break().fmt(f)?;
            } else {
                space().fmt(f)?;
            }

            annotation.format().fmt(f)?;
        }

        Ok(())
    }
}
