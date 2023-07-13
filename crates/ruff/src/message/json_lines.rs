use std::io::Write;

use crate::message::json::message_to_json_value;
use crate::message::{Emitter, EmitterContext, Message};

#[derive(Default)]
pub struct JsonLinesEmitter;

impl Emitter for JsonLinesEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        _context: &EmitterContext,
    ) -> anyhow::Result<()> {
        let mut w = writer;
        for message in messages {
            serde_json::to_writer(&mut w, &message_to_json_value(message))?;
            w.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::json_lines::JsonLinesEmitter;
    use crate::message::tests::{capture_emitter_output, create_messages};

    #[test]
    fn output() {
        let mut emitter = JsonLinesEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
