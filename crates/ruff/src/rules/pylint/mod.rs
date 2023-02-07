//! Rules from [Pylint](https://pypi.org/project/pylint/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use test_case::test_case;

    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::rules::pylint;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::UselessImportAlias, Path::new("import_aliasing.py"); "PLC0414")]
    #[test_case(Rule::UnnecessaryDirectLambdaCall, Path::new("unnecessary_direct_lambda_call.py"); "PLC3002")]
    #[test_case(Rule::NonlocalWithoutBinding, Path::new("nonlocal_without_binding.py"); "PLE0117")]
    #[test_case(Rule::UsedPriorGlobalDeclaration, Path::new("used_prior_global_declaration.py"); "PLE0118")]
    #[test_case(Rule::AwaitOutsideAsync, Path::new("await_outside_async.py"); "PLE1142")]
    #[test_case(Rule::ComparisonOfConstant, Path::new("comparison_of_constant.py"); "PLR0133")]
    #[test_case(Rule::PropertyWithParameters, Path::new("property_with_parameters.py"); "PLR0206")]
    #[test_case(Rule::ConsiderUsingFromImport, Path::new("import_aliasing.py"); "PLR0402")]
    #[test_case(Rule::ConsiderMergingIsinstance, Path::new("consider_merging_isinstance.py"); "PLR1701")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_0.py"); "PLR1722_0")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_1.py"); "PLR1722_1")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_2.py"); "PLR1722_2")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_3.py"); "PLR1722_3")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_4.py"); "PLR1722_4")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_5.py"); "PLR1722_5")]
    #[test_case(Rule::ConsiderUsingSysExit, Path::new("consider_using_sys_exit_6.py"); "PLR1722_6")]
    #[test_case(Rule::MagicValueComparison, Path::new("magic_value_comparison.py"); "PLR2004")]
    #[test_case(Rule::UselessElseOnLoop, Path::new("useless_else_on_loop.py"); "PLW0120")]
    #[test_case(Rule::GlobalVariableNotAssigned, Path::new("global_variable_not_assigned.py"); "PLW0602")]
    #[test_case(Rule::InvalidAllFormat, Path::new("invalid_all_format.py"); "PLE0605")]
    #[test_case(Rule::InvalidAllObject, Path::new("invalid_all_object.py"); "PLE0604")]
    #[test_case(Rule::TooManyReturnStatements, Path::new("too_many_return_statements.py"); "PLR0911")]
    #[test_case(Rule::TooManyArguments, Path::new("too_many_arguments.py"); "PLR0913")]
    #[test_case(Rule::TooManyBranches, Path::new("too_many_branches.py"); "PLR0912")]
    #[test_case(Rule::TooManyStatements, Path::new("too_many_statements.py"); "PLR0915")]
    #[test_case(Rule::BidirectionalUnicode, Path::new("bidirectional_unicode.py"); "PLE2502")]
    #[test_case(Rule::BadStrStripCall, Path::new("bad_str_strip_call.py"); "PLE01310")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pylint").join(path).as_path(),
            &Settings::for_rules(vec![rule_code]),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_args_with_dummy_variables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_arguments_params.py"),
            &Settings {
                dummy_variable_rgx: Regex::new(r"skip_.*").unwrap().into(),
                ..Settings::for_rules(vec![Rule::TooManyArguments])
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
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
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
