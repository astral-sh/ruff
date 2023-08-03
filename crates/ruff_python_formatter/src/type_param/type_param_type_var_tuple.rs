use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::TypeParamTypeVarTuple;

#[derive(Default)]
pub struct FormatTypeParamTypeVarTuple;

impl FormatNodeRule<TypeParamTypeVarTuple> for FormatTypeParamTypeVarTuple {
    fn fmt_fields(&self, item: &TypeParamTypeVarTuple, f: &mut PyFormatter) -> FormatResult<()> {
        let TypeParamTypeVarTuple { range: _, name } = item;
        write!(f, [text("*"), name.format()])
    }
}
