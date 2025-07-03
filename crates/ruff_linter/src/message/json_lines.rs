use std::io::Write;

use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};

use crate::message::{DummyFileResolver, Emitter, EmitterContext};

#[derive(Default)]
pub struct JsonLinesEmitter;

impl Emitter for JsonLinesEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        let resolver = DummyFileResolver;
        let config = DisplayDiagnosticConfig::default().format(DiagnosticFormat::Json);
        for diagnostic in diagnostics {
            write!(
                writer,
                "{}",
                diagnostic.display(&resolver, &config, context.notebook_indexes)
            )?;
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
