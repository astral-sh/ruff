use super::black::Black;
use super::isort::Isort;
use super::pep621::Project;

#[derive(Default)]
pub struct ExternalConfig<'a> {
    pub black: Option<&'a Black>,
    pub isort: Option<&'a Isort>,
    pub project: Option<&'a Project>,
}
