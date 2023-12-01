//! Abstractions for NumPy-style docstrings.

use crate::docstrings::sections::SectionKind;

pub(crate) static NUMPY_SECTIONS: &[SectionKind] = &[
    SectionKind::Attributes,
    SectionKind::Examples,
    SectionKind::Methods,
    SectionKind::Notes,
    SectionKind::Raises,
    SectionKind::References,
    SectionKind::Returns,
    SectionKind::SeeAlso,
    SectionKind::Yields,
    // NumPy-only
    SectionKind::ExtendedSummary,
    SectionKind::OtherParams,
    SectionKind::OtherParameters,
    SectionKind::Parameters,
    SectionKind::ShortSummary,
];
