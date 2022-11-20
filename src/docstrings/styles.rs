use once_cell::sync::Lazy;
use rustc_hash::FxHashSet;

use crate::docstrings::google::{GOOGLE_SECTION_NAMES, LOWERCASE_GOOGLE_SECTION_NAMES};
use crate::docstrings::numpy::{LOWERCASE_NUMPY_SECTION_NAMES, NUMPY_SECTION_NAMES};

pub(crate) enum SectionStyle {
    NumPy,
    Google,
}

impl SectionStyle {
    pub(crate) fn section_names(&self) -> &Lazy<FxHashSet<&'static str>> {
        match self {
            SectionStyle::NumPy => &NUMPY_SECTION_NAMES,
            SectionStyle::Google => &GOOGLE_SECTION_NAMES,
        }
    }

    pub(crate) fn lowercase_section_names(&self) -> &Lazy<FxHashSet<&'static str>> {
        match self {
            SectionStyle::NumPy => &LOWERCASE_NUMPY_SECTION_NAMES,
            SectionStyle::Google => &LOWERCASE_GOOGLE_SECTION_NAMES,
        }
    }
}
