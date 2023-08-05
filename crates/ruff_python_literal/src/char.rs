use unic_ucd_category::GeneralCategory;

/// According to python following categories aren't printable:
/// * Cc (Other, Control)
/// * Cf (Other, Format)
/// * Cs (Other, Surrogate)
/// * Co (Other, Private Use)
/// * Cn (Other, Not Assigned)
/// * Zl Separator, Line ('\u2028', LINE SEPARATOR)
/// * Zp Separator, Paragraph ('\u2029', PARAGRAPH SEPARATOR)
/// * Zs (Separator, Space) other than ASCII space('\x20').
pub fn is_printable(c: char) -> bool {
    let cat = GeneralCategory::of(c);
    !(cat.is_other() || cat.is_separator())
}
