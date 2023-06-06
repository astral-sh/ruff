use crate::comments::leading_node_comments;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg {
    kind: ArgumentKind,
}

#[derive(Copy, Clone, Debug, Default)]
pub enum ArgumentKind {
    /// Positional only, regular argument, or a keyword only argument.
    #[default]
    Normal,
    /// A `*args` arguments
    Varg,
    /// A `**kwargs` argument
    Kwarg,
}

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
            [source_text_slice(
                TextRange::at(range.start(), arg.text_len()),
                ContainsNewlines::No
            )]
        )?;

        if let Some(annotation) = annotation {
            write!(f, [text(":"), space(), annotation.format()])?;
        }

        Ok(())
    }

    fn fmt_leading_comments(&self, node: &Arg, f: &mut PyFormatter) -> FormatResult<()> {
        match self.kind {
            ArgumentKind::Normal => leading_node_comments(node).fmt(f),
            // Formatted as part of the `Arguments` to avoid emitting leading comments between the `*` and the argument.
            ArgumentKind::Kwarg | ArgumentKind::Varg => Ok(()),
        }
    }
}

impl FormatRuleWithOptions<Arg, PyFormatContext<'_>> for FormatArg {
    type Options = ArgumentKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.kind = options;
        self
    }
}
