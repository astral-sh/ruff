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
    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings::Settings;

    #[test_case(Path::new("doubles.py"))]
    #[test_case(Path::new("doubles_escaped.py"))]
    #[test_case(Path::new("doubles_multiline_string.py"))]
    #[test_case(Path::new("doubles_noqa.py"))]
    #[test_case(Path::new("doubles_wrapped.py"))]
    fn doubles(path: &Path) -> Result<()> {
        let snapshot = format!("doubles_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
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
    #[test_case(Path::new("singles_multiline_string.py"))]
    #[test_case(Path::new("singles_noqa.py"))]
    #[test_case(Path::new("singles_wrapped.py"))]
    fn singles(path: &Path) -> Result<()> {
        let snapshot = format!("singles_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
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
    fn double_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("double_docstring_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
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
    fn single_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("single_docstring_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
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
