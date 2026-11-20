use ruff_formatter::write;
use ruff_python_ast::TypeParamTypeVarTuple;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamTypeVarTuple;

impl FormatNodeRule<ruff_python_ast::kind::TypeParamTypeVarTuple> for FormatTypeParamTypeVarTuple {
    fn fmt_fields(&self, item: TypeParamTypeVarTuple<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let name = item.name();
        let default = item.default();
        write!(f, [token("*"), name.format()])?;
        if let Some(default) = default {
            write!(f, [space(), token("="), space(), default.format()])?;
        }
        Ok(())
    }
}
