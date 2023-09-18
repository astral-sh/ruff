use ruff_formatter::write;
use ruff_python_ast::TypeParamTypeVarTuple;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamTypeVarTuple;

impl FormatNodeRule<TypeParamTypeVarTuple> for FormatTypeParamTypeVarTuple {
    fn fmt_fields(&self, item: &TypeParamTypeVarTuple, f: &mut PyFormatter) -> FormatResult<()> {
        let TypeParamTypeVarTuple { range: _, name } = item;
        write!(f, [token("*"), name.format()])
    }
}
