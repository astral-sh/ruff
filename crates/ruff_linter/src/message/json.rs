use std::io::Write;

use ruff_db::diagnostic::{
    Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig, DisplayDiagnostics,
};

use crate::message::{Emitter, EmitterContext};

#[derive(Default)]
pub struct JsonEmitter;

impl Emitter for JsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        let config = DisplayDiagnosticConfig::default().format(DiagnosticFormat::Json);
        write!(
            writer,
            "{}",
            DisplayDiagnostics::new(context, &config, diagnostics)
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::JsonEmitter;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = JsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = JsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = JsonEmitter;
        let (diagnostics, notebook_indexes) = create_notebook_diagnostics();
        let content =
            capture_emitter_notebook_output(&mut emitter, &diagnostics, &notebook_indexes);

        assert_snapshot!(content);
    }
}
