//! Rules from [flake8-quotes](https://pypi.org/project/flake8-quotes/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::Quote;
    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Path::new("doubles.py"))]
    #[test_case(Path::new("doubles_escaped.py"))]
    #[test_case(Path::new("doubles_implicit.py"))]
    #[test_case(Path::new("doubles_multiline_string.py"))]
    #[test_case(Path::new("doubles_noqa.py"))]
    #[test_case(Path::new("doubles_wrapped.py"))]
    fn require_singles(path: &Path) -> Result<()> {
        let snapshot = format!("require_singles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &Settings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidQuoteEscape,
                ])
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("singles.py"))]
    #[test_case(Path::new("singles_escaped.py"))]
    #[test_case(Path::new("singles_implicit.py"))]
    #[test_case(Path::new("singles_multiline_string.py"))]
    #[test_case(Path::new("singles_noqa.py"))]
    #[test_case(Path::new("singles_wrapped.py"))]
    fn require_doubles(path: &Path) -> Result<()> {
        let snapshot = format!("require_doubles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &Settings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Double,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidQuoteEscape,
                ])
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
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
    fn require_docstring_doubles(path: &Path) -> Result<()> {
        let snapshot = format!("require_docstring_doubles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &Settings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidQuoteEscape,
                ])
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
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
    fn require_docstring_singles(path: &Path) -> Result<()> {
        let snapshot = format!("require_docstring_singles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &Settings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidQuoteEscape,
                ])
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
