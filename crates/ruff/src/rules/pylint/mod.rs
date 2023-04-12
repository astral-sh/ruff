//! Rules from [Pylint](https://pypi.org/project/pylint/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assert_messages;
    use anyhow::Result;

    use regex::Regex;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pylint;
    use crate::settings::types::PythonVersion;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::AwaitOutsideAsync, Path::new("await_outside_async.py"); "PLE1142")]
    #[test_case(Rule::AssertOnStringLiteral, Path::new("assert_on_string_literal.py"); "PLW0129")]
    #[test_case(Rule::BadStrStripCall, Path::new("bad_str_strip_call.py"); "PLE01310")]
    #[test_case(Rule::BadStringFormatType, Path::new("bad_string_format_type.py"); "PLE1307")]
    #[test_case(Rule::BidirectionalUnicode, Path::new("bidirectional_unicode.py"); "PLE2502")]
    #[test_case(Rule::BinaryOpException, Path::new("binary_op_exception.py"); "PLW0711")]
    #[test_case(Rule::CollapsibleElseIf, Path::new("collapsible_else_if.py"); "PLR5501")]
    #[test_case(Rule::CompareToEmptyString, Path::new("compare_to_empty_string.py"); "PLC1901")]
    #[test_case(Rule::ComparisonOfConstant, Path::new("comparison_of_constant.py"); "PLR0133")]
    #[test_case(Rule::RepeatedIsinstanceCalls, Path::new("repeated_isinstance_calls.py"); "PLR1701")]
    #[test_case(Rule::ManualFromImport, Path::new("import_aliasing.py"); "PLR0402")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_0.py"); "PLR1722_0")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_1.py"); "PLR1722_1")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_2.py"); "PLR1722_2")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_3.py"); "PLR1722_3")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_4.py"); "PLR1722_4")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_5.py"); "PLR1722_5")]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_6.py"); "PLR1722_6")]
    #[test_case(Rule::ContinueInFinally, Path::new("continue_in_finally.py"); "PLE0116")]
    #[test_case(Rule::GlobalStatement, Path::new("global_statement.py"); "PLW0603")]
    #[test_case(Rule::GlobalVariableNotAssigned, Path::new("global_variable_not_assigned.py"); "PLW0602")]
    #[test_case(Rule::InvalidAllFormat, Path::new("invalid_all_format.py"); "PLE0605")]
    #[test_case(Rule::InvalidAllObject, Path::new("invalid_all_object.py"); "PLE0604")]
    #[test_case(Rule::InvalidCharacterBackspace, Path::new("invalid_characters.py"); "PLE2510")]
    #[test_case(Rule::InvalidCharacterEsc, Path::new("invalid_characters.py"); "PLE2513")]
    #[test_case(Rule::InvalidCharacterNul, Path::new("invalid_characters.py"); "PLE2514")]
    #[test_case(Rule::InvalidCharacterSub, Path::new("invalid_characters.py"); "PLE2512")]
    #[test_case(Rule::InvalidCharacterZeroWidthSpace, Path::new("invalid_characters.py"); "PLE2515")]
    #[test_case(Rule::InvalidEnvvarDefault, Path::new("invalid_envvar_default.py"); "PLW1508")]
    #[test_case(Rule::InvalidEnvvarValue, Path::new("invalid_envvar_value.py"); "PLE1507")]
    #[test_case(Rule::LoggingTooFewArgs, Path::new("logging_too_few_args.py"); "PLE1206")]
    #[test_case(Rule::LoggingTooManyArgs, Path::new("logging_too_many_args.py"); "PLE1205")]
    #[test_case(Rule::MagicValueComparison, Path::new("magic_value_comparison.py"); "PLR2004")]
    #[test_case(Rule::NonlocalWithoutBinding, Path::new("nonlocal_without_binding.py"); "PLE0117")]
    #[test_case(Rule::PropertyWithParameters, Path::new("property_with_parameters.py"); "PLR0206")]
    #[test_case(Rule::RedefinedLoopName, Path::new("redefined_loop_name.py"); "PLW2901")]
    #[test_case(Rule::ReturnInInit, Path::new("return_in_init.py"); "PLE0101")]
    #[test_case(Rule::TooManyArguments, Path::new("too_many_arguments.py"); "PLR0913")]
    #[test_case(Rule::TooManyBranches, Path::new("too_many_branches.py"); "PLR0912")]
    #[test_case(Rule::TooManyReturnStatements, Path::new("too_many_return_statements.py"); "PLR0911")]
    #[test_case(Rule::TooManyStatements, Path::new("too_many_statements.py"); "PLR0915")]
    #[test_case(Rule::UnnecessaryDirectLambdaCall, Path::new("unnecessary_direct_lambda_call.py"); "PLC3002")]
    #[test_case(Rule::LoadBeforeGlobalDeclaration, Path::new("load_before_global_declaration.py"); "PLE0118")]
    #[test_case(Rule::UselessElseOnLoop, Path::new("useless_else_on_loop.py"); "PLW0120")]
    #[test_case(Rule::UselessImportAlias, Path::new("import_aliasing.py"); "PLC0414")]
    #[test_case(Rule::UselessReturn, Path::new("useless_return.py"); "PLR1711")]
    #[test_case(Rule::YieldInInit, Path::new("yield_in_init.py"); "PLE0100")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pylint").join(path).as_path(),
            &Settings::for_rules(vec![rule_code]),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn continue_in_finally() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/continue_in_finally.py"),
            &Settings {
                target_version: PythonVersion::Py37,
                ..Settings::for_rules(vec![Rule::ContinueInFinally])
            },
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
                ..Settings::for_rules(vec![Rule::MagicValueComparison])
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
                ..Settings::for_rules(vec![Rule::TooManyArguments])
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
                ..Settings::for_rules(vec![Rule::TooManyArguments])
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
                ..Settings::for_rules(vec![Rule::TooManyBranches])
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
                ..Settings::for_rules(vec![Rule::TooManyStatements])
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
                ..Settings::for_rules(vec![Rule::TooManyReturnStatements])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
