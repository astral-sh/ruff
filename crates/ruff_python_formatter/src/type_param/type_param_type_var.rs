use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use ruff_python_ast::TypeParamTypeVar;

#[derive(Default)]
pub struct FormatTypeParamTypeVar;

impl FormatNodeRule<TypeParamTypeVar> for FormatTypeParamTypeVar {
    fn fmt_fields(&self, item: &TypeParamTypeVar, f: &mut PyFormatter) -> FormatResult<()> {
        let TypeParamTypeVar {
            range: _,
            name,
            bound,
        } = item;
        name.format().fmt(f)?;
        if let Some(bound) = bound {
            write!(f, [text(":"), space(), bound.format()])?;
        }
        Ok(())
    }
}
