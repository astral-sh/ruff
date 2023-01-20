//! Rules from [flake8-executable](https://pypi.org/project/flake8-executable/2.1.1/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings;

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
            Path::new("./resources/test/fixtures/flake8_executable")
                .join(path)
                .as_path(),
            &settings::Settings::for_rules(vec![
                Rule::ShebangWhitespace,
                Rule::ShebangNewline,
                Rule::ShebangPython,
            ]),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
