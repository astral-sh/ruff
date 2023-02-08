//! Rules from [flake8-bugbear](https://pypi.org/project/flake8-bugbear/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::UnaryPrefixIncrement, Path::new("B002.py"); "B002")]
    #[test_case(Rule::AssignmentToOsEnviron, Path::new("B003.py"); "B003")]
    #[test_case(Rule::UnreliableCallableCheck, Path::new("B004.py"); "B004")]
    #[test_case(Rule::StripWithMultiCharacters, Path::new("B005.py"); "B005")]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_B008.py"); "B006")]
    #[test_case(Rule::UnusedLoopControlVariable, Path::new("B007.py"); "B007")]
    #[test_case(Rule::FunctionCallArgumentDefault, Path::new("B006_B008.py"); "B008")]
    #[test_case(Rule::GetAttrWithConstant, Path::new("B009_B010.py"); "B009")]
    #[test_case(Rule::SetAttrWithConstant, Path::new("B009_B010.py"); "B010")]
    #[test_case(Rule::DoNotAssertFalse, Path::new("B011.py"); "B011")]
    #[test_case(Rule::JumpStatementInFinally, Path::new("B012.py"); "B012")]
    #[test_case(Rule::RedundantTupleInExceptionHandler, Path::new("B013.py"); "B013")]
    #[test_case(Rule::DuplicateHandlerException, Path::new("B014.py"); "B014")]
    #[test_case(Rule::UselessComparison, Path::new("B015.py"); "B015")]
    #[test_case(Rule::CannotRaiseLiteral, Path::new("B016.py"); "B016")]
    #[test_case(Rule::AssertRaisesException, Path::new("B017.py"); "B017")]
    #[test_case(Rule::UselessExpression, Path::new("B018.py"); "B018")]
    #[test_case(Rule::CachedInstanceMethod, Path::new("B019.py"); "B019")]
    #[test_case(Rule::LoopVariableOverridesIterator, Path::new("B020.py"); "B020")]
    #[test_case(Rule::FStringDocstring, Path::new("B021.py"); "B021")]
    #[test_case(Rule::UselessContextlibSuppress, Path::new("B022.py"); "B022")]
    #[test_case(Rule::FunctionUsesLoopVariable, Path::new("B023.py"); "B023")]
    #[test_case(Rule::AbstractBaseClassWithoutAbstractMethod, Path::new("B024.py"); "B024")]
    #[test_case(Rule::DuplicateTryBlockException, Path::new("B025.py"); "B025")]
    #[test_case(Rule::StarArgUnpackingAfterKeywordArg, Path::new("B026.py"); "B026")]
    #[test_case(Rule::EmptyMethodWithoutAbstractDecorator, Path::new("B027.py"); "B027")]
    #[test_case(Rule::RaiseWithoutFromInsideExcept, Path::new("B904.py"); "B904")]
    #[test_case(Rule::ZipWithoutExplicitStrict, Path::new("B905.py"); "B905")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls() -> Result<()> {
        let snapshot = "extend_immutable_calls".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B008_extended.py"),
            &Settings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                    ],
                },
                ..Settings::for_rules(vec![Rule::FunctionCallArgumentDefault])
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
