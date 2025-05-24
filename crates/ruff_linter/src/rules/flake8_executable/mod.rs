//! Rules from [flake8-executable](https://pypi.org/project/flake8-executable/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(any(unix, windows))]
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::test::{test_path, test_resource_path};
    use crate::{
        assert_messages, registry, settings::rule_table::RuleTable, settings::LinterSettings,
        RuleSelector,
    };
    use anyhow::Result;
    use test_case::test_matrix;

    fn has_pyproject_toml(pyproject_toml: bool) -> PathBuf {
        let location = if pyproject_toml {
            test_resource_path("fixtures").join("flake8_executable")
        } else {
            test_resource_path("fixtures")
        };
        assert_eq!(
            location.join("pyproject.toml").exists(),
            pyproject_toml,
            "Error setting up a project_root with(out) pyproject.toml"
        );
        location
    }

    #[cfg_attr(
        all(unix, not(test_environment = "ntfs")),
        test_matrix(
            ["EXE001_1.py", "EXE001_2.py", "EXE001_3.py",
            "EXE002_1.py", "EXE002_2.py", "EXE002_3.py",
            "EXE003.py", "EXE003_uv.py",
            "EXE004_1.py", "EXE004_2.py", "EXE004_3.py", "EXE004_4.py",
            "EXE005_1.py", "EXE005_2.py", "EXE005_3.py"],
            [true, false]
        )
    )]
    #[cfg_attr(
        any(windows, all(unix, test_environment = "ntfs")),
        test_matrix(
            ["EXE001_1_wsl.py", "EXE001_2.py", "EXE001_3.py",
            "EXE002_1_wsl.py", "EXE002_2.py", "EXE002_3.py",
            "EXE003.py", "EXE003_uv.py",
            "EXE004_1.py", "EXE004_2.py", "EXE004_3.py", "EXE004_4.py",
            "EXE005_1.py", "EXE005_2.py", "EXE005_3.py"],
            [true, false]
        )
    )]
    fn rules(filename: &str, with_pyproject_toml: bool) -> Result<()> {
        let path = Path::new(filename);
        let snapshot = path.to_string_lossy().into_owned();

        let rules: RuleTable = RuleSelector::Linter(registry::Linter::Flake8Executable)
            .all_rules()
            .collect();
        let settings = LinterSettings {
            rules,
            project_root: has_pyproject_toml(with_pyproject_toml),
            ..LinterSettings::default()
        };

        let diagnostics = test_path(
            Path::new("flake8_executable").join(path).as_path(),
            &settings,
        )?;

        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
