use std::io::Write;

use ruff_db::diagnostic::{
    Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig, DisplayDiagnostics,
};
use ruff_diagnostics::Applicability;

use crate::message::{Emitter, EmitterContext};

pub struct TextEmitter {
    config: DisplayDiagnosticConfig,
}

impl Default for TextEmitter {
    fn default() -> Self {
        Self {
            config: DisplayDiagnosticConfig::default()
                .format(DiagnosticFormat::Concise)
                .hide_severity(true)
                .color(!cfg!(test) && colored::control::SHOULD_COLORIZE.should_colorize()),
        }
    }
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.config = self.config.show_fix_status(show_fix_status);
        self
    }

    #[must_use]
    pub fn with_show_fix_diff(mut self, show_fix_diff: bool) -> Self {
        self.config = self.config.show_fix_diff(show_fix_diff);
        self
    }

    #[must_use]
    pub fn with_show_source(mut self, show_source: bool) -> Self {
        self.config = self.config.format(if show_source {
            DiagnosticFormat::Full
        } else {
            DiagnosticFormat::Concise
        });
        self
    }

    #[must_use]
    pub fn with_fix_applicability(mut self, applicability: Applicability) -> Self {
        self.config = self.config.fix_applicability(applicability);
        self
    }

    #[must_use]
    pub fn with_preview(mut self, preview: bool) -> Self {
        self.config = self.config.preview(preview);
        self
    }

    #[must_use]
    pub fn with_color(mut self, color: bool) -> Self {
        self.config = self.config.color(color);
        self
    }
}

impl Emitter for TextEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        write!(
            writer,
            "{}",
            DisplayDiagnostics::new(context, &self.config, diagnostics)
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_diagnostics::Applicability;

    use crate::message::TextEmitter;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn default() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_fix_applicability(Applicability::Unsafe);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_fix_applicability(Applicability::Unsafe);
        let (messages, notebook_indexes) = create_notebook_diagnostics();
        let content = capture_emitter_notebook_output(&mut emitter, &messages, &notebook_indexes);

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }
}
