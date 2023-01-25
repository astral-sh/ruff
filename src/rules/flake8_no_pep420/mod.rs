//! Rules from [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_yaml_snapshot;
    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings::Settings;

    #[test_case(Path::new("test_pass_init"); "INP001_0")]
    #[test_case(Path::new("test_fail_empty"); "INP001_1")]
    #[test_case(Path::new("test_fail_nonempty"); "INP001_2")]
    #[test_case(Path::new("test_fail_shebang"); "INP001_3")]
    #[test_case(Path::new("test_ignored"); "INP001_4")]
    #[test_case(Path::new("test_pass_namespace_package"); "INP001_5")]
    fn test_flake8_no_pep420(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        // Platform-independent paths
        let p = PathBuf::from(format!(
            "./resources/test/fixtures/flake8_no_pep420/{}/example.py",
            path.display()
        ));
        let diagnostics = test_path(
            p.as_path(),
            &Settings {
                namespace_packages: vec![PathBuf::from(
                    "./resources/test/fixtures/flake8_no_pep420/test_pass_namespace_package",
                )],
                ..Settings::for_rule(Rule::ImplicitNamespacePackage)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
