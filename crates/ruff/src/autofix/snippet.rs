use unicode_width::UnicodeWidthStr;

/// A snippet of source code for user-facing display, as in a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceCodeSnippet(String);

impl SourceCodeSnippet {
    pub(crate) fn new(source_code: String) -> Self {
        Self(source_code)
    }

    pub(crate) fn from_str(source_code: &str) -> Self {
        Self(source_code.to_string())
    }

    /// Return the full snippet for user-facing display, or `None` if the snippet should be
    /// truncated.
    pub(crate) fn full_display(&self) -> Option<&str> {
        if Self::should_truncate(&self.0) {
            None
        } else {
            Some(&self.0)
        }
    }

    /// Return a truncated snippet for user-facing display.
    pub(crate) fn truncated_display(&self) -> &str {
        if Self::should_truncate(&self.0) {
            "..."
        } else {
            &self.0
        }
    }

    /// Returns `true` if the source code should be truncated when included in a user-facing
    /// diagnostic.
    fn should_truncate(source_code: &str) -> bool {
        source_code.width() > 50 || source_code.contains(['\r', '\n'])
    }
}
