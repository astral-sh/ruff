//! Rules from [flake8-executable](https://pypi.org/project/flake8-executable/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Path::new("EXE001_1.py"); "EXE001_1")]
    #[test_case(Path::new("EXE001_2.py"); "EXE001_2")]
    #[test_case(Path::new("EXE001_3.py"); "EXE001_3")]
    #[test_case(Path::new("EXE002_1.py"); "EXE002_1")]
    #[test_case(Path::new("EXE002_2.py"); "EXE002_2")]
    #[test_case(Path::new("EXE002_3.py"); "EXE002_3")]
    #[test_case(Path::new("EXE003.py"); "EXE003")]
    #[test_case(Path::new("EXE004_1.py"); "EXE004_1")]
    #[test_case(Path::new("EXE004_2.py"); "EXE004_2")]
    #[test_case(Path::new("EXE004_3.py"); "EXE004_3")]
    #[test_case(Path::new("EXE005_1.py"); "EXE005_1")]
    #[test_case(Path::new("EXE005_2.py"); "EXE005_2")]
    #[test_case(Path::new("EXE005_3.py"); "EXE005_3")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_executable").join(path).as_path(),
            &settings::Settings::for_rules(vec![
                Rule::ShebangNotExecutable,
                Rule::ShebangMissingExecutableFile,
                Rule::ShebangWhitespace,
                Rule::ShebangNewline,
                Rule::ShebangPython,
            ]),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
