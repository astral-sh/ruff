use std::io::Write;

use crate::message::json::message_to_json_value;
use crate::message::{Emitter, EmitterContext, OldDiagnostic};

#[derive(Default)]
pub struct JsonLinesEmitter;

impl Emitter for JsonLinesEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[OldDiagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for diagnostic in diagnostics {
            serde_json::to_writer(&mut *writer, &message_to_json_value(diagnostic, context))?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::json_lines::JsonLinesEmitter;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = JsonLinesEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = JsonLinesEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = JsonLinesEmitter;
        let (messages, notebook_indexes) = create_notebook_diagnostics();
        let content = capture_emitter_notebook_output(&mut emitter, &messages, &notebook_indexes);

        assert_snapshot!(content);
    }
}
