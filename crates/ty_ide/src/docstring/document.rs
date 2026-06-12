use indexmap::IndexMap;

pub(in crate::docstring) mod google;
pub(in crate::docstring) mod numpy;
pub(super) mod preformatted;
pub(super) mod rst;
/// Syntax utilities shared by docstring format parsers and renderers.
pub(in crate::docstring) mod syntax;

/// Returns docs for all parameters recognized in the given docstring.
pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let normalized = super::documentation_trim(raw);
    // Parse Google sections before PEP 257 normalization because normalization can erase the
    // indentation that distinguishes a nested section from a sibling. For example,
    // `Note:\n        context\n\n    Args:\n        value: docs` contains `Args` inside `Note`,
    // even though normalization aligns both headings.
    let mut parameters = google::parameter_documentation(raw);
    parameters.extend(numpy::parameter_documentation(&normalized));
    parameters.extend(rst::parameter_documentation(raw));
    parameters
}

/// Canonical docstring sections shared by supported formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::EnumIter)]
pub(in crate::docstring) enum SectionKind {
    /// Function or method parameters.
    Parameters,
    /// Keyword arguments documented separately from the main parameter section.
    KeywordArguments,
    /// Less commonly used parameters listed separately from the main parameter section.
    OtherParameters,
    /// Class or module attributes.
    Attributes,
    /// A returned value.
    Returns,
    /// A yielded value.
    Yields,
    /// Exceptions raised by a callable.
    Raises,
}

impl SectionKind {
    /// Returns the canonical display heading for this section.
    pub(super) const fn heading(self) -> &'static str {
        match self {
            SectionKind::Parameters => "Parameters",
            SectionKind::KeywordArguments => "Keyword Arguments",
            SectionKind::OtherParameters => "Other Parameters",
            SectionKind::Attributes => "Attributes",
            SectionKind::Returns => "Returns",
            SectionKind::Yields => "Yields",
            SectionKind::Raises => "Raises",
        }
    }
}
