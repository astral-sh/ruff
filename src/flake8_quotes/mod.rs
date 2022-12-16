pub mod checks;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::flake8_quotes::settings::Quote;
    use crate::linter::test_path;
    use crate::{flake8_quotes, Settings};

    #[test_case(Path::new("doubles.py"))]
    #[test_case(Path::new("doubles_escaped.py"))]
    #[test_case(Path::new("doubles_multiline_string.py"))]
    #[test_case(Path::new("doubles_noqa.py"))]
    #[test_case(Path::new("doubles_wrapped.py"))]
    fn doubles(path: &Path) -> Result<()> {
        let snapshot = format!("doubles_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("singles.py"))]
    #[test_case(Path::new("singles_escaped.py"))]
    #[test_case(Path::new("singles_multiline_string.py"))]
    #[test_case(Path::new("singles_noqa.py"))]
    #[test_case(Path::new("singles_wrapped.py"))]
    fn singles(path: &Path) -> Result<()> {
        let snapshot = format!("singles_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Double,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("docstring_doubles.py"))]
    #[test_case(Path::new("docstring_doubles_module_multiline.py"))]
    #[test_case(Path::new("docstring_doubles_module_singleline.py"))]
    #[test_case(Path::new("docstring_doubles_class.py"))]
    #[test_case(Path::new("docstring_doubles_function.py"))]
    #[test_case(Path::new("docstring_singles.py"))]
    #[test_case(Path::new("docstring_singles_module_multiline.py"))]
    #[test_case(Path::new("docstring_singles_module_singleline.py"))]
    #[test_case(Path::new("docstring_singles_class.py"))]
    #[test_case(Path::new("docstring_singles_function.py"))]
    fn double_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("double_docstring_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("docstring_doubles.py"))]
    #[test_case(Path::new("docstring_doubles_module_multiline.py"))]
    #[test_case(Path::new("docstring_doubles_module_singleline.py"))]
    #[test_case(Path::new("docstring_doubles_class.py"))]
    #[test_case(Path::new("docstring_doubles_function.py"))]
    #[test_case(Path::new("docstring_singles.py"))]
    #[test_case(Path::new("docstring_singles_module_multiline.py"))]
    #[test_case(Path::new("docstring_singles_module_singleline.py"))]
    #[test_case(Path::new("docstring_singles_class.py"))]
    #[test_case(Path::new("docstring_singles_function.py"))]
    fn single_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("single_docstring_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
