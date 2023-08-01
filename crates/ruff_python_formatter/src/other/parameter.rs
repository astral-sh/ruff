use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
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

        if let Some(annotation) = annotation {
            write!(f, [text(":"), space(), annotation.format()])?;
        }

        Ok(())
    }
}
