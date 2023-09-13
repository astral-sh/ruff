pub use diagnostic::{Diagnostic, DiagnosticKind};
pub use edit::Edit;
pub use fix::{Applicability, Fix, IsolationLevel};
pub use source_map::{SourceMap, SourceMarker};
pub use violation::{AlwaysAutofixableViolation, AutofixKind, Violation};

mod diagnostic;
mod edit;
mod fix;
mod source_map;
mod violation;
