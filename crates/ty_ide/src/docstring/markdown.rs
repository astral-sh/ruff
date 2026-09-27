mod general;
mod structured;

use super::DocstringFragment;

/// Render Markdown for a source docstring.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(super) fn render(source: &str) -> String {
    let mut output = String::new();
    structured::render_into(&mut output, source);
    output
}

impl DocstringFragment {
    pub(super) fn render_markdown(&self) -> String {
        let mut output = String::new();
        general::render_fragment_into(&mut output, &self.0);
        output
    }
}
