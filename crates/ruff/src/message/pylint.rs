use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use std::io::Write;

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
            let row = if context.is_jupyter_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                1
            } else {
                message.location.row()
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
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::PylintEmitter;
    use insta::assert_snapshot;

    #[test]
    fn output() {
        let mut emitter = PylintEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
