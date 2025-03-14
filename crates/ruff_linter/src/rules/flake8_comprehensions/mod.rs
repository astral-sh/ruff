//! Rules from [flake8-comprehensions](https://pypi.org/project/flake8-comprehensions/).
mod fixes;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::types::PreviewMode;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::UnnecessaryCallAroundSorted, Path::new("C413.py"))]
    #[test_case(Rule::UnnecessaryCollectionCall, Path::new("C408.py"))]
    #[test_case(Rule::UnnecessaryComprehension, Path::new("C416.py"))]
    #[test_case(Rule::UnnecessaryComprehensionInCall, Path::new("C419.py"))]
    #[test_case(Rule::UnnecessaryComprehensionInCall, Path::new("C419_2.py"))]
    #[test_case(Rule::UnnecessaryDictComprehensionForIterable, Path::new("C420.py"))]
    #[test_case(Rule::UnnecessaryDictComprehensionForIterable, Path::new("C420_1.py"))]
    #[test_case(Rule::UnnecessaryDoubleCastOrProcess, Path::new("C414.py"))]
    #[test_case(Rule::UnnecessaryGeneratorDict, Path::new("C402.py"))]
    #[test_case(Rule::UnnecessaryGeneratorList, Path::new("C400.py"))]
    #[test_case(Rule::UnnecessaryGeneratorSet, Path::new("C401.py"))]
    #[test_case(Rule::UnnecessaryListCall, Path::new("C411.py"))]
    #[test_case(Rule::UnnecessaryListComprehensionDict, Path::new("C404.py"))]
    #[test_case(Rule::UnnecessaryListComprehensionSet, Path::new("C403.py"))]
    #[test_case(Rule::UnnecessaryLiteralDict, Path::new("C406.py"))]
    #[test_case(Rule::UnnecessaryLiteralSet, Path::new("C405.py"))]
    #[test_case(Rule::UnnecessaryLiteralWithinDictCall, Path::new("C418.py"))]
    #[test_case(Rule::UnnecessaryLiteralWithinListCall, Path::new("C410.py"))]
    #[test_case(Rule::UnnecessaryLiteralWithinTupleCall, Path::new("C409.py"))]
    #[test_case(Rule::UnnecessaryMap, Path::new("C417.py"))]
    #[test_case(Rule::UnnecessaryMap, Path::new("C417_1.py"))]
    #[test_case(Rule::UnnecessarySubscriptReversal, Path::new("C415.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnnecessaryLiteralWithinTupleCall, Path::new("C409.py"))]
    #[test_case(Rule::UnnecessaryComprehensionInCall, Path::new("C419_1.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &LinterSettings {
                preview: PreviewMode::Enabled,
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnnecessaryCollectionCall, Path::new("C408.py"))]
    fn allow_dict_calls_with_keyword_arguments(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_allow_dict_calls_with_keyword_arguments",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &LinterSettings {
                flake8_comprehensions: super::settings::Settings {
                    allow_dict_calls_with_keyword_arguments: true,
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
