//! Rules from [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::{test_path, test_resource_path};

    #[test_case(Path::new("test_pass_init"), Path::new("example.py"); "INP001_0")]
    #[test_case(Path::new("test_fail_empty"), Path::new("example.py"); "INP001_1")]
    #[test_case(Path::new("test_fail_nonempty"), Path::new("example.py"); "INP001_2")]
    #[test_case(Path::new("test_fail_shebang"), Path::new("example.py"); "INP001_3")]
    #[test_case(Path::new("test_ignored"), Path::new("example.py"); "INP001_4")]
    #[test_case(Path::new("test_pass_namespace_package"), Path::new("example.py"); "INP001_5")]
    #[test_case(Path::new("test_pass_pyi"), Path::new("example.pyi"); "INP001_6")]
    #[test_case(Path::new("test_pass_script"), Path::new("script"); "INP001_7")]
    fn test_flake8_no_pep420(path: &Path, filename: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let p = PathBuf::from(format!(
            "flake8_no_pep420/{}/{}",
            path.display(),
            filename.display()
        ));
        let diagnostics = test_path(
            p.as_path(),
            &Settings {
                namespace_packages: vec![test_resource_path(
                    "fixtures/flake8_no_pep420/test_pass_namespace_package",
                )],
                ..Settings::for_rule(Rule::ImplicitNamespacePackage)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
