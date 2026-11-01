use ruff_formatter::write;
use ruff_python_ast::TypeParamTypeVar;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamTypeVar;

impl FormatNodeRule<ruff_python_ast::kind::TypeParamTypeVar> for FormatTypeParamTypeVar {
    fn fmt_fields(&self, item: TypeParamTypeVar<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let name = item.name();
        let bound = item.bound();
        let default = item.default();
        name.format().fmt(f)?;
        if let Some(bound) = bound {
            write!(f, [token(":"), space(), bound.format()])?;
        }
        if let Some(default) = default {
            write!(f, [space(), token("="), space(), default.format()])?;
        }
        Ok(())
    }
}
