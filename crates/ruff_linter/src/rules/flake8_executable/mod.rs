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
    use crate::{assert_messages, settings};

    #[test_case(Path::new("EXE001_1.py"))]
    #[test_case(Path::new("EXE001_2.py"))]
    #[test_case(Path::new("EXE001_3.py"))]
    #[test_case(Path::new("EXE002_1.py"))]
    #[test_case(Path::new("EXE002_2.py"))]
    #[test_case(Path::new("EXE002_3.py"))]
    #[test_case(Path::new("EXE003.py"))]
    #[test_case(Path::new("EXE004_1.py"))]
    #[test_case(Path::new("EXE004_2.py"))]
    #[test_case(Path::new("EXE004_3.py"))]
    #[test_case(Path::new("EXE004_4.py"))]
    #[test_case(Path::new("EXE005_1.py"))]
    #[test_case(Path::new("EXE005_2.py"))]
    #[test_case(Path::new("EXE005_3.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_executable").join(path).as_path(),
            &settings::LinterSettings::for_rules(vec![
                Rule::ShebangNotExecutable,
                Rule::ShebangMissingExecutableFile,
                Rule::ShebangLeadingWhitespace,
                Rule::ShebangNotFirstLine,
                Rule::ShebangMissingPython,
            ]),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
