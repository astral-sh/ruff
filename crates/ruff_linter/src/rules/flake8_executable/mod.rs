//! Rules from [flake8-executable](https://pypi.org/project/flake8-executable/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(any(unix, windows))]
#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::{test_path, test_resource_path};
    use crate::{assert_diagnostics, settings};

    #[cfg_attr(
        all(unix, not(test_environment = "ntfs")),
        test_case(Path::new("EXE001_1.py"))
    )]
    #[cfg_attr(
        any(windows, test_environment = "ntfs"),
        test_case(Path::new("EXE001_1_ntfs.py"))
    )]
    #[test_case(Path::new("EXE001_2.py"))]
    #[test_case(Path::new("EXE001_3.py"))]
    #[cfg_attr(
        all(unix, not(test_environment = "ntfs")),
        test_case(Path::new("EXE002_1.py"))
    )]
    #[cfg_attr(
        any(windows, test_environment = "ntfs"),
        test_case(Path::new("EXE002_1_ntfs.py"))
    )]
    #[test_case(Path::new("EXE002_2.py"))]
    #[test_case(Path::new("EXE002_3.py"))]
    #[test_case(Path::new("EXE003.py"))]
    #[test_case(Path::new("EXE003_uv.py"))]
    #[test_case(Path::new("EXE003_uv_tool.py"))]
    #[test_case(Path::new("EXE003_uvx.py"))]
    #[test_case(Path::new("EXE004_1.py"))]
    #[test_case(Path::new("EXE004_2.py"))]
    #[test_case(Path::new("EXE004_3.py"))]
    #[test_case(Path::new("EXE004_4.py"))]
    #[test_case(Path::new("EXE005_1.py"))]
    #[test_case(Path::new("EXE005_2.py"))]
    #[test_case(Path::new("EXE005_3.py"))]
    fn rules_no_pyproject_toml(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let settings = settings::LinterSettings::for_rules(vec![
                Rule::ShebangNotExecutable,
                Rule::ShebangMissingExecutableFile,
                Rule::ShebangLeadingWhitespace,
                Rule::ShebangNotFirstLine,
                Rule::ShebangMissingPython,
            ]);
        assert!(!&settings.project_root.join("pyproject.toml").exists(), "unexpected pyproject.toml found: {}", &settings.project_root.join("pyproject.toml").to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_executable").join(path).as_path(),
            &settings,
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[cfg_attr(
        all(unix, not(test_environment = "ntfs")),
        test_case(Path::new("EXE001_1.py"))
    )]
    #[cfg_attr(
        any(windows, test_environment = "ntfs"),
        test_case(Path::new("EXE001_1_ntfs.py"))
    )]
    #[test_case(Path::new("EXE001_2.py"))]
    #[test_case(Path::new("EXE001_3.py"))]
    #[cfg_attr(
        all(unix, not(test_environment = "ntfs")),
        test_case(Path::new("EXE002_1.py"))
    )]
    #[cfg_attr(
        any(windows, test_environment = "ntfs"),
        test_case(Path::new("EXE002_1_ntfs.py"))
    )]
    #[test_case(Path::new("EXE002_2.py"))]
    #[test_case(Path::new("EXE002_3.py"))]
    #[test_case(Path::new("EXE003.py"))]
    #[test_case(Path::new("EXE003_uv.py"))]
    #[test_case(Path::new("EXE003_uv_tool.py"))]
    #[test_case(Path::new("EXE003_uvx.py"))]
    #[test_case(Path::new("EXE004_1.py"))]
    #[test_case(Path::new("EXE004_2.py"))]
    #[test_case(Path::new("EXE004_3.py"))]
    #[test_case(Path::new("EXE004_4.py"))]
    #[test_case(Path::new("EXE005_1.py"))]
    #[test_case(Path::new("EXE005_2.py"))]
    #[test_case(Path::new("EXE005_3.py"))]
    fn rules_with_pyproject_toml(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let mut settings = settings::LinterSettings::for_rules(vec![
                Rule::ShebangNotExecutable,
                Rule::ShebangMissingExecutableFile,
                Rule::ShebangLeadingWhitespace,
                Rule::ShebangNotFirstLine,
                Rule::ShebangMissingPython,
            ]);
        settings.project_root = test_resource_path("fixtures").join("flake8_executable");
        assert!(&settings.project_root.join("pyproject.toml").exists(), "{} not found", &settings.project_root.join("pyproject.toml").to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_executable").join(path).as_path(),
            &settings,
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

}
