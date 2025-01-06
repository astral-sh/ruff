//! Rules from [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;

    use crate::assert_messages;
    use crate::settings::LinterSettings;
    use crate::test::{test_path, test_resource_path};

    #[test_case(Path::new("test_fail_empty"), Path::new("example.py"))]
    #[test_case(Path::new("test_fail_nonempty"), Path::new("example.py"))]
    #[test_case(Path::new("test_ignored"), Path::new("example.py"))]
    #[test_case(Path::new("test_pass_init"), Path::new("example.py"))]
    #[test_case(Path::new("test_pass_namespace_package"), Path::new("example.py"))]
    #[test_case(Path::new("test_pass_pep723"), Path::new("script.py"))]
    #[test_case(Path::new("test_pass_pyi"), Path::new("example.pyi"))]
    #[test_case(Path::new("test_pass_script"), Path::new("script"))]
    #[test_case(Path::new("test_pass_shebang"), Path::new("example.py"))]
    fn default(path: &Path, filename: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let p = PathBuf::from(format!(
            "flake8_no_pep420/{}/{}",
            path.display(),
            filename.display()
        ));
        let diagnostics = test_path(
            p.as_path(),
            &LinterSettings {
                namespace_packages: vec![test_resource_path(
                    "fixtures/flake8_no_pep420/test_pass_namespace_package",
                )],
                ..LinterSettings::for_rule(Rule::ImplicitNamespacePackage)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
