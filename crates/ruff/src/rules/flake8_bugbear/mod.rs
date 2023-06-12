//! Rules from [flake8-bugbear](https://pypi.org/project/flake8-bugbear/).
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

    #[test_case(Rule::AbstractBaseClassWithoutAbstractMethod, Path::new("B024.py"))]
    #[test_case(Rule::AssertFalse, Path::new("B011.py"))]
    #[test_case(Rule::AssertRaisesException, Path::new("B017.py"))]
    #[test_case(Rule::AssignmentToOsEnviron, Path::new("B003.py"))]
    #[test_case(Rule::CachedInstanceMethod, Path::new("B019.py"))]
    #[test_case(Rule::RaiseLiteral, Path::new("B016.py"))]
    #[test_case(Rule::DuplicateHandlerException, Path::new("B014.py"))]
    #[test_case(Rule::DuplicateTryBlockException, Path::new("B025.py"))]
    #[test_case(Rule::DuplicateValue, Path::new("B033.py"))]
    #[test_case(Rule::EmptyMethodWithoutAbstractDecorator, Path::new("B027.py"))]
    #[test_case(Rule::EmptyMethodWithoutAbstractDecorator, Path::new("B027.pyi"))]
    #[test_case(Rule::ExceptWithEmptyTuple, Path::new("B029.py"))]
    #[test_case(Rule::ExceptWithNonExceptionClasses, Path::new("B030.py"))]
    #[test_case(Rule::FStringDocstring, Path::new("B021.py"))]
    #[test_case(Rule::FunctionCallInDefaultArgument, Path::new("B006_B008.py"))]
    #[test_case(Rule::FunctionUsesLoopVariable, Path::new("B023.py"))]
    #[test_case(Rule::GetAttrWithConstant, Path::new("B009_B010.py"))]
    #[test_case(Rule::JumpStatementInFinally, Path::new("B012.py"))]
    #[test_case(Rule::LoopVariableOverridesIterator, Path::new("B020.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_B008.py"))]
    #[test_case(Rule::NoExplicitStacklevel, Path::new("B028.py"))]
    #[test_case(Rule::RaiseWithoutFromInsideExcept, Path::new("B904.py"))]
    #[test_case(Rule::RedundantTupleInExceptionHandler, Path::new("B013.py"))]
    #[test_case(Rule::ReuseOfGroupbyGenerator, Path::new("B031.py"))]
    #[test_case(Rule::SetAttrWithConstant, Path::new("B009_B010.py"))]
    #[test_case(Rule::StarArgUnpackingAfterKeywordArg, Path::new("B026.py"))]
    #[test_case(Rule::StripWithMultiCharacters, Path::new("B005.py"))]
    #[test_case(Rule::UnaryPrefixIncrement, Path::new("B002.py"))]
    #[test_case(Rule::UnintentionalTypeAnnotation, Path::new("B032.py"))]
    #[test_case(Rule::UnreliableCallableCheck, Path::new("B004.py"))]
    #[test_case(Rule::UnusedLoopControlVariable, Path::new("B007.py"))]
    #[test_case(Rule::UselessComparison, Path::new("B015.py"))]
    #[test_case(Rule::UselessContextlibSuppress, Path::new("B022.py"))]
    #[test_case(Rule::UselessExpression, Path::new("B018.py"))]
    #[test_case(Rule::ZipWithoutExplicitStrict, Path::new("B905.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
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
                ..Settings::for_rules(vec![Rule::FunctionCallInDefaultArgument])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
