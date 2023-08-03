use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::TypeParamParamSpec;

#[derive(Default)]
pub struct FormatTypeParamParamSpec;

impl FormatNodeRule<TypeParamParamSpec> for FormatTypeParamParamSpec {
    fn fmt_fields(&self, item: &TypeParamParamSpec, f: &mut PyFormatter) -> FormatResult<()> {
        let TypeParamParamSpec { range: _, name } = item;
        write!(f, [text("**"), name.format()])
    }
}
