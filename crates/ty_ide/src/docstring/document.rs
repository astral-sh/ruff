use indexmap::IndexMap;
use strum_macros::EnumIter;

/// Google-style docstring parsing.
mod google;
pub(super) mod preformatted;
pub(super) mod rst;
/// Syntax utilities shared by docstring format parsers and renderers.
pub(in crate::docstring) mod syntax;

/// Returns docs for all parameters recognized in the given docstring.
///
/// `normalized_source` must have already undergone PEP-257 trimming and universal newline
/// normalization.
pub(super) fn parameter_documentation(
    normalized_source: &str,
    numpy_parameters: IndexMap<String, String>,
) -> IndexMap<String, String> {
    let mut parameters = google::parameter_documentation(normalized_source);
    parameters.extend(numpy_parameters);
    parameters.extend(rst::parameter_documentation(normalized_source));
    parameters
}

/// Canonical docstring sections shared by supported formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
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
