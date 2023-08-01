use ruff_formatter::write;
use ruff_python_ast::ParameterWithDefault;

use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatParameterWithDefault;

impl FormatNodeRule<ParameterWithDefault> for FormatParameterWithDefault {
    fn fmt_fields(&self, item: &ParameterWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        let ParameterWithDefault {
            range: _,
            def,
            default,
        } = item;

        write!(f, [def.format()])?;

        if let Some(default) = default {
            let space = def.annotation.is_some().then_some(space());
            write!(f, [space, text("="), space, group(&default.format())])?;
        }

        Ok(())
    }
}
