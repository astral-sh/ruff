//! Rules from [flake8-bugbear](https://pypi.org/project/flake8-bugbear/).
pub(crate) mod helpers;
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

    #[test_case(Rule::AbstractBaseClassWithoutAbstractMethod, Path::new("B024.py"))]
    #[test_case(Rule::AssertFalse, Path::new("B011.py"))]
    #[test_case(Rule::AssertRaisesException, Path::new("B017.py"))]
    #[test_case(Rule::AssignmentToOsEnviron, Path::new("B003.py"))]
    #[test_case(Rule::CachedInstanceMethod, Path::new("B019.py"))]
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
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_1.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_2.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_3.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_4.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_5.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_6.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_7.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_8.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_B008.py"))]
    #[test_case(Rule::NoExplicitStacklevel, Path::new("B028.py"))]
    #[test_case(Rule::RaiseLiteral, Path::new("B016.py"))]
    #[test_case(Rule::RaiseWithoutFromInsideExcept, Path::new("B904.py"))]
    #[test_case(Rule::ReSubPositionalArgs, Path::new("B034.py"))]
    #[test_case(Rule::RedundantTupleInExceptionHandler, Path::new("B013.py"))]
    #[test_case(Rule::ReuseOfGroupbyGenerator, Path::new("B031.py"))]
    #[test_case(Rule::SetAttrWithConstant, Path::new("B009_B010.py"))]
    #[test_case(Rule::StarArgUnpackingAfterKeywordArg, Path::new("B026.py"))]
    #[test_case(Rule::StaticKeyDictComprehension, Path::new("B035.py"))]
    #[test_case(Rule::StripWithMultiCharacters, Path::new("B005.py"))]
    #[test_case(Rule::UnaryPrefixIncrementDecrement, Path::new("B002.py"))]
    #[test_case(Rule::UnintentionalTypeAnnotation, Path::new("B032.py"))]
    #[test_case(Rule::UnreliableCallableCheck, Path::new("B004.py"))]
    #[test_case(Rule::UnusedLoopControlVariable, Path::new("B007.py"))]
    #[test_case(Rule::UselessComparison, Path::new("B015.ipynb"))]
    #[test_case(Rule::UselessComparison, Path::new("B015.py"))]
    #[test_case(Rule::UselessContextlibSuppress, Path::new("B022.py"))]
    #[test_case(Rule::UselessExpression, Path::new("B018.ipynb"))]
    #[test_case(Rule::UselessExpression, Path::new("B018.py"))]
    #[test_case(Rule::ReturnInGenerator, Path::new("B901.py"))]
    #[test_case(Rule::LoopIteratorMutation, Path::new("B909.py"))]
    #[test_case(Rule::MutableContextvarDefault, Path::new("B039.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn zip_without_explicit_strict() -> Result<()> {
        let snapshot = "B905.py";
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(snapshot).as_path(),
            &LinterSettings::for_rule(Rule::ZipWithoutExplicitStrict),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls_arg_annotation() -> Result<()> {
        let snapshot = "extend_immutable_calls_arg_annotation".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B006_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec![
                        "custom.ImmutableTypeA".to_string(),
                        "custom.ImmutableTypeB".to_string(),
                    ],
                },
                ..LinterSettings::for_rule(Rule::MutableArgumentDefault)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls_arg_default() -> Result<()> {
        let snapshot = "extend_immutable_calls_arg_default".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B008_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                        "custom.ImmutableTypeA".to_string(),
                        "B008_extended.Class".to_string(),
                    ],
                },
                ..LinterSettings::for_rule(Rule::FunctionCallInDefaultArgument)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_mutable_contextvar_default() -> Result<()> {
        let snapshot = "extend_mutable_contextvar_default".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B039_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec!["fastapi.Query".to_string()],
                },
                ..LinterSettings::for_rule(Rule::MutableContextvarDefault)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
