use ruff_formatter::write;
use rustpython_parser::ast::ArgWithDefault;

use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatArgWithDefault;

impl FormatNodeRule<ArgWithDefault> for FormatArgWithDefault {
    fn fmt_fields(&self, item: &ArgWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        let ArgWithDefault {
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
