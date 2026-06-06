//! Rules from [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;

    use crate::assert_diagnostics;
    use crate::settings::LinterSettings;
    use crate::test::{test_path, test_resource_path};

    #[test_case(Path::new("test_fail_empty"), Path::new("example.py"))]
    #[test_case(
        Path::new("test_fail_nested_tests"),
        Path::new("package/tests/test_foo.py")
    )]
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
        insta::with_settings!({filters => vec![(r"\\", "/")]}, {
            assert_diagnostics!(snapshot, diagnostics);
        });
        Ok(())
    }

    #[test_case(Path::new("tests/test_foo.py"))]
    #[test_case(Path::new("tests/unit/test_bar.py"))]
    fn top_level_tests(filename: &Path) -> Result<()> {
        let project_root =
            test_resource_path("fixtures/flake8_no_pep420/test_pass_top_level_tests");
        let p = project_root.join(filename);
        let diagnostics = test_path(
            p.strip_prefix(test_resource_path("fixtures"))?,
            &LinterSettings {
                project_root: project_root.clone(),
                src: vec![project_root.clone(), project_root.join("src")],
                ..LinterSettings::for_rule(Rule::ImplicitNamespacePackage)
            },
        )?;
        insta::with_settings!({filters => vec![(r"\\", "/")]}, {
            assert_diagnostics!(format!("top_level_tests_{}", filename.display()), diagnostics);
        });
        Ok(())
    }
}
