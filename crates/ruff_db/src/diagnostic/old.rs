use crate::diagnostic::{Annotation, Severity, Span, SubDiagnostic};

/// A single secondary message assigned to a `Diagnostic`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OldSecondaryDiagnosticMessage {
    span: Span,
    message: String,
}

impl OldSecondaryDiagnosticMessage {
    pub fn new(span: Span, message: impl Into<String>) -> OldSecondaryDiagnosticMessage {
        OldSecondaryDiagnosticMessage {
            span,
            message: message.into(),
        }
    }

    pub fn to_sub_diagnostic(&self) -> SubDiagnostic {
        let mut sub = SubDiagnostic::new(Severity::Info, "");
        sub.annotate(Annotation::secondary(self.span.clone()).message(&self.message));
        sub
    }
}
