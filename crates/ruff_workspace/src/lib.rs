pub mod configuration;
pub mod options;
pub mod pyproject;
pub mod resolver;

pub mod options_base;
mod settings;

pub use settings::Settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    pub(crate) fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
        Path::new("../ruff_linter/resources/test/").join(path)
    }
}
