use std::io::Write;

use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};

use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext};
use crate::settings::types::UnsafeFixes;

pub struct TextEmitter {
    /// Whether to show the diff of a fix, for diagnostics that have a fix.
    ///
    /// Note that this is not currently exposed in the CLI (#7352) and is only used in tests.
    show_fix_diff: bool,
    config: DisplayDiagnosticConfig,
}

impl Default for TextEmitter {
    fn default() -> Self {
        Self {
            show_fix_diff: false,
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
        self.show_fix_diff = show_fix_diff;
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
    pub fn with_unsafe_fixes(mut self, unsafe_fixes: UnsafeFixes) -> Self {
        self.config = self
            .config
            .fix_applicability(unsafe_fixes.required_applicability());
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
        for message in diagnostics {
            write!(writer, "{}", message.display(context, &self.config))?;

            if self.show_fix_diff {
                if let Some(diff) = Diff::from_message(message) {
                    writeln!(writer, "{diff}")?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::TextEmitter;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };
    use crate::settings::types::UnsafeFixes;

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
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
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
