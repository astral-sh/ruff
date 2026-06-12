use indexmap::IndexMap;
use ruff_python_trivia::leading_indentation;
use ruff_text_size::TextSize;

mod google;
pub(super) mod preformatted;
pub(super) mod rst;

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

/// Calculates indentation width, advancing tabs to the next multiple of eight columns.
pub(super) fn indentation(line: &str) -> TextSize {
    TextSize::new(
        leading_indentation(line)
            .bytes()
            .fold(0u32, |column, byte| match byte {
                b'\t' => (column / 8 + 1) * 8,
                _ => column + 1,
            }),
    )
}
