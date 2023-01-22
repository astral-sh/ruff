//! Rules from [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use test_case::test_case;

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
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_no_pep420")
                .join(path)
                .join("example.py")
                .as_path(),
            &Settings {
                namespace_packages: vec![PathBuf::from(
                    "./resources/test/fixtures/flake8_no_pep420/test_pass_namespace_package",
                )],
                ..Settings::for_rule(Rule::ImplicitNamespacePackage)
            },
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
