use std::borrow::Cow;
use std::slice::Iter;
use std::sync::Arc;

use ruff_db::diagnostic::{DiagnosticId, OldDiagnosticTrait, Severity, Span};
use ruff_db::files::File;
use ruff_python_syntax_errors::SyntaxError;
use ruff_text_size::TextRange;

#[derive(Default, Debug, Eq, PartialEq, salsa::Update)]
pub struct SyntaxDiagnostics {
    diagnostics: Vec<Arc<SyntaxDiagnostic>>,
}

impl Extend<SyntaxDiagnostic> for SyntaxDiagnostics {
    fn extend<T: IntoIterator<Item = SyntaxDiagnostic>>(&mut self, iter: T) {
        self.diagnostics.extend(iter.into_iter().map(Arc::new));
    }
}

impl<'a> Extend<&'a Arc<SyntaxDiagnostic>> for SyntaxDiagnostics {
    fn extend<T: IntoIterator<Item = &'a Arc<SyntaxDiagnostic>>>(&mut self, iter: T) {
        self.diagnostics.extend(iter.into_iter().cloned());
    }
}

impl<'a> IntoIterator for &'a SyntaxDiagnostics {
    type Item = &'a Arc<SyntaxDiagnostic>;
    type IntoIter = Iter<'a, Arc<SyntaxDiagnostic>>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

impl SyntaxDiagnostics {
    pub fn iter(&self) -> Iter<'_, Arc<SyntaxDiagnostic>> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SyntaxDiagnostic {
    id: DiagnosticId,
    message: String,
    file: File,
    range: TextRange,
}

impl OldDiagnosticTrait for SyntaxDiagnostic {
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
    pub(crate) fn from_syntax_error(value: &SyntaxError, file: File) -> Self {
        Self {
            id: DiagnosticId::InvalidSyntax,
            message: value.message(),
            file,
            range: value.range,
        }
    }
}
