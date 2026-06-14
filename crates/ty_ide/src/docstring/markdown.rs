mod general;
mod structured;

/// Render Markdown for a source docstring.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(super) fn render(source: &str) -> String {
    let mut output = String::new();
    structured::render_into(&mut output, source);
    output
}
