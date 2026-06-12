use indexmap::IndexMap;

pub(super) mod google;
pub(super) mod numpy;
pub(super) mod rst;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) enum SectionKind {
    Parameters,
    Attributes,
    Returns,
    Yields,
    Raises,
}

/// Encapsulates the set of docstring formats for which we support the following
/// features:
///
/// 1. Extracting parameter documentation for signature help.
/// 2. Rendering as Markdown on hover.
pub(super) struct Formats<'a> {
    rst: rst::Docstring,
    google: google::Docstring<'a>,
    numpy: numpy::Docstring<'a>,
    google_parameter_documentation: IndexMap<String, String>,
    numpy_parameter_documentation: IndexMap<String, String>,
}

impl<'a> Formats<'a> {
    /// Parses all supported formats in the given docstring.
    pub(super) fn parse(raw: &'a str) -> Self {
        let trimmed = super::documentation_trim(raw);
        let google_parameter_documentation =
            google::Docstring::parse(&trimmed).parameter_documentation();
        let numpy_parameter_documentation =
            numpy::Docstring::parse(&trimmed).parameter_documentation();

        Self {
            rst: rst::Docstring::parse(raw),
            google: google::Docstring::parse(raw),
            numpy: numpy::Docstring::parse(raw),
            google_parameter_documentation,
            numpy_parameter_documentation,
        }
    }

    /// Returns parameter docs parsed from all supported formats.
    pub(super) fn parameter_documentation(&self) -> IndexMap<String, String> {
        let mut parameters = self.rst.parameter_documentation();
        for (name, description) in &self.google_parameter_documentation {
            parameters
                .entry(name.clone())
                .or_insert_with(|| description.clone());
        }
        for (name, description) in &self.numpy_parameter_documentation {
            parameters
                .entry(name.clone())
                .or_insert_with(|| description.clone());
        }
        parameters
    }

    /// Returns the outcome of parsing the reStructuredText format.
    pub(super) fn rst(&self) -> &rst::Docstring {
        &self.rst
    }

    pub(super) fn google(&self) -> &google::Docstring<'a> {
        &self.google
    }

    pub(super) fn numpy(&self) -> &numpy::Docstring<'a> {
        &self.numpy
    }
}
