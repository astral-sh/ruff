use indexmap::IndexMap;

pub(super) mod google;
pub(super) mod rst;

/// Encapsulates the set of docstring formats for which we support the following
/// features:
///
/// 1. Extracting parameter documentation for signature help.
/// 2. Rendering as Markdown on hover.
pub(super) struct Formats {
    rst: rst::Docstring,
    google: google::Docstring,
}

impl Formats {
    /// Parses all supported formats in the given docstring.
    pub(super) fn parse(raw: &str) -> Self {
        Self {
            rst: rst::Docstring::parse(raw),
            google: google::Docstring::parse(&super::documentation_trim(raw)),
        }
    }

    /// Returns parameter docs parsed from all supported formats.
    pub(super) fn parameter_documentation(&self) -> IndexMap<String, String> {
        let mut parameters = self.rst.parameter_documentation();
        for (name, description) in self.google.parameter_documentation() {
            parameters.entry(name).or_insert(description);
        }
        parameters
    }

    /// Returns the outcome of parsing the reStructuredText format.
    pub(super) fn rst(&self) -> &rst::Docstring {
        &self.rst
    }
}
