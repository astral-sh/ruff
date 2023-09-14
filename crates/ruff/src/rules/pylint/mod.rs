//! Rules from [Pylint](https://pypi.org/project/pylint/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::rules::pylint;
    use crate::settings::types::PythonVersion;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::AssertOnStringLiteral, Path::new("assert_on_string_literal.py"))]
    #[test_case(Rule::AwaitOutsideAsync, Path::new("await_outside_async.py"))]
    #[test_case(
        Rule::BadStringFormatCharacter,
        Path::new("bad_string_format_character.py")
    )]
    #[test_case(Rule::BadStrStripCall, Path::new("bad_str_strip_call.py"))]
    #[test_case(Rule::BadStringFormatType, Path::new("bad_string_format_type.py"))]
    #[test_case(Rule::BidirectionalUnicode, Path::new("bidirectional_unicode.py"))]
    #[test_case(Rule::BinaryOpException, Path::new("binary_op_exception.py"))]
    #[test_case(Rule::CollapsibleElseIf, Path::new("collapsible_else_if.py"))]
    #[test_case(Rule::CompareToEmptyString, Path::new("compare_to_empty_string.py"))]
    #[test_case(Rule::ComparisonOfConstant, Path::new("comparison_of_constant.py"))]
    #[test_case(
        Rule::RepeatedIsinstanceCalls,
        Path::new("repeated_isinstance_calls.py")
    )]
    #[test_case(Rule::ComparisonWithItself, Path::new("comparison_with_itself.py"))]
    #[test_case(Rule::EqWithoutHash, Path::new("eq_without_hash.py"))]
    #[test_case(Rule::ManualFromImport, Path::new("import_aliasing.py"))]
    #[test_case(Rule::SingleStringSlots, Path::new("single_string_slots.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_0.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_1.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_2.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_3.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_4.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_5.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_6.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_7.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_8.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_9.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_10.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_11.py"))]
    #[test_case(Rule::ContinueInFinally, Path::new("continue_in_finally.py"))]
    #[test_case(Rule::GlobalStatement, Path::new("global_statement.py"))]
    #[test_case(
        Rule::GlobalVariableNotAssigned,
        Path::new("global_variable_not_assigned.py")
    )]
    #[test_case(Rule::ImportSelf, Path::new("import_self/module.py"))]
    #[test_case(Rule::InvalidAllFormat, Path::new("invalid_all_format.py"))]
    #[test_case(Rule::InvalidAllObject, Path::new("invalid_all_object.py"))]
    #[test_case(Rule::InvalidStrReturnType, Path::new("invalid_return_type_str.py"))]
    #[test_case(Rule::DuplicateBases, Path::new("duplicate_bases.py"))]
    #[test_case(Rule::InvalidCharacterBackspace, Path::new("invalid_characters.py"))]
    #[test_case(Rule::InvalidCharacterEsc, Path::new("invalid_characters.py"))]
    #[test_case(Rule::InvalidCharacterNul, Path::new("invalid_characters.py"))]
    #[test_case(Rule::InvalidCharacterSub, Path::new("invalid_characters.py"))]
    #[test_case(
        Rule::InvalidCharacterZeroWidthSpace,
        Path::new("invalid_characters.py")
    )]
    #[test_case(Rule::InvalidEnvvarDefault, Path::new("invalid_envvar_default.py"))]
    #[test_case(Rule::InvalidEnvvarValue, Path::new("invalid_envvar_value.py"))]
    #[test_case(Rule::IterationOverSet, Path::new("iteration_over_set.py"))]
    #[test_case(Rule::LoggingTooFewArgs, Path::new("logging_too_few_args.py"))]
    #[test_case(Rule::LoggingTooManyArgs, Path::new("logging_too_many_args.py"))]
    #[test_case(Rule::MagicValueComparison, Path::new("magic_value_comparison.py"))]
    #[test_case(
        Rule::NamedExprWithoutContext,
        Path::new("named_expr_without_context.py")
    )]
    #[test_case(Rule::NonlocalWithoutBinding, Path::new("nonlocal_without_binding.py"))]
    #[test_case(Rule::PropertyWithParameters, Path::new("property_with_parameters.py"))]
    #[test_case(Rule::RedefinedLoopName, Path::new("redefined_loop_name.py"))]
    #[test_case(Rule::ReturnInInit, Path::new("return_in_init.py"))]
    #[test_case(Rule::TooManyArguments, Path::new("too_many_arguments.py"))]
    #[test_case(Rule::TooManyBranches, Path::new("too_many_branches.py"))]
    #[test_case(
        Rule::TooManyReturnStatements,
        Path::new("too_many_return_statements.py")
    )]
    #[test_case(Rule::TooManyStatements, Path::new("too_many_statements.py"))]
    #[test_case(Rule::TypeBivariance, Path::new("type_bivariance.py"))]
    #[test_case(
        Rule::TypeNameIncorrectVariance,
        Path::new("type_name_incorrect_variance.py")
    )]
    #[test_case(Rule::TypeParamNameMismatch, Path::new("type_param_name_mismatch.py"))]
    #[test_case(
        Rule::UnexpectedSpecialMethodSignature,
        Path::new("unexpected_special_method_signature.py")
    )]
    #[test_case(
        Rule::UnnecessaryDirectLambdaCall,
        Path::new("unnecessary_direct_lambda_call.py")
    )]
    #[test_case(
        Rule::LoadBeforeGlobalDeclaration,
        Path::new("load_before_global_declaration.py")
    )]
    #[test_case(Rule::UselessElseOnLoop, Path::new("useless_else_on_loop.py"))]
    #[test_case(Rule::UselessImportAlias, Path::new("import_aliasing.py"))]
    #[test_case(Rule::UselessReturn, Path::new("useless_return.py"))]
    #[test_case(
        Rule::YieldFromInAsyncFunction,
        Path::new("yield_from_in_async_function.py")
    )]
    #[test_case(Rule::YieldInInit, Path::new("yield_in_init.py"))]
    #[test_case(Rule::NestedMinMax, Path::new("nested_min_max.py"))]
    #[test_case(
        Rule::RepeatedEqualityComparison,
        Path::new("repeated_equality_comparison.py")
    )]
    #[test_case(Rule::SelfAssigningVariable, Path::new("self_assigning_variable.py"))]
    #[test_case(
        Rule::SubprocessPopenPreexecFn,
        Path::new("subprocess_popen_preexec_fn.py")
    )]
    #[test_case(
        Rule::SubprocessRunWithoutCheck,
        Path::new("subprocess_run_without_check.py")
    )]
    #[test_case(Rule::BadDunderMethodName, Path::new("bad_dunder_method_name.py"))]
    #[test_case(Rule::NoSelfUse, Path::new("no_self_use.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pylint").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn repeated_isinstance_calls() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/repeated_isinstance_calls.py"),
            &Settings::for_rule(Rule::RepeatedIsinstanceCalls)
                .with_target_version(PythonVersion::Py39),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn continue_in_finally() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/continue_in_finally.py"),
            &Settings::for_rule(Rule::ContinueInFinally).with_target_version(PythonVersion::Py37),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_magic_value_types() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/magic_value_comparison.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    allow_magic_value_types: vec![pylint::settings::ConstantType::Int],
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MagicValueComparison)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_args() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_arguments_params.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    max_args: 4,
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::TooManyArguments)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_args_with_dummy_variables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_arguments_params.py"),
            &Settings {
                dummy_variable_rgx: Regex::new(r"skip_.*").unwrap(),
                ..Settings::for_rule(Rule::TooManyArguments)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_branches() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_branches_params.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    max_branches: 1,
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::TooManyBranches)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_statements() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_statements_params.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    max_statements: 1,
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::TooManyStatements)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_return_statements() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_return_statements_params.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    max_returns: 1,
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::TooManyReturnStatements)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn too_many_public_methods() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_public_methods.py"),
            &Settings {
                pylint: pylint::settings::Settings {
                    max_public_methods: 7,
                    ..pylint::settings::Settings::default()
                },
                ..Settings::for_rules(vec![Rule::TooManyPublicMethods])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
