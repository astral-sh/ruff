use ruff_formatter::write;
use ruff_python_ast::ParameterWithDefault;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatParameterWithDefault;

impl FormatNodeRule<ParameterWithDefault> for FormatParameterWithDefault {
    fn fmt_fields(&self, item: &ParameterWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        let ParameterWithDefault {
            range: _,
            parameter,
            default,
        } = item;

        write!(f, [parameter.format()])?;

        if let Some(default) = default {
            let space = parameter.annotation.is_some().then_some(space());
            write!(f, [space, token("="), space, group(&default.format())])?;
        }

        Ok(())
    }
}
