use super::black::Black;
use super::isort::Isort;

#[derive(Default)]
pub struct ExternalConfig<'a> {
    pub black: Option<&'a Black>,
    pub isort: Option<&'a Isort>,
}
