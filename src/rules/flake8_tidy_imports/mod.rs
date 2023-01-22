//! Rules from [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/).
pub mod options;

pub mod banned_api;
pub mod relative_imports;

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub ban_relative_imports: relative_imports::Settings,
    pub banned_api: banned_api::Settings,
}
