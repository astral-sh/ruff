use crate::line_width::IndentWidth;

/// Returns `true` if the name should be considered "ambiguous".
pub(super) fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// Return the amount of indentation, expanding tabs to the next multiple of the settings' tab size.
pub(crate) fn expand_indent(line: &str, indent_width: IndentWidth) -> usize {
    let line = line.trim_end_matches(['\n', '\r']);

    let mut indent = 0;
    let tab_size = indent_width.as_usize();
    for c in line.bytes() {
        match c {
            b'\t' => indent += (indent / tab_size) * tab_size + tab_size,
            b' ' => indent += 1,
            _ => break,
        }
    }

    indent
}
