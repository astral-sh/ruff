//! Abstractions for Google-style docstrings.

use crate::docstrings::sections::SectionKind;

pub(crate) static GOOGLE_SECTIONS: &[SectionKind] = &[
    SectionKind::Attributes,
    SectionKind::Examples,
    SectionKind::Methods,
    SectionKind::Notes,
    SectionKind::Raises,
    SectionKind::References,
    SectionKind::Returns,
    SectionKind::SeeAlso,
    SectionKind::Yields,
    // Google-only
    SectionKind::Args,
    SectionKind::Arguments,
    SectionKind::Attention,
    SectionKind::Caution,
    SectionKind::Danger,
    SectionKind::Error,
    SectionKind::Example,
    SectionKind::Hint,
    SectionKind::Important,
    SectionKind::KeywordArgs,
    SectionKind::KeywordArguments,
    SectionKind::Note,
    SectionKind::Notes,
    SectionKind::OtherArgs,
    SectionKind::OtherArguments,
    SectionKind::Return,
    SectionKind::Tip,
    SectionKind::Todo,
    SectionKind::Warning,
    SectionKind::Warnings,
    SectionKind::Warns,
    SectionKind::Yield,
];
