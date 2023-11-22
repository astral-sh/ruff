use super::black::Black;
use super::isort::Isort;
use super::pep621::Project;

#[derive(Default)]
pub(crate) struct ExternalConfig<'a> {
    pub(crate) black: Option<&'a Black>,
    pub(crate) isort: Option<&'a Isort>,
    pub(crate) project: Option<&'a Project>,
}
