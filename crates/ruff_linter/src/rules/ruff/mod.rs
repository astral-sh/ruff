//! Ruff-specific rules.

mod helpers;
pub(crate) mod rules;
pub mod settings;
pub(crate) mod typing;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use ruff_python_ast::PythonVersion;
    use ruff_source_file::SourceFileBuilder;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use crate::pyproject_toml::lint_pyproject_toml;
    use crate::registry::Rule;
    use crate::rules::pydocstyle::settings::Settings as PydocstyleSettings;
    use crate::settings::LinterSettings;
    use crate::settings::types::{CompiledPerFileIgnoreList, PerFileIgnore, PreviewMode};
    use crate::source_kind::SourceKind;
    use crate::test::{test_contents, test_path, test_resource_path, test_snippet};
    use crate::{assert_diagnostics, assert_diagnostics_diff, settings};

    #[test_case(Rule::CollectionLiteralConcatenation, Path::new("RUF005.py"))]
    #[test_case(Rule::CollectionLiteralConcatenation, Path::new("RUF005_slices.py"))]
    #[test_case(Rule::AsyncioDanglingTask, Path::new("RUF006.py"))]
    #[test_case(Rule::ZipInsteadOfPairwise, Path::new("RUF007.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008_attrs.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008_deferred.py"))]
    #[test_case(Rule::FunctionCallInDataclassDefaultArgument, Path::new("RUF009.py"))]
    #[test_case(
        Rule::FunctionCallInDataclassDefaultArgument,
        Path::new("RUF009_attrs.py")
    )]
    #[test_case(
        Rule::FunctionCallInDataclassDefaultArgument,
        Path::new("RUF009_attrs_auto_attribs.py")
    )]
    #[test_case(
        Rule::FunctionCallInDataclassDefaultArgument,
        Path::new("RUF009_deferred.py")
    )]
    #[test_case(Rule::ExplicitFStringTypeConversion, Path::new("RUF010.py"))]
    #[test_case(Rule::MutableClassDefault, Path::new("RUF012.py"))]
    #[test_case(Rule::MutableClassDefault, Path::new("RUF012_deferred.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_0.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_1.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_2.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_3.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_4.py"))]
    #[test_case(
        Rule::UnnecessaryIterableAllocationForFirstElement,
        Path::new("RUF015.py")
    )]
    #[test_case(Rule::InvalidIndexType, Path::new("RUF016.py"))]
    #[test_case(Rule::QuadraticListSummation, Path::new("RUF017_1.py"))]
    #[test_case(Rule::QuadraticListSummation, Path::new("RUF017_0.py"))]
    #[test_case(Rule::AssignmentInAssert, Path::new("RUF018.py"))]
    #[test_case(Rule::UnnecessaryKeyCheck, Path::new("RUF019.py"))]
    #[test_case(Rule::NeverUnion, Path::new("RUF020.py"))]
    #[test_case(Rule::ParenthesizeChainedOperators, Path::new("RUF021.py"))]
    #[test_case(Rule::UnsortedDunderAll, Path::new("RUF022.py"))]
    #[test_case(Rule::UnsortedDunderSlots, Path::new("RUF023.py"))]
    #[test_case(Rule::MutableFromkeysValue, Path::new("RUF024.py"))]
    #[test_case(Rule::DefaultFactoryKwarg, Path::new("RUF026.py"))]
    #[test_case(Rule::MissingFStringSyntax, Path::new("RUF027_0.py"))]
    #[test_case(Rule::MissingFStringSyntax, Path::new("RUF027_1.py"))]
    #[test_case(Rule::MissingFStringSyntax, Path::new("RUF027_2.py"))]
    #[test_case(Rule::InvalidFormatterSuppressionComment, Path::new("RUF028.py"))]
    #[test_case(Rule::UnusedAsync, Path::new("RUF029.py"))]
    #[test_case(Rule::AssertWithPrintMessage, Path::new("RUF030.py"))]
    #[test_case(Rule::IncorrectlyParenthesizedTupleInSubscript, Path::new("RUF031.py"))]
    #[test_case(Rule::DecimalFromFloatLiteral, Path::new("RUF032.py"))]
    #[test_case(Rule::PostInitDefault, Path::new("RUF033.py"))]
    #[test_case(Rule::UselessIfElse, Path::new("RUF034.py"))]
    #[test_case(Rule::NoneNotAtEndOfUnion, Path::new("RUF036.py"))]
    #[test_case(Rule::NoneNotAtEndOfUnion, Path::new("RUF036.pyi"))]
    #[test_case(Rule::UnnecessaryEmptyIterableWithinDequeCall, Path::new("RUF037.py"))]
    #[test_case(Rule::RedundantBoolLiteral, Path::new("RUF038.py"))]
    #[test_case(Rule::RedundantBoolLiteral, Path::new("RUF038.pyi"))]
    #[test_case(Rule::InvalidAssertMessageLiteralArgument, Path::new("RUF040.py"))]
    #[test_case(Rule::UnnecessaryNestedLiteral, Path::new("RUF041.py"))]
    #[test_case(Rule::UnnecessaryNestedLiteral, Path::new("RUF041.pyi"))]
    #[test_case(Rule::PytestRaisesAmbiguousPattern, Path::new("RUF043.py"))]
    #[test_case(Rule::UnnecessaryCastToInt, Path::new("RUF046.py"))]
    #[test_case(Rule::UnnecessaryCastToInt, Path::new("RUF046_CR.py"))]
    #[test_case(Rule::UnnecessaryCastToInt, Path::new("RUF046_LF.py"))]
    #[test_case(Rule::NeedlessElse, Path::new("RUF047_if.py"))]
    #[test_case(Rule::NeedlessElse, Path::new("RUF047_for.py"))]
    #[test_case(Rule::NeedlessElse, Path::new("RUF047_while.py"))]
    #[test_case(Rule::NeedlessElse, Path::new("RUF047_try.py"))]
    #[test_case(Rule::MapIntVersionParsing, Path::new("RUF048.py"))]
    #[test_case(Rule::MapIntVersionParsing, Path::new("RUF048_1.py"))]
    #[test_case(Rule::DataclassEnum, Path::new("RUF049.py"))]
    #[test_case(Rule::UnnecessaryIf, Path::new("RUF050.py"))]
    #[test_case(Rule::IfKeyInDictDel, Path::new("RUF051.py"))]
    #[test_case(Rule::UsedDummyVariable, Path::new("RUF052_0.py"))]
    #[test_case(Rule::UsedDummyVariable, Path::new("RUF052_1.py"))]
    #[test_case(Rule::ClassWithMixedTypeVars, Path::new("RUF053.py"))]
    #[test_case(Rule::FalsyDictGetFallback, Path::new("RUF056.py"))]
    #[test_case(Rule::UnnecessaryRound, Path::new("RUF057.py"))]
    #[test_case(Rule::StarmapZip, Path::new("RUF058_0.py"))]
    #[test_case(Rule::StarmapZip, Path::new("RUF058_1.py"))]
    #[test_case(Rule::UnusedUnpackedVariable, Path::new("RUF059_0.py"))]
    #[test_case(Rule::UnusedUnpackedVariable, Path::new("RUF059_1.py"))]
    #[test_case(Rule::UnusedUnpackedVariable, Path::new("RUF059_2.py"))]
    #[test_case(Rule::UnusedUnpackedVariable, Path::new("RUF059_3.py"))]
    #[test_case(Rule::InEmptyCollection, Path::new("RUF060.py"))]
    #[test_case(Rule::LegacyFormPytestRaises, Path::new("RUF061_raises.py"))]
    #[test_case(Rule::LegacyFormPytestRaises, Path::new("RUF061_warns.py"))]
    #[test_case(Rule::LegacyFormPytestRaises, Path::new("RUF061_deprecated_call.py"))]
    #[test_case(Rule::NonOctalPermissions, Path::new("RUF064.py"))]
    #[test_case(Rule::LoggingEagerConversion, Path::new("RUF065_0.py"))]
    #[test_case(Rule::LoggingEagerConversion, Path::new("RUF065_1.py"))]
    #[test_case(Rule::PropertyWithoutReturn, Path::new("RUF066.py"))]
    #[test_case(Rule::DuplicateEntryInDunderAll, Path::new("RUF068.py"))]
    #[test_case(Rule::RedirectedNOQA, Path::new("RUF101_0.py"))]
    #[test_case(Rule::RedirectedNOQA, Path::new("RUF101_1.py"))]
    #[test_case(Rule::InvalidRuleCode, Path::new("RUF102.py"))]
    #[test_case(Rule::InvalidRuleCode, Path::new("RUF102_1.py"))]
    #[test_case(Rule::NonEmptyInitModule, Path::new("RUF067/modules/__init__.py"))]
    #[test_case(Rule::NonEmptyInitModule, Path::new("RUF067/modules/okay.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    /// Test that RUF072 (useless-finally) and RUF047 (needless-else) converge
    /// when both are enabled on the same `try` statement: RUF072 removes the
    /// empty `finally` and RUF047 removes the empty `else` independently
    #[test]
    fn useless_finally_and_needless_else() -> Result<()> {
        use ruff_python_ast::{PySourceType, SourceType};

        let path = test_resource_path("fixtures").join("ruff/RUF072_RUF047.py");
        let source_type = SourceType::Python(PySourceType::from(&path));
        let source_kind = SourceKind::from_path(&path, source_type)?.expect("valid source");
        let settings =
            settings::LinterSettings::for_rules(vec![Rule::UselessFinally, Rule::NeedlessElse]);

        let (diagnostics, transformed) = test_contents(&source_kind, &path, &settings);
        assert_diagnostics!(diagnostics);

        insta::assert_snapshot!(transformed.source_code());
        Ok(())
    }

    /// Test that RUF072 (useless-finally) and SIM105 (suppressible-exception)
    /// converge: RUF072 removes the empty `finally` first, unblocking SIM105
    /// to rewrite `try/except: pass` into `contextlib.suppress()`
    #[test]
    fn useless_finally_and_suppressible_exception() -> Result<()> {
        use ruff_python_ast::{PySourceType, SourceType};

        let path = test_resource_path("fixtures").join("ruff/RUF072_SIM105.py");
        let source_type = SourceType::Python(PySourceType::from(&path));
        let source_kind = SourceKind::from_path(&path, source_type)?.expect("valid source");
        let settings = settings::LinterSettings::for_rules(vec![
            Rule::UselessFinally,
            Rule::SuppressibleException,
        ]);

        let (diagnostics, transformed) = test_contents(&source_kind, &path, &settings);
        assert_diagnostics!(diagnostics);

        insta::assert_snapshot!(transformed.source_code());
        Ok(())
    }

    /// Test that RUF072 + RUF047 + SIM105 converge when all three non-body
    /// clauses (except, else, finally) are no-ops
    #[test]
    fn useless_finally_and_needless_else_and_suppressible_exception() -> Result<()> {
        use ruff_python_ast::{PySourceType, SourceType};

        let path = test_resource_path("fixtures").join("ruff/RUF072_RUF047_SIM105.py");
        let source_type = SourceType::Python(PySourceType::from(&path));
        let source_kind = SourceKind::from_path(&path, source_type)?.expect("valid source");
        let settings = settings::LinterSettings::for_rules(vec![
            Rule::UselessFinally,
            Rule::NeedlessElse,
            Rule::SuppressibleException,
        ]);

        let (diagnostics, transformed) = test_contents(&source_kind, &path, &settings);
        assert_diagnostics!(diagnostics);

        insta::assert_snapshot!(transformed.source_code());
        Ok(())
    }

    /// Test that RUF047 (needless-else) and RUF050 (unnecessary-if) converge
    /// when both are enabled: RUF047 removes the empty `else` first, then
    /// RUF050 removes the remaining empty `if` on the next fix iteration.
    #[test]
    fn unnecessary_if_and_needless_else() -> Result<()> {
        use ruff_python_ast::{PySourceType, SourceType};

        let path = test_resource_path("fixtures").join("ruff/RUF050_RUF047.py");
        let source_type = SourceType::Python(PySourceType::from(&path));
        let source_kind = SourceKind::from_path(&path, source_type)?.expect("valid source");
        let settings =
            settings::LinterSettings::for_rules(vec![Rule::NeedlessElse, Rule::UnnecessaryIf]);

        let (diagnostics, transformed) = test_contents(&source_kind, &path, &settings);
        assert_diagnostics!(diagnostics);

        insta::assert_snapshot!(transformed.source_code());
        Ok(())
    }

    /// Reproduces issue #9472: F401 removes unused imports leaving empty `if`
    /// blocks, then RUF050 removes those, then F401 cleans up the now-unused
    /// guard imports. Verifies the full chain converges and produces the
    /// expected output.
    #[test]
    fn unnecessary_if_and_unused_import() -> Result<()> {
        use ruff_python_ast::{PySourceType, SourceType};

        let path = test_resource_path("fixtures").join("ruff/RUF050_F401.py");
        let source_type = SourceType::Python(PySourceType::from(&path));
        let source_kind = SourceKind::from_path(&path, source_type)?.expect("valid source");
        let settings =
            settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnnecessaryIf]);

        let (diagnostics, transformed) = test_contents(&source_kind, &path, &settings);
        assert_diagnostics!(diagnostics);

        insta::assert_snapshot!(transformed.source_code());
        Ok(())
    }

    #[test]
    fn missing_fstring_syntax_backslash_py311() -> Result<()> {
        assert_diagnostics_diff!(
            Path::new("ruff/RUF027_0.py"),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY312.into(),
                ..LinterSettings::for_rule(Rule::MissingFStringSyntax)
            },
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY311.into(),
                ..LinterSettings::for_rule(Rule::MissingFStringSyntax)
            },
        );
        Ok(())
    }

    #[test]
    fn prefer_parentheses_getitem_tuple() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF031_prefer_parens.py"),
            &LinterSettings {
                ruff: super::settings::Settings {
                    parenthesize_tuple_in_subscript: true,
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::IncorrectlyParenthesizedTupleInSubscript)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_remove_parentheses_starred_expr_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF031.py"),
            &LinterSettings {
                ruff: super::settings::Settings {
                    parenthesize_tuple_in_subscript: false,
                    ..super::settings::Settings::default()
                },
                unresolved_target_version: PythonVersion::PY310.into(),
                ..LinterSettings::for_rule(Rule::IncorrectlyParenthesizedTupleInSubscript)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test_case(Path::new("RUF013_0.py"))]
    #[test_case(Path::new("RUF013_1.py"))]
    fn implicit_optional_py39(path: &Path) -> Result<()> {
        let snapshot = format!(
            "PY39_{}_{}",
            Rule::ImplicitOptional.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings::for_rule(Rule::ImplicitOptional)
                .with_target_version(PythonVersion::PY39),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn access_annotations_from_class_dict_py39_no_typing_extensions() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF063.py"),
            &LinterSettings {
                typing_extensions: false,
                unresolved_target_version: PythonVersion::PY39.into(),
                ..LinterSettings::for_rule(Rule::AccessAnnotationsFromClassDict)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn access_annotations_from_class_dict_py39_with_typing_extensions() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF063.py"),
            &LinterSettings {
                typing_extensions: true,
                unresolved_target_version: PythonVersion::PY39.into(),
                ..LinterSettings::for_rule(Rule::AccessAnnotationsFromClassDict)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn none_not_at_end_of_union_py313() {
        let diagnostics = test_snippet(
            r"
            def func(arg: None | int):
                ...

            print(None | (int)and 2)
            ",
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY313.into(),
                ..settings::LinterSettings::for_rule(Rule::NoneNotAtEndOfUnion)
            },
        );
        assert_diagnostics!("PY313_RUF036_runtime_evaluated", diagnostics);
    }

    #[test]
    fn quadratic_list_summation_py315() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF017_0.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY315.into(),
                ..settings::LinterSettings::for_rule(Rule::QuadraticListSummation)
            },
        )?;
        assert_diagnostics!("PY315_RUF017_RUF017_0.py", diagnostics);
        Ok(())
    }

    #[test]
    fn access_annotations_from_class_dict_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF063.py"),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY310.into(),
                ..LinterSettings::for_rule(Rule::AccessAnnotationsFromClassDict)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn access_annotations_from_class_dict_py314() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF063.py"),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY314.into(),
                ..LinterSettings::for_rule(Rule::AccessAnnotationsFromClassDict)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn property_without_return_custom_decorator() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF066_custom_property_decorator.py"),
            &LinterSettings {
                pydocstyle: PydocstyleSettings {
                    property_decorators: ["my_library.custom_property".to_string()]
                        .into_iter()
                        .collect(),
                    ..PydocstyleSettings::default()
                },
                ..LinterSettings::for_rule(Rule::PropertyWithoutReturn)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn confusables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/confusables.py"),
            &settings::LinterSettings {
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::LinterSettings::for_rules(vec![
                    Rule::AmbiguousUnicodeCharacterString,
                    Rule::AmbiguousUnicodeCharacterDocstring,
                    Rule::AmbiguousUnicodeCharacterComment,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn confusables_deferred_annotations_diff() -> Result<()> {
        assert_diagnostics_diff!(
            Path::new("ruff/confusables.py"),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY313.into(),
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::LinterSettings::for_rules(vec![
                    Rule::AmbiguousUnicodeCharacterString,
                    Rule::AmbiguousUnicodeCharacterDocstring,
                    Rule::AmbiguousUnicodeCharacterComment,
                ])
            },
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY314.into(),
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::LinterSettings::for_rules(vec![
                    Rule::AmbiguousUnicodeCharacterString,
                    Rule::AmbiguousUnicodeCharacterDocstring,
                    Rule::AmbiguousUnicodeCharacterComment,
                ])
            },
        );
        Ok(())
    }

    #[test]
    fn preview_confusables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/confusables.py"),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::LinterSettings::for_rules(vec![
                    Rule::AmbiguousUnicodeCharacterString,
                    Rule::AmbiguousUnicodeCharacterDocstring,
                    Rule::AmbiguousUnicodeCharacterComment,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/noqa.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::UnusedVariable,
                Rule::AmbiguousVariableName,
            ]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn range_suppressions() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/suppressions.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::UnusedVariable,
                Rule::UnusedFunctionArgument,
                Rule::UnusedMethodArgument,
                Rule::AmbiguousVariableName,
                Rule::UnusedNOQA,
                Rule::InvalidRuleCode,
                Rule::InvalidSuppressionComment,
                Rule::UnmatchedSuppressionComment,
            ])
            .with_external_rules(&["TK421"])
            .with_preview_mode(),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_0() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_0.py"),
            &settings::LinterSettings {
                external: vec!["V101".to_string()],
                ..settings::LinterSettings::for_rules(vec![
                    Rule::UnusedNOQA,
                    Rule::LineTooLong,
                    Rule::UnusedImport,
                    Rule::UnusedVariable,
                    Rule::TabIndentation,
                    Rule::YodaConditions,
                    Rule::SuspiciousEvalUsage,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_0_prefix() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_0.py"),
            &settings::LinterSettings {
                external: vec!["V".to_string()],
                ..settings::LinterSettings::for_rules(vec![
                    Rule::UnusedNOQA,
                    Rule::LineTooLong,
                    Rule::UnusedImport,
                    Rule::UnusedVariable,
                    Rule::TabIndentation,
                    Rule::YodaConditions,
                    Rule::SuspiciousEvalUsage,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_1() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_1.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_2() -> Result<()> {
        let mut settings =
            settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]);

        settings.per_file_ignores = CompiledPerFileIgnoreList::resolve(vec![PerFileIgnore::new(
            "RUF100_2.py".to_string(),
            &["F401".parse().unwrap()],
            None,
        )])
        .unwrap();

        let diagnostics = test_path(Path::new("ruff/RUF100_2.py"), &settings)?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_3() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_3.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::UnusedNOQA,
                Rule::LineTooLong,
                Rule::UndefinedName,
            ]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_4() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_4.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_5() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_5.py"),
            &settings::LinterSettings {
                ..settings::LinterSettings::for_rules(vec![
                    Rule::UnusedNOQA,
                    Rule::LineTooLong,
                    Rule::CommentedOutCode,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_filedirective_unused() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_6.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_filedirective_unused_last_of_many() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_7.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::UnusedNOQA,
                Rule::FStringMissingPlaceholders,
                Rule::LineTooLong,
                Rule::UnusedVariable,
            ]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test_case(Path::new("ruff/RUF102.py"))]
    #[test_case(Path::new("ruff/RUF102_1.py"))]
    fn invalid_rule_code_external_rules(path: &Path) -> Result<()> {
        let snapshot = format!(
            "invalid_rule_code_external_rules_{}",
            path.to_string_lossy(),
        );
        let diagnostics = test_path(
            path,
            &settings::LinterSettings {
                external: vec!["V".to_string()],
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(Rule::InvalidRuleCode)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_per_file_ignores() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_per_file_ignores.py"),
            &settings::LinterSettings {
                per_file_ignores: CompiledPerFileIgnoreList::resolve(vec![PerFileIgnore::new(
                    "ruff_per_file_ignores.py".to_string(),
                    &["F401".parse().unwrap(), "RUF100".parse().unwrap()],
                    None,
                )])
                .unwrap(),
                ..settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedNOQA])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_per_file_ignores_empty() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_per_file_ignores.py"),
            &settings::LinterSettings {
                per_file_ignores: CompiledPerFileIgnoreList::resolve(vec![PerFileIgnore::new(
                    "ruff_per_file_ignores.py".to_string(),
                    &["RUF100".parse().unwrap()],
                    None,
                )])
                .unwrap(),
                ..settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/flake8_noqa.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_all() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_all.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_codes() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_codes.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_invalid() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_invalid.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/redirects.py"),
            &settings::LinterSettings::for_rule(Rule::NonPEP604AnnotationUnion),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::InvalidPyprojectToml, Path::new("bleach"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("invalid_author"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("maturin"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("various_invalid"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("pep639"))]
    fn invalid_pyproject_toml(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let path = test_resource_path("fixtures")
            .join("ruff")
            .join("pyproject_toml")
            .join(path)
            .join("pyproject.toml");
        let contents = fs::read_to_string(path)?;
        let source_file = SourceFileBuilder::new("pyproject.toml", contents).finish();
        let messages = lint_pyproject_toml(
            &source_file,
            &settings::LinterSettings::for_rule(Rule::InvalidPyprojectToml),
        );
        assert_diagnostics!(snapshot, messages);
        Ok(())
    }

    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008_attrs.py"))]
    #[test_case(Rule::UnrawRePattern, Path::new("RUF039.py"))]
    #[test_case(Rule::UnrawRePattern, Path::new("RUF039_concat.py"))]
    #[test_case(Rule::UnnecessaryRegularExpression, Path::new("RUF055_0.py"))]
    #[test_case(Rule::UnnecessaryRegularExpression, Path::new("RUF055_1.py"))]
    #[test_case(Rule::UnnecessaryRegularExpression, Path::new("RUF055_2.py"))]
    #[test_case(Rule::UnnecessaryRegularExpression, Path::new("RUF055_3.py"))]
    #[test_case(Rule::IndentedFormFeed, Path::new("RUF054.py"))]
    #[test_case(Rule::ImplicitClassVarInDataclass, Path::new("RUF045.py"))]
    #[test_case(Rule::FloatEqualityComparison, Path::new("RUF069.py"))]
    #[test_case(Rule::UnnecessaryAssignBeforeYield, Path::new("RUF070.py"))]
    #[test_case(Rule::OsPathCommonprefix, Path::new("RUF071.py"))]
    #[test_case(Rule::UselessFinally, Path::new("RUF072.py"))]
    #[test_case(Rule::FStringPercentFormat, Path::new("RUF073.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnrawRePattern, Path::new("RUF039_py_version_sensitive.py"))]
    fn preview_rules_py37(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__py37__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                unresolved_target_version: PythonVersion::PY37.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnrawRePattern, Path::new("RUF039_py_version_sensitive.py"))]
    fn preview_rules_py38(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__py38__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                unresolved_target_version: PythonVersion::PY38.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UsedDummyVariable, Path::new("RUF052_0.py"), r"^_+", 1)]
    #[test_case(Rule::UsedDummyVariable, Path::new("RUF052_0.py"), r"", 2)]
    fn custom_regexp_preset(
        rule_code: Rule,
        path: &Path,
        regex_pattern: &str,
        id: u8,
    ) -> Result<()> {
        // Compile the regex from the pattern string
        let regex = Regex::new(regex_pattern).unwrap();

        let snapshot = format!(
            "custom_dummy_var_regexp_preset__{}_{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy(),
            id,
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                dummy_variable_rgx: regex,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::StarmapZip, Path::new("RUF058_2.py"))]
    fn map_strict_py314(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "py314__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY314.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_0.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_1.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_2.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_3.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_4.py"))]
    fn ruf013_add_future_import(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("add_future_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings {
                future_annotations: true,
                unresolved_target_version: PythonVersion::PY39.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn strictly_empty_init_modules_ruf067() -> Result<()> {
        assert_diagnostics_diff!(
            Path::new("ruff/RUF067/modules/__init__.py"),
            &LinterSettings {
                ruff: super::settings::Settings {
                    strictly_empty_init_modules: false,
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::NonEmptyInitModule)
            },
            &LinterSettings {
                ruff: super::settings::Settings {
                    strictly_empty_init_modules: true,
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::NonEmptyInitModule)
            },
        );
        Ok(())
    }
}
