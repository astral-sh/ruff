mod general;
mod structured;

use super::{DocstringFragment, documentation_trim};

/// Renders Markdown for a decoded source docstring.
pub(super) fn render(raw: &str) -> String {
    let source = documentation_trim(raw);
    let mut output = String::new();
    structured::render_into(&mut output, raw, &source);
    output
}

impl DocstringFragment {
    pub(super) fn render_markdown(&self) -> String {
        let mut output = String::new();
        general::render_fragment_into(&mut output, &self.0);
        output
    }
}
