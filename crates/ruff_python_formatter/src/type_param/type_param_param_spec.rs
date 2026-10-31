use ruff_formatter::write;
use ruff_python_ast::TypeParamParamSpec;
use ruff_python_ast::flat;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatTypeParamParamSpec;

impl FormatNodeRule<TypeParamParamSpec> for FormatTypeParamParamSpec {
    fn fmt_fields(
        &self,
        item: flat::View<'_, TypeParamParamSpec>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let name = item.name();
        let default = item.default();
        write!(f, [token("**"), name.format()])?;
        if let Some(default) = default {
            write!(f, [space(), token("="), space(), default.format()])?;
        }
        Ok(())
    }
}
