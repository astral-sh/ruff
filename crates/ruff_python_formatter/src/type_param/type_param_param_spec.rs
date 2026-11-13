use ruff_formatter::write;
use ruff_python_ast::TypeParamParamSpec;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamParamSpec;

impl FormatNodeRule<ruff_python_ast::kind::TypeParamParamSpec> for FormatTypeParamParamSpec {
    fn fmt_fields(&self, item: TypeParamParamSpec<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let name = item.name();
        let default = item.default();
        write!(f, [token("**"), name.format()])?;
        if let Some(default) = default {
            write!(f, [space(), token("="), space(), default.format()])?;
        }
        Ok(())
    }
}
