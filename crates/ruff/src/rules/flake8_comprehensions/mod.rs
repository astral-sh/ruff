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
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::UnnecessaryCallAroundSorted, Path::new("C413.py"); "C413")]
    #[test_case(Rule::UnnecessaryCollectionCall, Path::new("C408.py"); "C408")]
    #[test_case(Rule::UnnecessaryComprehension, Path::new("C416.py"); "C416")]
    #[test_case(Rule::UnnecessaryComprehensionAnyAll, Path::new("C419.py"); "C419")]
    #[test_case(Rule::UnnecessaryDoubleCastOrProcess, Path::new("C414.py"); "C414")]
    #[test_case(Rule::UnnecessaryGeneratorDict, Path::new("C402.py"); "C402")]
    #[test_case(Rule::UnnecessaryGeneratorList, Path::new("C400.py"); "C400")]
    #[test_case(Rule::UnnecessaryGeneratorSet, Path::new("C401.py"); "C401")]
    #[test_case(Rule::UnnecessaryListCall, Path::new("C411.py"); "C411")]
    #[test_case(Rule::UnnecessaryListComprehensionDict, Path::new("C404.py"); "C404")]
    #[test_case(Rule::UnnecessaryListComprehensionSet, Path::new("C403.py"); "C403")]
    #[test_case(Rule::UnnecessaryLiteralDict, Path::new("C406.py"); "C406")]
    #[test_case(Rule::UnnecessaryLiteralSet, Path::new("C405.py"); "C405")]
    #[test_case(Rule::UnnecessaryLiteralWithinDictCall, Path::new("C418.py"); "C418")]
    #[test_case(Rule::UnnecessaryLiteralWithinListCall, Path::new("C410.py"); "C410")]
    #[test_case(Rule::UnnecessaryLiteralWithinTupleCall, Path::new("C409.py"); "C409")]
    #[test_case(Rule::UnnecessaryMap, Path::new("C417.py"); "C417")]
    #[test_case(Rule::UnnecessarySubscriptReversal, Path::new("C415.py"); "C415")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnnecessaryCollectionCall, Path::new("C408.py"); "C408")]
    fn allow_dict_calls_with_keyword_arguments(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_allow_dict_calls_with_keyword_arguments",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &Settings {
                flake8_comprehensions: super::settings::Settings {
                    allow_dict_calls_with_keyword_arguments: true,
                },
                ..Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
