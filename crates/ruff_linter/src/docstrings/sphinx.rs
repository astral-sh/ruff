//! Abstractions for Sphinx-style docstrings.

use crate::docstrings::sections::SectionKind;

pub(crate) static SPHINX_SECTIONS: &[SectionKind] = &[
    SectionKind::Param,
    SectionKind::Type,
    SectionKind::Raises,
    SectionKind::Return,
    SectionKind::RType,
    SectionKind::Yield,
];
