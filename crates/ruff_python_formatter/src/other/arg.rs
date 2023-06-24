use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg;

impl FormatNodeRule<Arg> for FormatArg {
    fn fmt_fields(&self, item: &Arg, f: &mut PyFormatter) -> FormatResult<()> {
        let Arg {
            range: _,
            arg,
            annotation,
            type_comment: _,
        } = item;

        arg.format().fmt(f)?;

        if let Some(annotation) = annotation {
            write!(f, [text(":"), space(), annotation.format()])?;
        }

        Ok(())
    }
}
