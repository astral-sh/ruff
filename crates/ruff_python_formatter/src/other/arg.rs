use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg;

impl FormatNodeRule<Arg> for FormatArg {
    fn fmt_fields(&self, item: &Arg, f: &mut PyFormatter) -> FormatResult<()> {
        let Arg {
            range,
            arg,
            annotation,
            type_comment: _,
        } = item;
        write!(
            f,
            [
                // The name of the argument
                source_text_slice(
                    TextRange::at(range.start(), arg.text_len()),
                    ContainsNewlines::No
                )
            ]
        )?;

        if let Some(annotation) = annotation {
            write!(f, [text(":"), space(), annotation.format()])?;
        }

        Ok(())
    }
}
