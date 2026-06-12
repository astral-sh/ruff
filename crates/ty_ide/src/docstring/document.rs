use indexmap::IndexMap;

/// Google-style docstring parsing.
pub(in crate::docstring) mod google;
pub(super) mod preformatted;
pub(super) mod rst;
/// Syntax utilities shared by docstring format parsers and renderers.
pub(in crate::docstring) mod syntax;

/// Returns docs for all parameters recognized in the given docstring.
pub(super) fn parameter_documentation(
    raw: &str,
    numpy_parameters: IndexMap<String, String>,
) -> IndexMap<String, String> {
    let mut parameters = google::parameter_documentation(raw);
    parameters.extend(numpy_parameters);
    parameters.extend(rst::parameter_documentation(raw));
    parameters
}

/// Parameter sections shared by supported docstring formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) enum SectionKind {
    /// Function or method parameters.
    Parameters,
    /// Keyword arguments documented separately from the main parameter section.
    KeywordArguments,
    /// Less commonly used parameters listed separately from the main parameter section.
    OtherParameters,
}
