use std::borrow::Cow;
use std::sync::Arc;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_python_syntax_errors::{self as syntax, SyntaxError};
use ruff_text_size::TextRange;

use crate::PythonVersion;

/// Mirrors the structure of `TypeCheckDiagnostics`
#[derive(Default, Eq, PartialEq)]
pub struct SyntaxDiagnostics {
    diagnostics: Vec<Arc<SyntaxDiagnostic>>,
}

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

impl From<PythonVersion> for syntax::PythonVersion {
    fn from(value: PythonVersion) -> Self {
        Self {
            major: value.major,
            minor: value.minor,
        }
    }
}

impl SyntaxDiagnostic {
    pub fn from_syntax_error(value: &SyntaxError, file: File) -> Self {
        Self {
            id: DiagnosticId::invalid_syntax(Some(value.kind.as_str())),
            message: value.message(),
            file,
            range: value.range,
        }
    }
}
