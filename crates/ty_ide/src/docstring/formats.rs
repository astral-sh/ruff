use indexmap::IndexMap;

pub(super) mod rst;

/// Encapsulates the set of docstring formats for which we support the following
/// features:
///
/// 1. Extracting parameter documentation for signature help.
/// 2. Rendering as Markdown on hover.
pub(super) struct Formats {
    rst: rst::Docstring,
}

impl Formats {
    /// Parses all supported formats in the given docstring.
    pub(super) fn parse(raw: &str) -> Self {
        Self {
            rst: rst::Docstring::parse(raw),
        }
    }

    /// Returns docs for all parameters recognized in a parsed docstring.
    pub(super) fn parameter_documentation(&self) -> IndexMap<String, String> {
        self.rst.parameter_documentation()
    }
}
