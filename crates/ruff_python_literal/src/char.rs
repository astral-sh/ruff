use icu_properties::props::{EnumeratedProperty, GeneralCategory};

/// According to python following categories aren't printable:
/// * Cc (Other, Control)
/// * Cf (Other, Format)
/// * Cs (Other, Surrogate)
/// * Co (Other, Private Use)
/// * Cn (Other, Not Assigned)
/// * Zl Separator, Line ('\u2028', LINE SEPARATOR)
/// * Zp Separator, Paragraph ('\u2029', PARAGRAPH SEPARATOR)
/// * Zs (Separator, Space) other than ASCII space('\x20').
pub(crate) fn is_printable(c: char) -> bool {
    let cat = GeneralCategory::for_char(c);

    !matches!(
        cat,
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::Surrogate
            | GeneralCategory::PrivateUse
            | GeneralCategory::Unassigned
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
            | GeneralCategory::SpaceSeparator
    )
}

/// Returns whether the character has a Unicode [mark category] (Mn, Mc, or Me).
/// Characters in these categories are called [combining marks].
///
/// [mark category]: https://www.unicode.org/reports/tr44/#General_Category_Values
/// [combining marks]: https://www.unicode.org/glossary/#combining_character
pub(crate) fn is_combining_mark(c: char) -> bool {
    matches!(
        GeneralCategory::for_char(c),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}
