use std::borrow::Cow;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::File;
use ruff_python_parser::UnsupportedSyntaxError;
use ruff_text_size::TextRange;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SyntaxDiagnostic {
    id: DiagnosticId,
    message: String,
    file: File,
    range: TextRange,
}

impl Diagnostic for SyntaxDiagnostic {
    fn id(&self) -> ruff_db::diagnostic::DiagnosticId {
        self.id
    }

    fn message(&self) -> Cow<str> {
        Cow::from(&self.message)
    }

    fn severity(&self) -> ruff_db::diagnostic::Severity {
        Severity::Error
    }

    fn span(&self) -> Option<ruff_db::diagnostic::Span> {
        Some(Span::from(self.file).with_range(self.range))
    }
}

impl SyntaxDiagnostic {
    pub fn from_unsupported_syntax_error(value: &UnsupportedSyntaxError, file: File) -> Self {
        Self {
            id: DiagnosticId::InvalidSyntax,
            message: value.to_string(),
            file,
            range: value.range,
        }
    }
}
