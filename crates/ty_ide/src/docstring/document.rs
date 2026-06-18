use indexmap::IndexMap;

pub(super) mod preformatted;
pub(super) mod rst;

/// Returns docs for all parameters recognized in the given docstring.
pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    rst::Docstring::parse(raw).parameter_documentation()
}
