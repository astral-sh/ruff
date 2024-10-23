//! Ruff-specific rules.

pub(crate) mod rules;
pub mod settings;
pub(crate) mod typing;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use ruff_source_file::SourceFileBuilder;

    use crate::pyproject_toml::lint_pyproject_toml;
    use crate::registry::Rule;
    use crate::settings::types::{
        CompiledPerFileIgnoreList, PerFileIgnore, PreviewMode, PythonVersion,
    };
    use crate::settings::LinterSettings;
    use crate::test::{test_path, test_resource_path};
    use crate::{assert_messages, settings};

    #[test_case(Rule::AsyncioDanglingTask, Path::new("RUF006.py"))]
    #[test_case(Rule::CollectionLiteralConcatenation, Path::new("RUF005.py"))]
    #[test_case(Rule::ExplicitFStringTypeConversion, Path::new("RUF010.py"))]
    #[test_case(Rule::FunctionCallInDataclassDefaultArgument, Path::new("RUF009.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_0.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_1.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_2.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_3.py"))]
    #[test_case(Rule::MutableClassDefault, Path::new("RUF012.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008.py"))]
    #[test_case(Rule::ZipInsteadOfPairwise, Path::new("RUF007.py"))]
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
    #[test_case(Rule::UselessIfElse, Path::new("RUF034.py"))]
    #[test_case(Rule::RedirectedNOQA, Path::new("RUF101.py"))]
    #[test_case(Rule::PostInitDefault, Path::new("RUF033.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn prefer_parentheses_getitem_tuple() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF031_prefer_parens.py"),
            &LinterSettings {
                ruff: super::settings::Settings {
                    parenthesize_tuple_in_subscript: true,
                },
                ..LinterSettings::for_rule(Rule::IncorrectlyParenthesizedTupleInSubscript)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_remove_parentheses_starred_expr_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF031.py"),
            &LinterSettings {
                ruff: super::settings::Settings {
                    parenthesize_tuple_in_subscript: false,
                },
                target_version: PythonVersion::Py310,
                ..LinterSettings::for_rule(Rule::IncorrectlyParenthesizedTupleInSubscript)
            },
        )?;
        assert_messages!(diagnostics);
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
                .with_target_version(PythonVersion::Py39),
        )?;
        assert_messages!(snapshot, diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_1() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_1.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]),
        )?;
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_4() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_4.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]),
        )?;
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/flake8_noqa.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_all() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_all.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_codes() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_codes.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa_invalid() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa_invalid.py"),
            &settings::LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/redirects.py"),
            &settings::LinterSettings::for_rules(vec![Rule::NonPEP604Annotation]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::InvalidPyprojectToml, Path::new("bleach"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("invalid_author"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("maturin"))]
    #[test_case(Rule::InvalidPyprojectToml, Path::new("maturin_gh_1615"))]
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
            source_file,
            &settings::LinterSettings::for_rule(Rule::InvalidPyprojectToml),
        );
        assert_messages!(snapshot, messages);
        Ok(())
    }

    #[test_case(Rule::ZipInsteadOfPairwise, Path::new("RUF007.py"))]
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
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
