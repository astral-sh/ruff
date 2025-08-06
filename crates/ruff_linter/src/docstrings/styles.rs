use crate::docstrings::google::GOOGLE_SECTIONS;
use crate::docstrings::numpy::NUMPY_SECTIONS;
use crate::docstrings::sections::SectionKind;

#[derive(Copy, Clone, Debug, is_macro::Is)]
pub(crate) enum SectionStyle {
    Numpy,
    Google,
}

impl SectionStyle {
    pub(crate) fn sections(&self) -> &[SectionKind] {
        match self {
            Self::Numpy => NUMPY_SECTIONS,
            Self::Google => GOOGLE_SECTIONS,
        }
    }
}
