//! Rules from [flake8-quotes](https://pypi.org/project/flake8-quotes/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;
    use ruff_python_ast::PythonVersion;

    use super::settings::Quote;

    #[test_case(Path::new("doubles.py"))]
    #[test_case(Path::new("doubles_escaped.py"))]
    #[test_case(Path::new("doubles_escaped_unnecessary.py"))]
    #[test_case(Path::new("doubles_implicit.py"))]
    #[test_case(Path::new("doubles_multiline_string.py"))]
    #[test_case(Path::new("doubles_noqa.py"))]
    #[test_case(Path::new("doubles_wrapped.py"))]
    #[test_case(Path::new("doubles_would_be_triple_quotes.py"))]
    fn require_singles(path: &Path) -> Result<()> {
        let snapshot = format!("require_singles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidableEscapedQuote,
                    Rule::UnnecessaryEscapedQuote,
                ])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn require_singles_over_doubles_escaped_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_quotes/doubles_escaped.py"),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rule(Rule::AvoidableEscapedQuote)
                    .with_target_version(PythonVersion::PY311)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn require_doubles_over_singles_escaped_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_quotes/singles_escaped.py"),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Double,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rule(Rule::AvoidableEscapedQuote)
                    .with_target_version(PythonVersion::PY311)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Path::new("singles.py"))]
    #[test_case(Path::new("singles_escaped.py"))]
    #[test_case(Path::new("singles_escaped_unnecessary.py"))]
    #[test_case(Path::new("singles_implicit.py"))]
    #[test_case(Path::new("singles_multiline_string.py"))]
    #[test_case(Path::new("singles_noqa.py"))]
    #[test_case(Path::new("singles_wrapped.py"))]
    #[test_case(Path::new("singles_would_be_triple_quotes.py"))]
    fn require_doubles(path: &Path) -> Result<()> {
        let snapshot = format!("require_doubles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Double,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidableEscapedQuote,
                    Rule::UnnecessaryEscapedQuote,
                ])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
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
    #[test_case(Path::new("docstring_singles_mixed_quotes_module_singleline_var_1.py"))]
    #[test_case(Path::new("docstring_singles_mixed_quotes_module_singleline_var_2.py"))]
    #[test_case(Path::new("docstring_singles_mixed_quotes_class_var_1.py"))]
    #[test_case(Path::new("docstring_singles_mixed_quotes_class_var_2.py"))]
    fn require_docstring_doubles(path: &Path) -> Result<()> {
        let snapshot = format!("require_docstring_doubles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidableEscapedQuote,
                    Rule::UnnecessaryEscapedQuote,
                ])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
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
    #[test_case(Path::new("docstring_doubles_mixed_quotes_module_singleline_var_1.py"))]
    #[test_case(Path::new("docstring_doubles_mixed_quotes_module_singleline_var_2.py"))]
    #[test_case(Path::new("docstring_doubles_mixed_quotes_class_var_1.py"))]
    #[test_case(Path::new("docstring_doubles_mixed_quotes_class_var_2.py"))]
    fn require_docstring_singles(path: &Path) -> Result<()> {
        let snapshot = format!("require_docstring_singles_over_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![
                    Rule::BadQuotesInlineString,
                    Rule::BadQuotesMultilineString,
                    Rule::BadQuotesDocstring,
                    Rule::AvoidableEscapedQuote,
                    Rule::UnnecessaryEscapedQuote,
                ])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("doubles_all.py"))]
    fn only_inline(path: &Path) -> Result<()> {
        let snapshot = format!("only_inline_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![Rule::BadQuotesInlineString])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("doubles_all.py"))]
    fn only_multiline(path: &Path) -> Result<()> {
        let snapshot = format!("only_multiline_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![Rule::BadQuotesMultilineString])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("doubles_all.py"))]
    fn only_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("only_docstring_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_quotes").join(path).as_path(),
            &LinterSettings {
                flake8_quotes: super::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..LinterSettings::for_rules(vec![Rule::BadQuotesDocstring])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
