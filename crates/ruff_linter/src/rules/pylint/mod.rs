//! Rules from [Pylint](https://pypi.org/project/pylint/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use ruff_python_ast::PythonVersion;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::{flake8_tidy_imports, pylint};

    use crate::assert_messages;
    use crate::settings::types::PreviewMode;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::SingledispatchMethod, Path::new("singledispatch_method.py"))]
    #[test_case(
        Rule::SingledispatchmethodFunction,
        Path::new("singledispatchmethod_function.py")
    )]
    #[test_case(Rule::AssertOnStringLiteral, Path::new("assert_on_string_literal.py"))]
    #[test_case(Rule::AwaitOutsideAsync, Path::new("await_outside_async.py"))]
    #[test_case(Rule::AwaitOutsideAsync, Path::new("await_outside_async.ipynb"))]
    #[test_case(Rule::BadOpenMode, Path::new("bad_open_mode.py"))]
    #[test_case(
        Rule::BadStringFormatCharacter,
        Path::new("bad_string_format_character.py")
    )]
    #[test_case(Rule::BadStrStripCall, Path::new("bad_str_strip_call.py"))]
    #[test_case(Rule::BadStringFormatType, Path::new("bad_string_format_type.py"))]
    #[test_case(Rule::BidirectionalUnicode, Path::new("bidirectional_unicode.py"))]
    #[test_case(Rule::BinaryOpException, Path::new("binary_op_exception.py"))]
    #[test_case(
        Rule::BooleanChainedComparison,
        Path::new("boolean_chained_comparison.py")
    )]
    #[test_case(Rule::CollapsibleElseIf, Path::new("collapsible_else_if.py"))]
    #[test_case(Rule::CompareToEmptyString, Path::new("compare_to_empty_string.py"))]
    #[test_case(Rule::ComparisonOfConstant, Path::new("comparison_of_constant.py"))]
    #[test_case(Rule::ComparisonWithItself, Path::new("comparison_with_itself.py"))]
    #[test_case(Rule::EqWithoutHash, Path::new("eq_without_hash.py"))]
    #[test_case(Rule::EmptyComment, Path::new("empty_comment.py"))]
    #[test_case(Rule::ManualFromImport, Path::new("import_aliasing.py"))]
    #[test_case(Rule::IfStmtMinMax, Path::new("if_stmt_min_max.py"))]
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
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_12.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_13.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_14.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_15.py"))]
    #[test_case(Rule::SysExitAlias, Path::new("sys_exit_alias_16.py"))]
    #[test_case(Rule::ContinueInFinally, Path::new("continue_in_finally.py"))]
    #[test_case(Rule::GlobalStatement, Path::new("global_statement.py"))]
    #[test_case(
        Rule::GlobalVariableNotAssigned,
        Path::new("global_variable_not_assigned.py")
    )]
    #[test_case(Rule::ImportOutsideTopLevel, Path::new("import_outside_top_level.py"))]
    #[test_case(
        Rule::ImportPrivateName,
        Path::new("import_private_name/submodule/__main__.py")
    )]
    #[test_case(Rule::ImportSelf, Path::new("import_self/module.py"))]
    #[test_case(Rule::InvalidAllFormat, Path::new("invalid_all_format.py"))]
    #[test_case(Rule::InvalidAllObject, Path::new("invalid_all_object.py"))]
    #[test_case(Rule::InvalidBoolReturnType, Path::new("invalid_return_type_bool.py"))]
    #[test_case(
        Rule::InvalidBytesReturnType,
        Path::new("invalid_return_type_bytes.py")
    )]
    #[test_case(
        Rule::InvalidIndexReturnType,
        Path::new("invalid_return_type_index.py")
    )]
    #[test_case(Rule::InvalidHashReturnType, Path::new("invalid_return_type_hash.py"))]
    #[test_case(
        Rule::InvalidLengthReturnType,
        Path::new("invalid_return_type_length.py")
    )]
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
    #[test_case(
        Rule::InvalidCharacterBackspace,
        Path::new("invalid_characters_syntax_error.py")
    )]
    #[test_case(Rule::ShallowCopyEnviron, Path::new("shallow_copy_environ.py"))]
    #[test_case(Rule::InvalidEnvvarDefault, Path::new("invalid_envvar_default.py"))]
    #[test_case(Rule::InvalidEnvvarValue, Path::new("invalid_envvar_value.py"))]
    #[test_case(Rule::IterationOverSet, Path::new("iteration_over_set.py"))]
    #[test_case(Rule::LoggingTooFewArgs, Path::new("logging_too_few_args.py"))]
    #[test_case(Rule::LoggingTooManyArgs, Path::new("logging_too_many_args.py"))]
    #[test_case(Rule::MagicValueComparison, Path::new("magic_value_comparison.py"))]
    #[test_case(Rule::ModifiedIteratingSet, Path::new("modified_iterating_set.py"))]
    #[test_case(
        Rule::NamedExprWithoutContext,
        Path::new("named_expr_without_context.py")
    )]
    #[test_case(Rule::NonlocalAndGlobal, Path::new("nonlocal_and_global.py"))]
    #[test_case(
        Rule::RedefinedSlotsInSubclass,
        Path::new("redefined_slots_in_subclass.py")
    )]
    #[test_case(Rule::NonlocalWithoutBinding, Path::new("nonlocal_without_binding.py"))]
    #[test_case(Rule::NonSlotAssignment, Path::new("non_slot_assignment.py"))]
    #[test_case(Rule::PropertyWithParameters, Path::new("property_with_parameters.py"))]
    #[test_case(Rule::RedeclaredAssignedName, Path::new("redeclared_assigned_name.py"))]
    #[test_case(
        Rule::RedefinedArgumentFromLocal,
        Path::new("redefined_argument_from_local.py")
    )]
    #[test_case(Rule::RedefinedLoopName, Path::new("redefined_loop_name.py"))]
    #[test_case(Rule::ReturnInInit, Path::new("return_in_init.py"))]
    #[test_case(Rule::TooManyArguments, Path::new("too_many_arguments.py"))]
    #[test_case(
        Rule::TooManyPositionalArguments,
        Path::new("too_many_positional_arguments.py")
    )]
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
    #[test_case(Rule::UselessWithLock, Path::new("useless_with_lock.py"))]
    #[test_case(Rule::UnreachableCode, Path::new("unreachable.py"))]
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
    #[test_case(Rule::UnspecifiedEncoding, Path::new("unspecified_encoding.py"))]
    #[test_case(Rule::BadDunderMethodName, Path::new("bad_dunder_method_name.py"))]
    #[test_case(Rule::NoSelfUse, Path::new("no_self_use.py"))]
    #[test_case(Rule::MisplacedBareRaise, Path::new("misplaced_bare_raise.py"))]
    #[test_case(Rule::LiteralMembership, Path::new("literal_membership.py"))]
    #[test_case(Rule::GlobalAtModuleLevel, Path::new("global_at_module_level.py"))]
    #[test_case(Rule::UnnecessaryLambda, Path::new("unnecessary_lambda.py"))]
    #[test_case(Rule::NonAsciiImportName, Path::new("non_ascii_module_import.py"))]
    #[test_case(Rule::NonAsciiName, Path::new("non_ascii_name.py"))]
    #[test_case(
        Rule::RepeatedKeywordArgument,
        Path::new("repeated_keyword_argument.py")
    )]
    #[test_case(
        Rule::UnnecessaryListIndexLookup,
        Path::new("unnecessary_list_index_lookup.py")
    )]
    #[test_case(Rule::NoClassmethodDecorator, Path::new("no_method_decorator.py"))]
    #[test_case(Rule::UnnecessaryDunderCall, Path::new("unnecessary_dunder_call.py"))]
    #[test_case(Rule::NoStaticmethodDecorator, Path::new("no_method_decorator.py"))]
    #[test_case(Rule::PotentialIndexError, Path::new("potential_index_error.py"))]
    #[test_case(Rule::SuperWithoutBrackets, Path::new("super_without_brackets.py"))]
    #[test_case(Rule::SelfOrClsAssignment, Path::new("self_or_cls_assignment.py"))]
    #[test_case(Rule::TooManyNestedBlocks, Path::new("too_many_nested_blocks.py"))]
    #[test_case(Rule::DictIndexMissingItems, Path::new("dict_index_missing_items.py"))]
    #[test_case(Rule::DictIterMissingItems, Path::new("dict_iter_missing_items.py"))]
    #[test_case(
        Rule::UnnecessaryDictIndexLookup,
        Path::new("unnecessary_dict_index_lookup.py")
    )]
    #[test_case(Rule::NonAugmentedAssignment, Path::new("non_augmented_assignment.py"))]
    #[test_case(
        Rule::UselessExceptionStatement,
        Path::new("useless_exception_statement.py")
    )]
    #[test_case(Rule::NanComparison, Path::new("nan_comparison.py"))]
    #[test_case(
        Rule::BadStaticmethodArgument,
        Path::new("bad_staticmethod_argument.py")
    )]
    #[test_case(Rule::LenTest, Path::new("len_as_condition.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pylint").join(path).as_path(),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    allow_dunder_method_names: FxHashSet::from_iter([
                        "__special_custom_magic__".to_string()
                    ]),
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn continue_in_finally() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/continue_in_finally.py"),
            &LinterSettings::for_rule(Rule::ContinueInFinally)
                .with_target_version(PythonVersion::PY37),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn allow_magic_value_types() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/magic_value_comparison.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    allow_magic_value_types: vec![pylint::settings::ConstantType::Int],
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MagicValueComparison)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_args() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_arguments_params.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_args: 4,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyArguments)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_args_with_dummy_variables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_arguments_params.py"),
            &LinterSettings {
                dummy_variable_rgx: Regex::new(r"skip_.*").unwrap(),
                ..LinterSettings::for_rule(Rule::TooManyArguments)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_positional_args() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_positional_params.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_positional_args: 4,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyPositionalArguments)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_branches() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_branches_params.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_branches: 1,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyBranches)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_boolean_expressions() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_boolean_expressions.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_bool_expr: 5,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyBooleanExpressions)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_statements() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_statements_params.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_statements: 1,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyStatements)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_return_statements() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_return_statements_params.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_returns: 1,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::TooManyReturnStatements)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn too_many_public_methods() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_public_methods.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_public_methods: 7,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rules(vec![Rule::TooManyPublicMethods])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn too_many_locals() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/too_many_locals.py"),
            &LinterSettings {
                pylint: pylint::settings::Settings {
                    max_locals: 15,
                    ..pylint::settings::Settings::default()
                },
                ..LinterSettings::for_rules(vec![Rule::TooManyLocals])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn import_outside_top_level_with_banned() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pylint/import_outside_top_level_with_banned.py"),
            &LinterSettings {
                preview: PreviewMode::Enabled,
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_module_level_imports: vec![
                        "foo_banned".to_string(),
                        "pkg_banned".to_string(),
                        "pkg.bar_banned".to_string(),
                    ],
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![
                    Rule::BannedModuleLevelImports,
                    Rule::ImportOutsideTopLevel,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
