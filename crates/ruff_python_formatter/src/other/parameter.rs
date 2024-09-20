use crate::expression::parentheses::is_expression_parenthesized;
use crate::prelude::*;
use ruff_python_ast::Parameter;

#[derive(Default)]
pub struct FormatParameter;

impl FormatNodeRule<Parameter> for FormatParameter {
    fn fmt_fields(&self, item: &Parameter, f: &mut PyFormatter) -> FormatResult<()> {
        let Parameter {
            range: _,
            name,
            annotation,
        } = item;

        name.format().fmt(f)?;

        if let Some(annotation) = annotation.as_deref() {
            token(":").fmt(f)?;

            if f.context().comments().has_leading(annotation)
                && !is_expression_parenthesized(
                    annotation.into(),
                    f.context().comments().ranges(),
                    f.context().source(),
                )
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
