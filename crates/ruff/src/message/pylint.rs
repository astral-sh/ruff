use std::io::Write;

use ruff_source_file::OneIndexed;

use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;

/// Generate violations in Pylint format.
/// See: [Flake8 documentation](https://flake8.pycqa.org/en/latest/internal/formatters.html#pylint-formatter)
#[derive(Default)]
pub struct PylintEmitter;

impl Emitter for PylintEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for message in messages {
            let row = if context.is_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                OneIndexed::from_zero_indexed(0)
            } else {
                message.compute_start_location().row
            };

            writeln!(
                writer,
                "{path}:{row}: [{code}] {body}",
                path = relativize_path(message.filename()),
                code = message.kind.rule().noqa_code(),
                body = message.kind.body,
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::PylintEmitter;

    #[test]
    fn output() {
        let mut emitter = PylintEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
