use ruff_formatter::write;
use ruff_python_ast::TypeParamParamSpec;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamParamSpec;

impl FormatNodeRule<TypeParamParamSpec> for FormatTypeParamParamSpec {
    fn fmt_fields(&self, item: &TypeParamParamSpec, f: &mut PyFormatter) -> FormatResult<()> {
        let TypeParamParamSpec {
            range: _,
            name,
            default,
        } = item;
        write!(f, [token("**"), name.format()])?;
        if let Some(default) = default {
            write!(f, [space(), token("="), space(), default.format()])?;
        }
        Ok(())
    }
}
