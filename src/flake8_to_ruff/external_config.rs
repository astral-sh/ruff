use super::{black::Black, isort::Isort};

pub struct ExternalConfig<'a> {
    pub black: Option<&'a Black>,
    pub isort: Option<&'a Isort>,
}

impl<'a> Default for ExternalConfig<'a> {
    fn default() -> Self {
        Self {
            black: None,
            isort: None,
        }
    }
}
