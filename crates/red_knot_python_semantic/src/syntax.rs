use std::borrow::Cow;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_python_parser::SyntaxError;
use ruff_text_size::TextRange;

use crate::PythonVersion;

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

    fn file(&self) -> Option<File> {
        Some(self.file)
    }

    fn range(&self) -> Option<TextRange> {
        Some(self.range)
    }

    fn severity(&self) -> ruff_db::diagnostic::Severity {
        Severity::Error
    }
}

impl From<PythonVersion> for ruff_python_parser::version::PythonVersion {
    fn from(value: PythonVersion) -> Self {
        Self {
            major: value.major,
            minor: value.minor,
        }
    }
}

impl SyntaxDiagnostic {
    pub fn from_syntax_error(
        value: &SyntaxError,
        file: File,
        target_version: PythonVersion,
    ) -> Self {
        Self {
            id: DiagnosticId::invalid_syntax(Some(value.kind.as_str())),
            message: value.message(target_version.into()),
            file,
            range: value.range,
        }
    }
}
