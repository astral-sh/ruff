//! Rules from [flake8-comprehensions](https://pypi.org/project/flake8-comprehensions/).
mod fixes;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::UnnecessaryGeneratorList, Path::new("C400.py"); "C400")]
    #[test_case(Rule::UnnecessaryGeneratorSet, Path::new("C401.py"); "C401")]
    #[test_case(Rule::UnnecessaryGeneratorDict, Path::new("C402.py"); "C402")]
    #[test_case(Rule::UnnecessaryListComprehensionSet, Path::new("C403.py"); "C403")]
    #[test_case(Rule::UnnecessaryListComprehensionDict, Path::new("C404.py"); "C404")]
    #[test_case(Rule::UnnecessaryLiteralSet, Path::new("C405.py"); "C405")]
    #[test_case(Rule::UnnecessaryLiteralDict, Path::new("C406.py"); "C406")]
    #[test_case(Rule::UnnecessaryCollectionCall, Path::new("C408.py"); "C408")]
    #[test_case(Rule::UnnecessaryLiteralWithinTupleCall, Path::new("C409.py"); "C409")]
    #[test_case(Rule::UnnecessaryLiteralWithinListCall, Path::new("C410.py"); "C410")]
    #[test_case(Rule::UnnecessaryListCall, Path::new("C411.py"); "C411")]
    #[test_case(Rule::UnnecessaryCallAroundSorted, Path::new("C413.py"); "C413")]
    #[test_case(Rule::UnnecessaryDoubleCastOrProcess, Path::new("C414.py"); "C414")]
    #[test_case(Rule::UnnecessarySubscriptReversal, Path::new("C415.py"); "C415")]
    #[test_case(Rule::UnnecessaryComprehension, Path::new("C416.py"); "C416")]
    #[test_case(Rule::UnnecessaryMap, Path::new("C417.py"); "C417")]

    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_comprehensions").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
