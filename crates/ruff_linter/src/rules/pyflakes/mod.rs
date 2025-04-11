//! Rules from [Pyflakes](https://pypi.org/project/pyflakes/).
pub(crate) mod cformat;
pub(crate) mod fixes;
pub(crate) mod format;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use ruff_python_parser::ParseOptions;
    use rustc_hash::FxHashMap;
    use test_case::test_case;

    use ruff_python_ast::PySourceType;
    use ruff_python_codegen::Stylist;
    use ruff_python_index::Indexer;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::Ranged;

    use crate::linter::check_path;
    use crate::message::Message;
    use crate::registry::{AsRule, Linter, Rule};
    use crate::rules::isort;
    use crate::rules::pyflakes;
    use crate::settings::types::PreviewMode;
    use crate::settings::{flags, LinterSettings};
    use crate::source_kind::SourceKind;
    use crate::test::{test_contents, test_path, test_snippet};
    use crate::Locator;
    use crate::{assert_messages, directives};

    #[test_case(Rule::UnusedImport, Path::new("F401_0.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_1.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_2.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_3.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_4.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_5.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_6.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_7.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_8.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_9.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_10.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_11.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_12.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_13.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_14.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_15.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_16.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_17.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_18.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_19.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_20.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_21.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_22.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_23.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_32.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_34.py"))]
    #[test_case(Rule::ImportShadowedByLoopVar, Path::new("F402.py"))]
    #[test_case(Rule::ImportShadowedByLoopVar, Path::new("F402.ipynb"))]
    #[test_case(Rule::UndefinedLocalWithImportStar, Path::new("F403.py"))]
    #[test_case(Rule::LateFutureImport, Path::new("F404_0.py"))]
    #[test_case(Rule::LateFutureImport, Path::new("F404_1.py"))]
    #[test_case(Rule::UndefinedLocalWithImportStarUsage, Path::new("F405.py"))]
    #[test_case(Rule::UndefinedLocalWithNestedImportStarUsage, Path::new("F406.py"))]
    #[test_case(Rule::FutureFeatureNotDefined, Path::new("F407.py"))]
    #[test_case(Rule::PercentFormatInvalidFormat, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatExpectedMapping, Path::new("F502.py"))]
    #[test_case(Rule::PercentFormatExpectedMapping, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatExpectedSequence, Path::new("F503.py"))]
    #[test_case(Rule::PercentFormatExpectedSequence, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatExtraNamedArguments, Path::new("F504.py"))]
    #[test_case(Rule::PercentFormatExtraNamedArguments, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatMissingArgument, Path::new("F504.py"))]
    #[test_case(Rule::PercentFormatMissingArgument, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatMixedPositionalAndNamed, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatPositionalCountMismatch, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatStarRequiresSequence, Path::new("F50x.py"))]
    #[test_case(Rule::PercentFormatUnsupportedFormatCharacter, Path::new("F50x.py"))]
    #[test_case(Rule::StringDotFormatInvalidFormat, Path::new("F521.py"))]
    #[test_case(Rule::StringDotFormatExtraNamedArguments, Path::new("F522.py"))]
    #[test_case(Rule::StringDotFormatExtraPositionalArguments, Path::new("F523.py"))]
    #[test_case(Rule::StringDotFormatMissingArguments, Path::new("F524.py"))]
    #[test_case(Rule::StringDotFormatMixingAutomatic, Path::new("F525.py"))]
    #[test_case(Rule::FStringMissingPlaceholders, Path::new("F541.py"))]
    #[test_case(Rule::MultiValueRepeatedKeyLiteral, Path::new("F601.py"))]
    #[test_case(Rule::MultiValueRepeatedKeyVariable, Path::new("F602.py"))]
    #[test_case(Rule::MultipleStarredExpressions, Path::new("F622.py"))]
    #[test_case(Rule::AssertTuple, Path::new("F631.py"))]
    #[test_case(Rule::IsLiteral, Path::new("F632.py"))]
    #[test_case(Rule::InvalidPrintSyntax, Path::new("F633.py"))]
    #[test_case(Rule::IfTuple, Path::new("F634.py"))]
    #[test_case(Rule::BreakOutsideLoop, Path::new("F701.py"))]
    #[test_case(Rule::ContinueOutsideLoop, Path::new("F702.py"))]
    #[test_case(Rule::YieldOutsideFunction, Path::new("F704.py"))]
    #[test_case(Rule::ReturnOutsideFunction, Path::new("F706.py"))]
    #[test_case(Rule::DefaultExceptNotLast, Path::new("F707.py"))]
    #[test_case(Rule::ForwardAnnotationSyntaxError, Path::new("F722.py"))]
    #[test_case(Rule::ForwardAnnotationSyntaxError, Path::new("F722_1.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_0.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_1.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_2.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_3.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_4.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_5.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_6.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_7.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_8.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_9.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_10.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_11.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_12.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_13.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_14.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_15.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_16.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_17.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_18.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_19.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_20.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_21.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_22.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_23.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_24.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_25.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_26.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_27.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_28.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_29.pyi"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_30.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_31.py"))]
    #[test_case(Rule::RedefinedWhileUnused, Path::new("F811_32.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_0.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_1.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_2.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_3.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_4.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_5.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_5.pyi"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_6.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_7.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_8.pyi"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_9.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_10.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_11.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_11.pyi"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_12.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_13.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_14.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_15.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_16.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_17.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_18.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_19.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_20.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_21.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_22.ipynb"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_23.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_24.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_25.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_26.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_26.pyi"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_27.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_28.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_30.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_31.py"))]
    #[test_case(Rule::UndefinedName, Path::new("F821_32.pyi"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_0.py"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_0.pyi"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_1.py"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_1b.py"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_2.py"))]
    #[test_case(Rule::UndefinedExport, Path::new("F822_3.py"))]
    #[test_case(Rule::UndefinedLocal, Path::new("F823.py"))]
    #[test_case(Rule::UnusedVariable, Path::new("F841_0.py"))]
    #[test_case(Rule::UnusedVariable, Path::new("F841_1.py"))]
    #[test_case(Rule::UnusedVariable, Path::new("F841_2.py"))]
    #[test_case(Rule::UnusedVariable, Path::new("F841_3.py"))]
    #[test_case(Rule::UnusedAnnotation, Path::new("F842.py"))]
    #[test_case(Rule::RaiseNotImplemented, Path::new("F901.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UndefinedName, Path::new("F821_29.py"))]
    fn rules_with_flake8_type_checking_settings_enabled(
        rule_code: Rule,
        path: &Path,
    ) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings {
                flake8_type_checking: crate::rules::flake8_type_checking::settings::Settings {
                    runtime_required_base_classes: vec![
                        "pydantic.BaseModel".to_string(),
                        "sqlalchemy.orm.DeclarativeBase".to_string(),
                    ],
                    ..Default::default()
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn f821_with_builtin_added_on_new_py_version_but_old_target_version_specified() {
        let diagnostics = test_snippet(
            "PythonFinalizationError",
            &LinterSettings {
                unresolved_target_version: ruff_python_ast::PythonVersion::PY312,
                ..LinterSettings::for_rule(Rule::UndefinedName)
            },
        );
        assert_messages!(diagnostics);
    }

    #[test_case(Rule::UnusedImport, Path::new("__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_24/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_25__all_nonempty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_26__all_empty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_27__all_mistyped/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_28__all_multiple/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_29__all_conditional/__init__.py"))]
    #[test_case(Rule::UndefinedExport, Path::new("__init__.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings {
                preview: PreviewMode::Enabled,
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        r"import submodule.a",
        "f401_preview_first_party_submodule_no_dunder_all"
    )]
    #[test_case(
        r"
        import submodule.a
        __all__ = ['FOO']
        FOO = 42",
        "f401_preview_first_party_submodule_dunder_all"
    )]
    fn f401_preview_first_party_submodule(contents: &str, snapshot: &str) {
        let diagnostics = test_contents(
            &SourceKind::Python(dedent(contents).to_string()),
            Path::new("f401_preview_first_party_submodule/__init__.py"),
            &LinterSettings {
                preview: PreviewMode::Enabled,
                isort: isort::settings::Settings {
                    // This case specifically tests the scenario where
                    // the unused import is a first-party submodule import;
                    // use the isort settings to ensure that the `submodule.a` import
                    // is recognised as first-party in the test:
                    known_modules: isort::categorize::KnownModules::new(
                        vec!["submodule".parse().unwrap()],
                        vec![],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..isort::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::UnusedImport)
            },
        )
        .0;
        assert_messages!(snapshot, diagnostics);
    }

    // Regression test for https://github.com/astral-sh/ruff/issues/12897
    #[test_case(Rule::UnusedImport, Path::new("F401_33/__init__.py"))]
    fn f401_preview_local_init_import(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let settings = LinterSettings {
            preview: PreviewMode::Enabled,
            isort: isort::settings::Settings {
                // Like `f401_preview_first_party_submodule`, this test requires the input module to
                // be first-party
                known_modules: isort::categorize::KnownModules::new(
                    vec!["F401_*".parse()?],
                    vec![],
                    vec![],
                    vec![],
                    FxHashMap::default(),
                ),
                ..isort::settings::Settings::default()
            },
            ..LinterSettings::for_rule(rule_code)
        };
        let diagnostics = test_path(Path::new("pyflakes").join(path).as_path(), &settings)?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnusedImport, Path::new("F401_24/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_25__all_nonempty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_26__all_empty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_27__all_mistyped/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_28__all_multiple/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_29__all_conditional/__init__.py"))]
    fn f401_stable(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_stable_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnusedImport, Path::new("F401_24/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_25__all_nonempty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_26__all_empty/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_27__all_mistyped/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_28__all_multiple/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_29__all_conditional/__init__.py"))]
    #[test_case(Rule::UnusedImport, Path::new("F401_30.py"))]
    fn f401_deprecated_option(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_deprecated_option_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings {
                ignore_init_module_imports: false,
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnusedImport, Path::new("F401_31.py"))]
    fn f401_allowed_unused_imports_option(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes").join(path).as_path(),
            &LinterSettings {
                pyflakes: pyflakes::settings::Settings {
                    allowed_unused_imports: vec!["hvplot.pandas".to_string()],
                    ..pyflakes::settings::Settings::default()
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn f841_dummy_variable_rgx() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/F841_0.py"),
            &LinterSettings {
                dummy_variable_rgx: Regex::new(r"^z$").unwrap(),
                ..LinterSettings::for_rule(Rule::UnusedVariable)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn init() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/__init__.py"),
            &LinterSettings::for_rules(vec![
                Rule::UndefinedName,
                Rule::UndefinedExport,
                Rule::UnusedImport,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn default_builtins() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/builtins.py"),
            &LinterSettings::for_rules(vec![Rule::UndefinedName]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn extra_builtins() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/builtins.py"),
            &LinterSettings {
                builtins: vec!["_".to_string()],
                ..LinterSettings::for_rules(vec![Rule::UndefinedName])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn default_typing_modules() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/typing_modules.py"),
            &LinterSettings::for_rules(vec![Rule::UndefinedName]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn extra_typing_modules() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/typing_modules.py"),
            &LinterSettings {
                typing_modules: vec!["airflow.typing_compat".to_string()],
                ..LinterSettings::for_rules(vec![Rule::UndefinedName])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/future_annotations.py"),
            &LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UndefinedName]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn multi_statement_lines() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/multi_statement_lines.py"),
            &LinterSettings::for_rule(Rule::UnusedImport),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn relative_typing_module() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/project/foo/bar.py"),
            &LinterSettings {
                typing_modules: vec!["foo.typical".to_string()],
                ..LinterSettings::for_rule(Rule::UndefinedName)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn nested_relative_typing_module() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/project/foo/bop/baz.py"),
            &LinterSettings {
                typing_modules: vec!["foo.typical".to_string()],
                ..LinterSettings::for_rule(Rule::UndefinedName)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn extend_generics() -> Result<()> {
        let snapshot = "extend_immutable_calls".to_string();
        let diagnostics = test_path(
            Path::new("pyflakes/F401_15.py"),
            &LinterSettings {
                pyflakes: pyflakes::settings::Settings {
                    extend_generics: vec!["django.db.models.ForeignKey".to_string()],
                    ..pyflakes::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::UnusedImport)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        r"
        import os

        def f():
            import os

            # Despite this `del`, `import os` in `f` should still be flagged as shadowing an unused
            # import.
            del os
    ",
        "del_shadowed_global_import_in_local_scope"
    )]
    #[test_case(
        r"
        import os

        def f():
            import os

        # Despite this `del`, `import os` in `f` should still be flagged as shadowing an unused
        # import. (This is a false negative, but is consistent with Pyflakes.)
        del os
    ",
        "del_shadowed_global_import_in_global_scope"
    )]
    #[test_case(
        r"
        def f():
            import os
            import os

            # Despite this `del`, `import os` should still be flagged as shadowing an unused
            # import.
            del os
    ",
        "del_shadowed_local_import_in_local_scope"
    )]
    #[test_case(
        r"
        import os

        def f():
            os = 1
            print(os)

            del os

            def g():
                # `import os` doesn't need to be flagged as shadowing an import.
                os = 1
                print(os)
        ",
        "del_shadowed_import_shadow_in_local_scope"
    )]
    #[test_case(
        r"
        x = 1

        def foo():
            x = 2
            del x
            # Flake8 treats this as an F823 error, because it removes the binding
            # entirely after the `del` statement. However, it should be an F821
            # error, because the name is defined in the scope, but unbound.
            x += 1
    ",
        "augmented_assignment_after_del"
    )]
    #[test_case(
        r"
        def f():
            x = 1

            try:
                1 / 0
            except Exception as x:
                pass

            # No error here, though it should arguably be an F821 error. `x` will
            # be unbound after the `except` block (assuming an exception is raised
            # and caught).
            print(x)
            ",
        "print_in_body_after_shadowing_except"
    )]
    #[test_case(
        r"
        def f():
            x = 1

            try:
                1 / 0
            except ValueError as x:
                pass
            except ImportError as x:
                pass

            # No error here, though it should arguably be an F821 error. `x` will
            # be unbound after the `except` block (assuming an exception is raised
            # and caught).
            print(x)
            ",
        "print_in_body_after_double_shadowing_except"
    )]
    #[test_case(
        r"
        def f():
            try:
                x = 3
            except ImportError as x:
                print(x)
            else:
                print(x)
            ",
        "print_in_try_else_after_shadowing_except"
    )]
    #[test_case(
        r"
        def f():
            list = [1, 2, 3]

            for e in list:
                if e % 2 == 0:
                    try:
                        pass
                    except Exception as e:
                        print(e)
                else:
                    print(e)
            ",
        "print_in_if_else_after_shadowing_except"
    )]
    #[test_case(
        r"
        def f():
            x = 1
            del x
            del x
            ",
        "double_del"
    )]
    #[test_case(
        r"
        x = 1

        def f():
            try:
                pass
            except ValueError as x:
                pass

            # This should resolve to the `x` in `x = 1`.
            print(x)
            ",
        "load_after_unbind_from_module_scope"
    )]
    #[test_case(
        r"
        x = 1

        def f():
            try:
                pass
            except ValueError as x:
                pass

            try:
                pass
            except ValueError as x:
                pass

            # This should resolve to the `x` in `x = 1`.
            print(x)
            ",
        "load_after_multiple_unbinds_from_module_scope"
    )]
    #[test_case(
        r"
        x = 1

        def f():
            try:
                pass
            except ValueError as x:
                pass

            def g():
                try:
                    pass
                except ValueError as x:
                    pass

                # This should resolve to the `x` in `x = 1`.
                print(x)
            ",
        "load_after_unbind_from_nested_module_scope"
    )]
    #[test_case(
        r"
        class C:
            x = 1

            def f():
                try:
                    pass
                except ValueError as x:
                    pass

                # This should raise an F821 error, rather than resolving to the
                # `x` in `x = 1`.
                print(x)
            ",
        "load_after_unbind_from_class_scope"
    )]
    fn contents(contents: &str, snapshot: &str) {
        let diagnostics = test_snippet(
            contents,
            &LinterSettings::for_rules(Linter::Pyflakes.rules()),
        );
        assert_messages!(snapshot, diagnostics);
    }

    /// A re-implementation of the Pyflakes test runner.
    /// Note that all tests marked with `#[ignore]` should be considered TODOs.
    fn flakes(contents: &str, expected: &[Rule]) {
        let contents = dedent(contents);
        let source_type = PySourceType::default();
        let source_kind = SourceKind::Python(contents.to_string());
        let settings = LinterSettings::for_rules(Linter::Pyflakes.rules());
        let options =
            ParseOptions::from(source_type).with_target_version(settings.unresolved_target_version);
        let parsed = ruff_python_parser::parse_unchecked(source_kind.source_code(), options)
            .try_into_module()
            .expect("PySourceType always parses into a module");
        let locator = Locator::new(&contents);
        let stylist = Stylist::from_tokens(parsed.tokens(), locator.contents());
        let indexer = Indexer::from_tokens(parsed.tokens(), locator.contents());
        let directives = directives::extract_directives(
            parsed.tokens(),
            directives::Flags::from_settings(&settings),
            &locator,
            &indexer,
        );
        let mut messages = check_path(
            Path::new("<filename>"),
            None,
            &locator,
            &stylist,
            &indexer,
            &directives,
            &settings,
            flags::Noqa::Enabled,
            &source_kind,
            source_type,
            &parsed,
            settings.unresolved_target_version,
        );
        messages.sort_by_key(Ranged::start);
        let actual = messages
            .iter()
            .filter_map(Message::as_diagnostic_message)
            .map(|diagnostic| diagnostic.kind.rule())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_undefined_names.py>
    #[test]
    fn undefined() {
        flakes("bar", &[Rule::UndefinedName]);
    }

    #[test]
    fn defined_in_list_comp() {
        flakes("[a for a in range(10) if a]", &[]);
    }

    #[test]
    fn undefined_in_list_comp() {
        flakes(
            r"
        [a for a in range(10)]
        a
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn undefined_exception_name() {
        // Exception names can't be used after the except: block.
        //
        // The exc variable is unused inside the exception handler.
        flakes(
            r"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            pass
        exc
        ",
            &[Rule::UnusedVariable, Rule::UndefinedName],
        );
    }

    #[test]
    fn names_declared_in_except_blocks() {
        // Locals declared in except: blocks can be used after the block.
        //
        // This shows the example in test_undefinedExceptionName is
        // different.
        flakes(
            r"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            e = exc
        e
        ",
            &[],
        );
    }

    #[test]
    fn undefined_exception_name_obscuring_local_variable2() {
        // Exception names are unbound after the `except:` block.
        //
        // Last line will raise UnboundLocalError.
        // The exc variable is unused inside the exception handler.
        flakes(
            r"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            pass
        print(exc)
        exc = 'Original value'
        ",
            &[Rule::UnusedVariable, Rule::UndefinedName],
        );
    }

    #[test]
    fn del_exception_in_except() {
        // The exception name can be deleted in the except: block.
        flakes(
            r"
        try:
            pass
        except Exception as exc:
            del exc
        ",
            &[],
        );
    }

    #[test]
    fn functions_need_global_scope() {
        flakes(
            r"
        class a:
            def b():
                fu
        fu = 1
        ",
            &[],
        );
    }

    #[test]
    fn builtins() {
        flakes("range(10)", &[]);
    }

    #[test]
    fn builtin_windows_error() {
        // C{WindowsError} is sometimes a builtin name, so no warning is emitted
        // for using it.
        flakes("WindowsError", &[]);
    }

    #[test]
    fn module_annotations() {
        // Use of the C{__annotations__} in module scope should not emit
        // an undefined name warning when version is greater than or equal to 3.6.
        flakes("__annotations__", &[]);
    }

    #[test]
    fn magic_globals_file() {
        // Use of the C{__file__} magic global should not emit an undefined name
        // warning.
        flakes("__file__", &[]);
    }

    #[test]
    fn magic_globals_builtins() {
        // Use of the C{__builtins__} magic global should not emit an undefined
        // name warning.
        flakes("__builtins__", &[]);
    }

    #[test]
    fn magic_globals_name() {
        // Use of the C{__name__} magic global should not emit an undefined name
        // warning.
        flakes("__name__", &[]);
    }

    #[test]
    fn magic_module_in_class_scope() {
        // Use of the C{__module__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        flakes("__module__", &[Rule::UndefinedName]);
        flakes(
            r"
        class Foo:
            __module__
        ",
            &[],
        );
        flakes(
            r"
        class Foo:
            def bar(self):
                __module__
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn magic_qualname_in_class_scope() {
        // Use of the C{__qualname__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        flakes("__qualname__", &[Rule::UndefinedName]);
        flakes(
            r"
        class Foo:
            __qualname__
        ",
            &[],
        );
        flakes(
            r"
        class Foo:
            def bar(self):
                __qualname__
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        def f():
            __qualname__ = 1

            class Foo:
                __qualname__
        ",
            &[Rule::UnusedVariable],
        );
    }

    #[test]
    fn global_import_star() {
        // Can't find undefined names with import *.
        flakes(
            "from fu import *; bar",
            &[
                Rule::UndefinedLocalWithImportStar,
                Rule::UndefinedLocalWithImportStarUsage,
            ],
        );
    }

    #[test]
    fn defined_by_global() {
        // "global" can make an otherwise undefined name in another function
        // defined.
        flakes(
            r"
        def a(): global fu; fu = 1
        def b(): fu
        ",
            &[],
        );
        flakes(
            r"
        def c(): bar
        def b(): global bar; bar = 1
        ",
            &[],
        );
        // TODO(charlie): Extract globals recursively (such that we don't raise F821).
        flakes(
            r"
        def c(): bar
        def d():
            def b():
                global bar; bar = 1
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn defined_by_global_multiple_names() {
        // "global" can accept multiple names.
        flakes(
            r"
        def a(): global fu, bar; fu = 1; bar = 2
        def b(): fu; bar
        ",
            &[],
        );
    }

    #[test]
    fn global_in_global_scope() {
        // A global statement in the global scope is ignored.
        flakes(
            r"
        global x
        def foo():
            print(x)
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn global_reset_name_only() {
        // A global statement does not prevent other names being undefined.
        flakes(
            r"
        def f1():
            s

        def f2():
            global m
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn del() {
        // Del deletes bindings.
        flakes("a = 1; del a; a", &[Rule::UndefinedName]);
    }

    #[test]
    fn del_global() {
        // Del a global binding from a function.
        flakes(
            r"
        a = 1
        def f():
            global a
            del a
        a
        ",
            &[],
        );
    }

    #[test]
    fn del_undefined() {
        // Del an undefined name.
        flakes("del a", &[Rule::UndefinedName]);
    }

    #[test]
    fn del_conditional() {
        // Ignores conditional bindings deletion.
        flakes(
            r"
        context = None
        test = True
        if False:
            del(test)
        assert(test)
        ",
            &[],
        );
    }

    #[test]
    fn del_conditional_nested() {
        // Ignored conditional bindings deletion even if they are nested in other
        // blocks.
        flakes(
            r"
        context = None
        test = True
        if False:
            with context():
                del(test)
        assert(test)
        ",
            &[],
        );
    }

    #[test]
    fn del_while() {
        // Ignore bindings deletion if called inside the body of a while
        // statement.
        flakes(
            r"
        def test():
            foo = 'bar'
            while False:
                del foo
            assert(foo)
        ",
            &[],
        );
    }

    #[test]
    fn del_while_test_usage() {
        // Ignore bindings deletion if called inside the body of a while
        // statement and name is used inside while's test part.
        flakes(
            r"
        def _worker():
            o = True
            while o is not True:
                del o
                o = False
        ",
            &[],
        );
    }

    #[test]
    fn del_while_nested() {
        // Ignore bindings deletions if node is part of while's test, even when
        // del is in a nested block.
        flakes(
            r"
        context = None
        def _worker():
            o = True
            while o is not True:
                while True:
                    with context():
                        del o
                o = False
        ",
            &[],
        );
    }

    #[test]
    fn global_from_nested_scope() {
        // Global names are available from nested scopes.
        flakes(
            r"
        a = 1
        def b():
            def c():
                a
        ",
            &[],
        );
    }

    #[test]
    fn later_redefined_global_from_nested_scope() {
        // Test that referencing a local name that shadows a global, before it is
        // defined, generates a warning.
        flakes(
            r"
        a = 1
        def fun():
            a
            a = 2
            return a
        ",
            &[Rule::UndefinedLocal],
        );
    }

    #[test]
    fn later_redefined_global_from_nested_scope2() {
        // Test that referencing a local name in a nested scope that shadows a
        // global declared in an enclosing scope, before it is defined, generates
        // a warning.
        flakes(
            r"
            a = 1
            def fun():
                global a
                def fun2():
                    a
                    a = 2
                    return a
        ",
            &[Rule::UndefinedLocal],
        );
    }

    #[test]
    fn intermediate_class_scope_ignored() {
        // If a name defined in an enclosing scope is shadowed by a local variable
        // and the name is used locally before it is bound, an unbound local
        // warning is emitted, even if there is a class scope between the enclosing
        // scope and the local scope.
        flakes(
            r"
        def f():
            x = 1
            class g:
                def h(self):
                    a = x
                    x = None
                    print(x, a)
            print(x)
        ",
            &[Rule::UndefinedLocal],
        );
    }

    #[test]
    fn later_redefined_global_from_nested_scope3() {
        // Test that referencing a local name in a nested scope that shadows a
        // global, before it is defined, generates a warning.
        flakes(
            r"
            def fun():
                a = 1
                def fun2():
                    a
                    a = 1
                    return a
                return a
        ",
            &[Rule::UndefinedLocal],
        );
    }

    #[test]
    fn undefined_augmented_assignment() {
        flakes(
            r"
            def f(seq):
                a = 0
                seq[a] += 1
                seq[b] /= 2
                c[0] *= 2
                a -= 3
                d += 4
                e[any] = 5
            ",
            &[
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UnusedVariable,
                Rule::UndefinedName,
            ],
        );
    }

    #[test]
    fn nested_class() {
        // Nested classes can access enclosing scope.
        flakes(
            r"
        def f(foo):
            class C:
                bar = foo
                def f(self):
                    return foo
            return C()

        f(123).f()
        ",
            &[],
        );
    }

    #[test]
    fn bad_nested_class() {
        // Free variables in nested classes must bind at class creation.
        flakes(
            r"
        def f():
            class C:
                bar = foo
            foo = 456
            return foo
        f()
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn defined_as_star_args() {
        // Star and double-star arg names are defined.
        flakes(
            r"
        def f(a, *b, **c):
            print(a, b, c)
        ",
            &[],
        );
    }

    #[test]
    fn defined_as_star_unpack() {
        // Star names in unpack are defined.
        flakes(
            r"
        a, *b = range(10)
        print(a, b)
        ",
            &[],
        );
        flakes(
            r"
        *a, b = range(10)
        print(a, b)
        ",
            &[],
        );
        flakes(
            r"
        a, *b, c = range(10)
        print(a, b, c)
        ",
            &[],
        );
    }

    #[test]
    fn used_as_star_unpack() {
        // In stable, starred names in unpack are used if RHS is not a tuple/list literal.
        // In preview, these should be marked as unused.
        flakes(
            r"
        def f():
            a, *b = range(10)
        ",
            &[],
        );
        flakes(
            r"
        def f():
            (*a, b) = range(10)
        ",
            &[],
        );
        flakes(
            r"
        def f():
            [a, *b, c] = range(10)
        ",
            &[],
        );
    }

    #[test]
    fn keyword_only_args() {
        // Keyword-only arg names are defined.
        flakes(
            r"
        def f(*, a, b=None):
            print(a, b)
        ",
            &[],
        );
        flakes(
            r"
        import default_b
        def f(*, a, b=default_b):
            print(a, b)
        ",
            &[],
        );
    }

    #[test]
    fn keyword_only_args_undefined() {
        // Typo in kwonly name.
        flakes(
            r"
        def f(*, a, b=default_c):
            print(a, b)
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn annotation_undefined() {
        // Undefined annotations.
        flakes(
            r"
        from abc import note1, note2, note3, note4, note5
        def func(a: note1, *args: note2,
                 b: note3=12, **kw: note4) -> note5: pass
        ",
            &[],
        );

        flakes(
            r"
        def func():
            d = e = 42
            def func(a: {1, d}) -> (lambda c: e): pass
        ",
            &[],
        );
    }

    #[test]
    fn meta_class_undefined() {
        flakes(
            r"
        from abc import ABCMeta
        class A(metaclass=ABCMeta): pass
        ",
            &[],
        );
    }

    #[test]
    fn defined_in_gen_exp() {
        // Using the loop variable of a generator expression results in no
        // warnings.
        flakes("(a for a in [1, 2, 3] if a)", &[]);

        flakes("(b for b in (a for a in [1, 2, 3] if a) if b)", &[]);
    }

    #[test]
    fn undefined_in_gen_exp_nested() {
        // The loop variables of generator expressions nested together are
        // not defined in the other generator.
        flakes(
            "(b for b in (a for a in [1, 2, 3] if b) if b)",
            &[Rule::UndefinedName],
        );

        flakes(
            "(b for b in (a for a in [1, 2, 3] if a) if a)",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn undefined_with_error_handler() {
        // Some compatibility code checks explicitly for NameError.
        // It should not trigger warnings.
        flakes(
            r"
        try:
            socket_map
        except NameError:
            socket_map = {}
        ",
            &[],
        );
        flakes(
            r#"
        try:
            _memoryview.contiguous
        except (NameError, AttributeError):
            raise RuntimeError("Python >= 3.3 is required")
        "#,
            &[],
        );
        // If NameError is not explicitly handled, generate a warning.
        flakes(
            r"
        try:
            socket_map
        except:
            socket_map = {}
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        try:
            socket_map
        except Exception:
            socket_map = {}
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn defined_in_class() {
        // Defined name for generator expressions and dict/set comprehension.
        flakes(
            r"
        class A:
            T = range(10)

            Z = (x for x in T)
            L = [x for x in T]
            B = dict((i, str(i)) for i in T)
        ",
            &[],
        );

        flakes(
            r"
        class A:
            T = range(10)

            X = {x for x in T}
            Y = {x:x for x in T}
        ",
            &[],
        );

        flakes(
            r"
        class A:
            T = 1

            # In each case, `T` is undefined. Only the first `iter` uses the class scope.
            X = (T for x in range(10))
            Y = [x for x in range(10) if T]
            Z = [x for x in range(10) for y in T]
        ",
            &[
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
            ],
        );
    }

    #[test]
    fn defined_in_class_nested() {
        // Defined name for nested generator expressions in a class.
        flakes(
            r"
        class A:
            T = range(10)

            Z = (x for x in (a for a in T))
        ",
            &[],
        );
    }

    #[test]
    fn undefined_in_loop() {
        // The loop variable is defined after the expression is computed.
        flakes(
            r"
        for i in range(i):
            print(i)
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        [42 for i in range(i)]
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        (42 for i in range(i))
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn defined_from_lambda_in_dictionary_comprehension() {
        // Defined name referenced from a lambda function within a dict/set
        // comprehension.
        flakes(
            r"
        {lambda: id(x) for x in range(10)}
        ",
            &[],
        );
    }

    #[test]
    fn defined_from_lambda_in_generator() {
        // Defined name referenced from a lambda function within a generator
        // expression.
        flakes(
            r"
        any(lambda: id(x) for x in range(10))
        ",
            &[],
        );
    }

    #[test]
    fn undefined_from_lambda_in_dictionary_comprehension() {
        // Undefined name referenced from a lambda function within a dict/set
        // comprehension.
        flakes(
            r"
        {lambda: id(y) for x in range(10)}
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn undefined_from_lambda_in_comprehension() {
        // Undefined name referenced from a lambda function within a generator
        // expression.
        flakes(
            r"
        any(lambda: id(y) for x in range(10))
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn dunder_class() {
        flakes(
            r"
        class Test(object):
            def __init__(self):
                print(__class__.__name__)
                self.x = 1

        t = Test()
        ",
            &[],
        );
        flakes(
            r"
        class Test(object):
            print(__class__.__name__)

            def __init__(self):
                self.x = 1

        t = Test()
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        class Test(object):
            X = [__class__ for _ in range(10)]

            def __init__(self):
                self.x = 1

        t = Test()
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        def f(self):
            print(__class__.__name__)
            self.x = 1

        f()
        ",
            &[Rule::UndefinedName],
        );
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_imports.py>
    #[test]
    fn unused_import() {
        flakes("import fu, bar", &[Rule::UnusedImport, Rule::UnusedImport]);
        flakes(
            "from baz import fu, bar",
            &[Rule::UnusedImport, Rule::UnusedImport],
        );
    }

    #[test]
    fn unused_import_relative() {
        flakes("from . import fu", &[Rule::UnusedImport]);
        flakes("from . import fu as baz", &[Rule::UnusedImport]);
        flakes("from .. import fu", &[Rule::UnusedImport]);
        flakes("from ... import fu", &[Rule::UnusedImport]);
        flakes("from .. import fu as baz", &[Rule::UnusedImport]);
        flakes("from .bar import fu", &[Rule::UnusedImport]);
        flakes("from ..bar import fu", &[Rule::UnusedImport]);
        flakes("from ...bar import fu", &[Rule::UnusedImport]);
        flakes("from ...bar import fu as baz", &[Rule::UnusedImport]);
    }

    #[test]
    fn aliased_import() {
        flakes(
            "import fu as FU, bar as FU",
            &[Rule::RedefinedWhileUnused, Rule::UnusedImport],
        );
        flakes(
            "from moo import fu as FU, bar as FU",
            &[Rule::RedefinedWhileUnused, Rule::UnusedImport],
        );
    }

    #[test]
    fn aliased_import_shadow_module() {
        // Imported aliases can shadow the source of the import.
        flakes("from moo import fu as moo; moo", &[]);
        flakes("import fu as fu; fu", &[]);
        flakes("import fu.bar as fu; fu", &[]);
    }

    #[test]
    fn used_import() {
        flakes("import fu; print(fu)", &[]);
        flakes("from baz import fu; print(fu)", &[]);
        flakes("import fu; del fu", &[]);
    }

    #[test]
    fn used_import_relative() {
        flakes("from . import fu; assert fu", &[]);
        flakes("from .bar import fu; assert fu", &[]);
        flakes("from .. import fu; assert fu", &[]);
        flakes("from ..bar import fu as baz; assert baz", &[]);
    }

    #[test]
    fn redefined_while_unused() {
        flakes("import fu; fu = 3", &[Rule::RedefinedWhileUnused]);
        flakes("import fu; fu, bar = 3", &[Rule::RedefinedWhileUnused]);
        flakes("import fu; [fu, bar] = 3", &[Rule::RedefinedWhileUnused]);
    }

    #[test]
    fn redefined_if() {
        // Test that importing a module twice within an if
        // block does raise a warning.
        flakes(
            r"
        i = 2
        if i==1:
            import os
            import os
        os.path
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_if_else() {
        // Test that importing a module twice in if
        // and else blocks does not raise a warning.
        flakes(
            r"
        i = 2
        if i==1:
            import os
        else:
            import os
        os.path
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try() {
        // Test that importing a module twice in a try block
        // does raise a warning.
        flakes(
            r"
        try:
            import os
            import os
        except:
            pass
        os.path
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_try_except() {
        // Test that importing a module twice in a try
        // and except block does not raise a warning.
        flakes(
            r"
        try:
            import os
        except:
            import os
        os.path
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try_nested() {
        // Test that importing a module twice using a nested
        // try/except and if blocks does not issue a warning.
        flakes(
            r"
        try:
            if True:
                if True:
                    import os
        except:
            import os
        os.path
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try_except_multi() {
        flakes(
            r"
        try:
            from aa import mixer
        except AttributeError:
            from bb import mixer
        except RuntimeError:
            from cc import mixer
        except:
            from dd import mixer
        mixer(123)
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try_else() {
        flakes(
            r"
        try:
            from aa import mixer
        except ImportError:
            pass
        else:
            from bb import mixer
        mixer(123)
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_try_except_else() {
        flakes(
            r"
        try:
            import funca
        except ImportError:
            from bb import funca
            from bb import funcb
        else:
            from bbb import funcb
        print(funca, funcb)
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try_except_finally() {
        flakes(
            r"
        try:
            from aa import a
        except ImportError:
            from bb import a
        finally:
            a = 42
        print(a)
        ",
            &[],
        );
    }

    #[test]
    fn redefined_try_except_else_finally() {
        flakes(
            r"
        try:
            import b
        except ImportError:
            b = Ellipsis
            from bb import a
        else:
            from aa import a
        finally:
            a = 42
        print(a, b)
        ",
            &[],
        );
    }

    #[test]
    fn redefined_by_function() {
        flakes(
            r"
        import fu
        def fu():
            pass
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_in_nested_function() {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        flakes(
            r"
        import fu
        def bar():
            def baz():
                def fu():
                    pass
        ",
            &[Rule::UnusedImport, Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_in_nested_function_twice() {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        flakes(
            r"
        import fu
        def bar():
            import fu
            def baz():
                def fu():
                    pass
        ",
            &[
                Rule::UnusedImport,
                Rule::RedefinedWhileUnused,
                Rule::UnusedImport,
                Rule::RedefinedWhileUnused,
            ],
        );
    }

    #[test]
    fn redefined_but_used_later() {
        // Test that a global import which is redefined locally,
        // but used later in another scope does not generate a warning.
        flakes(
            r"
        import unittest, transport

        class GetTransportTestCase(unittest.TestCase):
            def test_get_transport(self):
                transport = 'transport'
                self.assertIsNotNone(transport)

        class TestTransportMethodArgs(unittest.TestCase):
            def test_send_defaults(self):
                transport.Transport()",
            &[],
        );
    }

    #[test]
    fn redefined_by_class() {
        flakes(
            r"
        import fu
        class fu:
            pass
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn redefined_by_subclass() {
        // If an imported name is redefined by a class statement which also uses
        // that name in the bases list, no warning is emitted.
        flakes(
            r"
        from fu import bar
        class bar(bar):
            pass
        ",
            &[],
        );
    }

    #[test]
    fn redefined_in_class() {
        // Test that shadowing a global with a class attribute does not produce a
        // warning.
        flakes(
            r"
        import fu
        class bar:
            fu = 1
        print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn import_in_class() {
        // Test that import within class is a locally scoped attribute.
        flakes(
            r"
        class bar:
            import fu
        ",
            &[],
        );

        flakes(
            r"
        class bar:
            import fu

        fu
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn used_in_function() {
        flakes(
            r"
        import fu
        def fun():
            print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn shadowed_by_parameter() {
        flakes(
            r"
        import fu
        def fun(fu):
            print(fu)
        ",
            &[Rule::UnusedImport, Rule::RedefinedWhileUnused],
        );

        flakes(
            r"
        import fu
        def fun(fu):
            print(fu)
        print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn new_assignment() {
        flakes("fu = None", &[]);
    }

    #[test]
    fn used_in_getattr() {
        flakes("import fu; fu.bar.baz", &[]);
        flakes("import fu; \"bar\".fu.baz", &[Rule::UnusedImport]);
    }

    #[test]
    fn used_in_slice() {
        flakes("import fu; print(fu.bar[1:])", &[]);
    }

    #[test]
    fn used_in_if_body() {
        flakes(
            r"
        import fu
        if True: print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn used_in_if_conditional() {
        flakes(
            r"
        import fu
        if fu: pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_elif_conditional() {
        flakes(
            r"
        import fu
        if False: pass
        elif fu: pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_else() {
        flakes(
            r"
        import fu
        if False: pass
        else: print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn used_in_call() {
        flakes("import fu; fu.bar()", &[]);
    }

    #[test]
    fn used_in_class() {
        flakes(
            r"
        import fu
        class bar:
            bar = fu
        ",
            &[],
        );
    }

    #[test]
    fn used_in_class_base() {
        flakes(
            r"
        import fu
        class bar(object, fu.baz):
            pass
        ",
            &[],
        );
    }

    #[test]
    fn not_used_in_nested_scope() {
        flakes(
            r"
        import fu
        def bleh():
            pass
        print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn used_in_for() {
        flakes(
            r"
        import fu
        for bar in range(9):
            print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn used_in_for_else() {
        flakes(
            r"
        import fu
        for bar in range(10):
            pass
        else:
            print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn redefined_by_for() {
        flakes(
            r"
        import fu
        for fu in range(2):
            pass
        ",
            &[Rule::ImportShadowedByLoopVar],
        );
    }

    #[test]
    fn shadowed_by_for() {
        // Test that shadowing a global name with a for loop variable generates a
        // warning.
        flakes(
            r"
        import fu
        fu.bar()
        for fu in ():
            pass
        ",
            &[Rule::ImportShadowedByLoopVar],
        );
    }

    #[test]
    fn shadowed_by_for_deep() {
        // Test that shadowing a global name with a for loop variable nested in a
        // tuple unpack generates a warning.
        flakes(
            r"
        import fu
        fu.bar()
        for (x, y, z, (a, b, c, (fu,))) in ():
            pass
        ",
            &[Rule::ImportShadowedByLoopVar],
        );
        flakes(
            r"
        import fu
        fu.bar()
        for [x, y, z, (a, b, c, (fu,))] in ():
            pass
        ",
            &[Rule::ImportShadowedByLoopVar],
        );
    }

    #[test]
    fn used_in_return() {
        flakes(
            r"
        import fu
        def fun():
            return fu
        ",
            &[],
        );
    }

    #[test]
    fn used_in_operators() {
        flakes("import fu; 3 + fu.bar", &[]);
        flakes("import fu; 3 % fu.bar", &[]);
        flakes("import fu; 3 - fu.bar", &[]);
        flakes("import fu; 3 * fu.bar", &[]);
        flakes("import fu; 3 ** fu.bar", &[]);
        flakes("import fu; 3 / fu.bar", &[]);
        flakes("import fu; 3 // fu.bar", &[]);
        flakes("import fu; -fu.bar", &[]);
        flakes("import fu; ~fu.bar", &[]);
        flakes("import fu; 1 == fu.bar", &[]);
        flakes("import fu; 1 | fu.bar", &[]);
        flakes("import fu; 1 & fu.bar", &[]);
        flakes("import fu; 1 ^ fu.bar", &[]);
        flakes("import fu; 1 >> fu.bar", &[]);
        flakes("import fu; 1 << fu.bar", &[]);
    }

    #[test]
    fn used_in_assert() {
        flakes("import fu; assert fu.bar", &[]);
    }

    #[test]
    fn used_in_subscript() {
        flakes("import fu; fu.bar[1]", &[]);
    }

    #[test]
    fn used_in_logic() {
        flakes("import fu; fu and False", &[]);
        flakes("import fu; fu or False", &[]);
        flakes("import fu; not fu.bar", &[]);
    }

    #[test]
    fn used_in_list() {
        flakes("import fu; [fu]", &[]);
    }

    #[test]
    fn used_in_tuple() {
        flakes("import fu; (fu,)", &[]);
    }

    #[test]
    fn used_in_try() {
        flakes(
            r"
        import fu
        try: fu
        except: pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_except() {
        flakes(
            r"
        import fu
        try: fu
        except: pass
        ",
            &[],
        );
    }

    #[test]
    fn redefined_by_except() {
        flakes(
            r"
        import fu
        try: pass
        except Exception as fu: pass
        ",
            &[Rule::UnusedVariable, Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn used_in_raise() {
        flakes(
            r"
        import fu
        raise fu.bar
        ",
            &[],
        );
    }

    #[test]
    fn used_in_yield() {
        flakes(
            r"
        import fu
        def gen():
            yield fu
        ",
            &[],
        );
    }

    #[test]
    fn used_in_dict() {
        flakes("import fu; {fu:None}", &[]);
        flakes("import fu; {1:fu}", &[]);
    }

    #[test]
    fn used_in_parameter_default() {
        flakes(
            r"
        import fu
        def f(bar=fu):
            pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_attribute_assign() {
        flakes("import fu; fu.bar = 1", &[]);
    }

    #[test]
    fn used_in_keyword_arg() {
        flakes("import fu; fu.bar(stuff=fu)", &[]);
    }

    #[test]
    fn used_in_assignment() {
        flakes("import fu; bar=fu", &[]);
        flakes("import fu; n=0; n+=fu", &[]);
    }

    #[test]
    fn used_in_list_comp() {
        flakes("import fu; [fu for _ in range(1)]", &[]);
        flakes("import fu; [1 for _ in range(1) if fu]", &[]);
    }

    #[test]
    fn used_in_try_finally() {
        flakes(
            r"
        import fu
        try: pass
        finally: fu
        ",
            &[],
        );

        flakes(
            r"
        import fu
        try: fu
        finally: pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_while() {
        flakes(
            r"
        import fu
        while 0:
            fu
        ",
            &[],
        );

        flakes(
            r"
        import fu
        while fu: pass
        ",
            &[],
        );
    }

    #[test]
    fn used_in_global() {
        // A 'global' statement shadowing an unused import should not prevent it
        // from being reported.
        flakes(
            r"
        import fu
        def f(): global fu
        ",
            &[Rule::UnusedImport],
        );
    }

    #[test]
    fn used_and_global() {
        // A 'global' statement shadowing a used import should not cause it to be
        // reported as unused.
        flakes(
            r"
            import foo
            def f(): global foo
            def g(): foo.is_used()
        ",
            &[],
        );
    }

    #[test]
    fn assigned_to_global() {
        // Binding an import to a declared global should not cause it to be
        // reported as unused.
        flakes(
            r"
            def f(): global foo; import foo
            def g(): foo.is_used()
        ",
            &[],
        );
    }

    #[test]
    fn used_in_exec() {
        flakes("import fu; exec('print(1)', fu.bar)", &[]);
    }

    #[test]
    fn used_in_lambda() {
        flakes(
            r"import fu;
lambda: fu
        ",
            &[],
        );
    }

    #[test]
    fn shadowed_by_lambda() {
        flakes(
            "import fu; lambda fu: fu",
            &[Rule::UnusedImport, Rule::RedefinedWhileUnused],
        );
        flakes("import fu; lambda fu: fu\nfu()", &[]);
    }

    #[test]
    fn used_in_slice_obj() {
        flakes(
            r#"import fu;
"meow"[::fu]
        "#,
            &[],
        );
    }

    #[test]
    fn unused_in_nested_scope() {
        flakes(
            r"
        def bar():
            import fu
        fu
        ",
            &[Rule::UnusedImport, Rule::UndefinedName],
        );
    }

    #[test]
    fn methods_dont_use_class_scope() {
        flakes(
            r"
        class bar:
            import fu
            def fun(self):
                fu
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn nested_functions_nest_scope() {
        flakes(
            r"
        def a():
            def b():
                fu
            import fu
        ",
            &[],
        );
    }

    #[test]
    fn nested_class_and_function_scope() {
        flakes(
            r"
        def a():
            import fu
            class b:
                def c(self):
                    print(fu)
        ",
            &[],
        );
    }

    #[test]
    fn package_import() {
        // If a dotted name is imported and used, no warning is reported.
        flakes(
            r"
        import fu.bar
        fu.bar
        ",
            &[],
        );
    }

    #[test]
    fn unused_package_import() {
        // If a dotted name is imported and not used, an unused import warning is
        // reported.
        flakes("import fu.bar", &[Rule::UnusedImport]);
    }

    #[test]
    fn duplicate_submodule_import() {
        // If a submodule of a package is imported twice, an unused import warning and a
        // redefined while unused warning are reported.
        flakes(
            r"
        import fu.bar, fu.bar
        fu.bar
        ",
            &[Rule::RedefinedWhileUnused],
        );
        flakes(
            r"
        import fu.bar
        import fu.bar
        fu.bar
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn different_submodule_import() {
        // If two different submodules of a package are imported, no duplicate import
        // warning is reported for the package.
        flakes(
            r"
        import fu.bar, fu.baz
        fu.bar, fu.baz
        ",
            &[],
        );
        flakes(
            r"
        import fu.bar
        import fu.baz
        fu.bar, fu.baz
        ",
            &[],
        );
    }

    #[test]
    fn aliased_submodule_import() {
        flakes(
            r"
        import fu.bar as baz
        import fu.bar as baz
        baz
        ",
            &[Rule::RedefinedWhileUnused],
        );

        flakes(
            r"
        import fu.bar as baz
        import baz
        baz
        ",
            &[Rule::RedefinedWhileUnused],
        );

        flakes(
            r"
        import fu.bar as baz
        import fu.bar as bop
        baz, bop
        ",
            &[],
        );

        flakes(
            r"
        import foo.baz
        import foo.baz as foo
        foo
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn used_package_with_submodule_import() {
        // Usage of package marks submodule imports as used.
        flakes(
            r"
        import fu
        import fu.bar
        fu.x
        ",
            &[],
        );

        flakes(
            r"
        import fu.bar
        import fu
        fu.x
        ",
            &[],
        );
    }

    #[test]
    fn used_package_with_submodule_import_of_alias() {
        // Usage of package by alias marks submodule imports as used.
        flakes(
            r"
        import foo as f
        import foo.bar
        f.bar.do_something()
        ",
            &[],
        );

        flakes(
            r"
        import foo as f
        import foo.bar.blah
        f.bar.blah.do_something()
        ",
            &[],
        );
    }

    #[test]
    fn unused_package_with_submodule_import() {
        // When a package and its submodule are imported, only report once.
        flakes(
            r"
        import fu
        import fu.bar
        ",
            &[Rule::UnusedImport],
        );
    }

    #[test]
    fn assign_rhs_first() {
        flakes("import fu; fu = fu", &[]);
        flakes("import fu; fu, bar = fu", &[]);
        flakes("import fu; [fu, bar] = fu", &[]);
        flakes("import fu; fu += fu", &[]);
    }

    #[test]
    fn trying_multiple_imports() {
        flakes(
            r"
        try:
            import fu
        except ImportError:
            import bar as fu
        fu
        ",
            &[],
        );
    }

    #[test]
    fn non_global_does_not_redefine() {
        flakes(
            r"
        import fu
        def a():
            fu = 3
            return fu
        fu
        ",
            &[],
        );
    }

    #[test]
    fn functions_run_later() {
        flakes(
            r"
        def a():
            fu
        import fu
        ",
            &[],
        );
    }

    #[test]
    fn function_names_are_bound_now() {
        flakes(
            r"
        import fu
        def fu():
            fu
        fu
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn ignore_non_import_redefinitions() {
        flakes("a = 1; a = 2", &[]);
    }

    #[test]
    fn imported_in_class() {
        // Imports in class scope can be used through self.
        flakes(
            r"
        class C:
            import i
            def __init__(self):
                self.i
        ",
            &[],
        );
    }

    #[test]
    fn import_used_in_method_definition() {
        // Method named 'foo' with default args referring to module named 'foo'.
        flakes(
            r"
        import foo

        class Thing(object):
            def foo(self, parser=foo.parse_foo):
                pass
        ",
            &[],
        );
    }

    #[test]
    fn future_import() {
        // __future__ is special.
        flakes("from __future__ import division", &[]);
        flakes(
            r#"
        "docstring is allowed before future import"
        from __future__ import division
        "#,
            &[],
        );
    }

    #[test]
    fn future_import_first() {
        // __future__ imports must come before anything else.
        flakes(
            r"
        x = 5
        from __future__ import division
        ",
            &[Rule::LateFutureImport],
        );
        flakes(
            r"
        from foo import bar
        from __future__ import division
        bar
        ",
            &[Rule::LateFutureImport],
        );
    }

    #[test]
    fn future_import_used() {
        // __future__ is special, but names are injected in the namespace.
        flakes(
            r"
        from __future__ import division
        from __future__ import print_function

        assert print_function is not division
        ",
            &[],
        );
    }

    #[test]
    fn future_import_undefined() {
        // Importing undefined names from __future__ fails.
        flakes(
            r"
        from __future__ import print_statement
        ",
            &[Rule::FutureFeatureNotDefined],
        );
    }

    #[test]
    fn future_import_star() {
        // Importing '*' from __future__ fails.
        flakes(
            r"
        from __future__ import *
        ",
            &[Rule::FutureFeatureNotDefined],
        );
    }

    #[test]
    fn ignored_in_function() {
        // An C{__all__} definition does not suppress unused import warnings in a
        // function scope.
        flakes(
            r#"
        def foo():
            import bar
            __all__ = ["bar"]
        "#,
            &[Rule::UnusedImport, Rule::UnusedVariable],
        );
    }

    #[test]
    fn ignored_in_class() {
        // An C{__all__} definition in a class does not suppress unused import warnings.
        flakes(
            r#"
        import bar
        class foo:
            __all__ = ["bar"]
        "#,
            &[Rule::UnusedImport],
        );
    }

    #[test]
    fn ignored_when_not_directly_assigned() {
        flakes(
            r#"
        import bar
        (__all__,) = ("foo",)
        "#,
            &[Rule::UnusedImport],
        );
    }

    #[test]
    fn warning_suppressed() {
        // If a name is imported and unused but is named in C{__all__}, no warning
        // is reported.
        flakes(
            r#"
        import foo
        __all__ = ["foo"]
        "#,
            &[],
        );
        flakes(
            r#"
        import foo
        __all__ = ("foo",)
        "#,
            &[],
        );
    }

    #[test]
    fn augmented_assignment() {
        // The C{__all__} variable is defined incrementally.
        flakes(
            r"
        import a
        import c
        __all__ = ['a']
        __all__ += ['b']
        if 1 < 3:
            __all__ += ['c', 'd']
        ",
            &[Rule::UndefinedExport, Rule::UndefinedExport],
        );
    }

    #[test]
    fn list_concatenation_assignment() {
        // The C{__all__} variable is defined through list concatenation.
        flakes(
            r"
        import sys
        __all__ = ['a'] + ['b'] + ['c']
        ",
            &[
                Rule::UnusedImport,
                Rule::UndefinedExport,
                Rule::UndefinedExport,
                Rule::UndefinedExport,
            ],
        );
    }

    #[test]
    fn tuple_concatenation_assignment() {
        // The C{__all__} variable is defined through tuple concatenation.
        flakes(
            r"
        import sys
        __all__ = ('a',) + ('b',) + ('c',)
        ",
            &[
                Rule::UnusedImport,
                Rule::UndefinedExport,
                Rule::UndefinedExport,
                Rule::UndefinedExport,
            ],
        );
    }

    #[test]
    fn all_with_attributes() {
        flakes(
            r"
        from foo import bar
        __all__ = [bar.__name__]
        ",
            &[],
        );
    }

    #[test]
    fn all_with_names() {
        flakes(
            r"
        from foo import bar
        __all__ = [bar]
        ",
            &[],
        );
    }

    #[test]
    fn all_with_attributes_added() {
        flakes(
            r"
        from foo import bar
        from bar import baz
        __all__ = [bar.__name__] + [baz.__name__]
        ",
            &[],
        );
    }

    #[test]
    fn all_mixed_attributes_and_strings() {
        flakes(
            r"
        from foo import bar
        from foo import baz
        __all__ = ['bar', baz.__name__]
        ",
            &[],
        );
    }

    #[test]
    fn unbound_exported() {
        // If C{__all__} includes a name which is not bound, a warning is emitted.
        flakes(
            r#"
        __all__ = ["foo"]
        "#,
            &[Rule::UndefinedExport],
        );
    }

    #[test]
    fn import_star_exported() {
        // Report undefined if import * is used
        flakes(
            r"
        from math import *
        __all__ = ['sin', 'cos']
        csc(1)
        ",
            &[
                Rule::UndefinedLocalWithImportStar,
                Rule::UndefinedLocalWithImportStarUsage,
                Rule::UndefinedLocalWithImportStarUsage,
                Rule::UndefinedLocalWithImportStarUsage,
            ],
        );
    }

    #[ignore]
    #[test]
    fn import_star_not_exported() {
        // Report unused import when not needed to satisfy __all__.
        flakes(
            r"
        from foolib import *
        a = 1
        __all__ = ['a']
        ",
            &[Rule::UndefinedLocalWithImportStar, Rule::UnusedImport],
        );
    }

    #[test]
    fn used_in_gen_exp() {
        // Using a global in a generator expression results in no warnings.
        flakes("import fu; (fu for _ in range(1))", &[]);
        flakes("import fu; (1 for _ in range(1) if fu)", &[]);
    }

    #[test]
    fn redefined_by_gen_exp() {
        // Reusing a global name as the loop variable for a generator
        // expression results in a redefinition warning.
        flakes(
            "import fu; (1 for fu in range(1))",
            &[Rule::UnusedImport, Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn used_as_decorator() {
        // Using a global name in a decorator statement results in no warnings,
        // but using an undefined name in a decorator statement results in an
        // undefined name warning.
        flakes(
            r#"
        from interior import decorate
        @decorate
        def f():
            return "hello"
        "#,
            &[],
        );

        flakes(
            r#"
        @decorate
        def f():
            return "hello"
        "#,
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn used_as_class_decorator() {
        // Using an imported name as a class decorator results in no warnings,
        // but using an undefined name as a class decorator results in an
        // undefined name warning.
        flakes(
            r"
        from interior import decorate
        @decorate
        class foo:
            pass
        ",
            &[],
        );

        flakes(
            r#"
        from interior import decorate
        @decorate("foo")
        class bar:
            pass
        "#,
            &[],
        );

        flakes(
            r"
        @decorate
        class foo:
            pass
        ",
            &[Rule::UndefinedName],
        );
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_type_annotations.py>
    #[test]
    fn typing_overload() {
        // Allow intentional redefinitions via @typing.overload.
        flakes(
            r"
        import typing
        from typing import overload

        @overload
        def f(s: None) -> None:
            pass

        @overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s

        @typing.overload
        def g(s: None) -> None:
            pass

        @typing.overload
        def g(s: int) -> int:
            pass

        def g(s):
            return s
        ",
            &[],
        );
    }

    #[test]
    fn typing_extensions_overload() {
        // Allow intentional redefinitions via @typing_extensions.overload.
        flakes(
            r"
        import typing_extensions
        from typing_extensions import overload

        @overload
        def f(s: None) -> None:
            pass

        @overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s

        @typing_extensions.overload
        def g(s: None) -> None:
            pass

        @typing_extensions.overload
        def g(s: int) -> int:
            pass

        def g(s):
            return s
        ",
            &[],
        );
    }

    #[test]
    fn typing_overload_async() {
        // Allow intentional redefinitions via @typing.overload (async).
        flakes(
            r"
        from typing import overload

        @overload
        async def f(s: None) -> None:
            pass

        @overload
        async def f(s: int) -> int:
            pass

        async def f(s):
            return s
        ",
            &[],
        );
    }

    #[test]
    fn overload_with_multiple_decorators() {
        flakes(
            r"
            from typing import overload
            dec = lambda f: f

            @dec
            @overload
            def f(x: int) -> int:
                pass

            @dec
            @overload
            def f(x: str) -> str:
                pass

            @dec
            def f(x): return x
       ",
            &[],
        );
    }

    #[test]
    fn overload_in_class() {
        flakes(
            r"
        from typing import overload

        class C:
            @overload
            def f(self, x: int) -> int:
                pass

            @overload
            def f(self, x: str) -> str:
                pass

            def f(self, x): return x
        ",
            &[],
        );
    }

    #[test]
    fn aliased_typing_import() {
        // Detect when typing is imported as another name.
        flakes(
            r"
        import typing as t

        @t.overload
        def f(s: None) -> None:
            pass

        @t.overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s
        ",
            &[],
        );
    }

    #[test]
    fn not_a_typing_overload() {
        // regression test for @typing.overload detection bug in 2.1.0.
        flakes(
            r"
            def foo(x):
                return x

            @foo
            def bar():
                pass

            def bar():
                pass
        ",
            &[Rule::RedefinedWhileUnused],
        );
    }

    #[test]
    fn variable_annotations() {
        flakes(
            r"
        name: str
        age: int
        ",
            &[],
        );
        flakes(
            r"
        name: str = 'Bob'
        age: int = 18
        ",
            &[],
        );
        flakes(
            r"
        class C:
            name: str
            age: int
        ",
            &[],
        );
        flakes(
            r"
        class C:
            name: str = 'Bob'
            age: int = 18
        ",
            &[],
        );
        flakes(
            r"
        def f():
            name: str
            age: int
        ",
            &[Rule::UnusedAnnotation, Rule::UnusedAnnotation],
        );
        flakes(
            r"
        def f():
            name: str = 'Bob'
            age: int = 18
            foo: not_a_real_type = None
        ",
            &[
                Rule::UnusedVariable,
                Rule::UnusedVariable,
                Rule::UnusedVariable,
                Rule::UndefinedName,
            ],
        );
        flakes(
            r"
        def f():
            name: str
            print(name)
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        from typing import Any
        def f():
            a: Any
        ",
            &[Rule::UnusedAnnotation],
        );
        flakes(
            r"
        foo: not_a_real_type
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        foo: not_a_real_type = None
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        class C:
            foo: not_a_real_type
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        class C:
            foo: not_a_real_type = None
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        def f():
            class C:
                foo: not_a_real_type
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        def f():
            class C:
                foo: not_a_real_type = None
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        from foo import Bar
        bar: Bar
        ",
            &[],
        );
        flakes(
            r"
        from foo import Bar
        bar: 'Bar'
        ",
            &[],
        );
        flakes(
            r"
        import foo
        bar: foo.Bar
        ",
            &[],
        );
        flakes(
            r"
        import foo
        bar: 'foo.Bar'
        ",
            &[],
        );
        flakes(
            r"
        from foo import Bar
        def f(bar: Bar): pass
        ",
            &[],
        );
        flakes(
            r"
        from foo import Bar
        def f(bar: 'Bar'): pass
        ",
            &[],
        );
        flakes(
            r"
        from foo import Bar
        def f(bar) -> Bar: return bar
        ",
            &[],
        );
        flakes(
            r"
        from foo import Bar
        def f(bar) -> 'Bar': return bar
        ",
            &[],
        );
        flakes(
            r"
        bar: 'Bar'
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        bar: 'foo.Bar'
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        from foo import Bar
        bar: str
        ",
            &[Rule::UnusedImport],
        );
        flakes(
            r"
        from foo import Bar
        def f(bar: str): pass
        ",
            &[Rule::UnusedImport],
        );
        flakes(
            r"
        def f(a: A) -> A: pass
        class A: pass
        ",
            &[Rule::UndefinedName, Rule::UndefinedName],
        );
        flakes(
            r"
        def f(a: 'A') -> 'A': return a
        class A: pass
        ",
            &[],
        );
        flakes(
            r"
        a: A
        class A: pass
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        a: 'A'
        class A: pass
        ",
            &[],
        );
        flakes(
            r"
        T: object
        def f(t: T): pass
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        T: object
        def g(t: 'T'): pass
        ",
            &[Rule::UndefinedName],
        );
        flakes(
            r"
        T = object
        def f(t: T): pass
        ",
            &[],
        );
        flakes(
            r"
        T = object
        def g(t: 'T'): pass
        ",
            &[],
        );
        flakes(
            r"
        a: 'A B'
        ",
            &[Rule::ForwardAnnotationSyntaxError],
        );
        flakes(
            r"
        a: 'A; B'
        ",
            &[Rule::ForwardAnnotationSyntaxError],
        );
        flakes(
            r"
        a: '1 + 2'
        ",
            &[],
        );
        flakes(
            r#"
        a: 'a: "A"'
        "#,
            &[Rule::ForwardAnnotationSyntaxError],
        );
    }

    #[test]
    fn variable_annotation_references_self_name_undefined() {
        flakes(
            r"
        x: int = x
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn use_pep695_type_aliass() {
        flakes(
            r"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias = Bar
        ",
            &[],
        );
        flakes(
            r"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias = 'Bar'
        ",
            &[],
        );
        flakes(
            r"
        from typing_extensions import TypeAlias
        from foo import Bar

        class A:
            bar: TypeAlias = Bar
        ",
            &[],
        );
        flakes(
            r"
        from typing_extensions import TypeAlias
        from foo import Bar

        class A:
            bar: TypeAlias = 'Bar'
        ",
            &[],
        );
        flakes(
            r"
        from typing_extensions import TypeAlias

        bar: TypeAlias
        ",
            &[],
        );
        flakes(
            r"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias
        ",
            &[Rule::UnusedImport],
        );
    }

    #[test]
    fn annotating_an_import() {
        flakes(
            r"
            from a import b, c
            b: c
            print(b)
        ",
            &[],
        );
    }

    #[test]
    fn unused_annotation() {
        // Unused annotations are fine in module and class scope.
        flakes(
            r"
        x: int
        class Cls:
            y: int
        ",
            &[],
        );
        flakes(
            r"
        def f():
            x: int
        ",
            &[Rule::UnusedAnnotation],
        );
        // This should only print one UnusedVariable message.
        flakes(
            r"
        def f():
            x: int
            x = 3
        ",
            &[Rule::UnusedVariable],
        );
    }

    #[test]
    fn unassigned_annotation_is_undefined() {
        flakes(
            r"
        name: str
        print(name)
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn annotated_async_def() {
        flakes(
            r"
        class c: pass
        async def func(c: c) -> None: pass
        ",
            &[],
        );
    }

    #[test]
    fn postponed_annotations() {
        flakes(
            r"
        from __future__ import annotations
        def f(a: A) -> A: pass
        class A:
            b: B
        class B: pass
        ",
            &[],
        );

        flakes(
            r"
        from __future__ import annotations
        def f(a: A) -> A: pass
        class A:
            b: Undefined
        class B: pass
        ",
            &[Rule::UndefinedName],
        );

        flakes(
            r"
        from __future__ import annotations
        T: object
        def f(t: T): pass
        def g(t: 'T'): pass
        ",
            &[Rule::UndefinedName, Rule::UndefinedName],
        );

        flakes(
            r"
        from __future__ import annotations
        T = object
        def f(t: T): pass
        def g(t: 'T'): pass
        ",
            &[],
        );
    }

    #[test]
    fn type_annotation_clobbers_all() {
        flakes(
            r#"
        from typing import TYPE_CHECKING, List

        from y import z

        if not TYPE_CHECKING:
            __all__ = ("z",)
        else:
            __all__: List[str]
        "#,
            &[],
        );
    }

    #[test]
    fn return_annotation_is_class_scope_variable() {
        flakes(
            r"
        from typing import TypeVar
        class Test:
            Y = TypeVar('Y')

            def t(self, x: Y) -> Y:
                return x
        ",
            &[],
        );
    }

    #[test]
    fn return_annotation_is_function_body_variable() {
        flakes(
            r"
        class Test:
            def t(self) -> Y:
                Y = 2
                return Y
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn positional_only_argument_annotations() {
        flakes(
            r"
        from x import C

        def f(c: C, /): ...
        ",
            &[],
        );
    }

    #[test]
    fn partially_quoted_type_annotation() {
        flakes(
            r"
        from queue import Queue
        from typing import Optional

        def f() -> Optional['Queue[str]']:
            return None
        ",
            &[],
        );
    }

    #[test]
    fn partially_quoted_type_assignment() {
        flakes(
            r"
        from queue import Queue
        from typing import Optional

        MaybeQueue = Optional['Queue[str]']
        ",
            &[],
        );
    }

    #[test]
    fn nested_partially_quoted_type_assignment() {
        flakes(
            r"
        from queue import Queue
        from typing import Callable

        Func = Callable[['Queue[str]'], None]
        ",
            &[],
        );
    }

    #[test]
    fn quoted_type_cast() {
        flakes(
            r"
        from typing import cast, Optional

        maybe_int = cast('Optional[int]', 42)
        ",
            &[],
        );
    }

    #[test]
    fn type_cast_literal_str_to_str() {
        // Checks that our handling of quoted type annotations in the first
        // argument to `cast` doesn't cause issues when (only) the _second_
        // argument is a literal str which looks a bit like a type annotation.
        flakes(
            r"
        from typing import cast

        a_string = cast(str, 'Optional[int]')
        ",
            &[],
        );
    }

    #[test]
    fn quoted_type_cast_renamed_import() {
        flakes(
            r"
        from typing import cast as tsac, Optional as Maybe

        maybe_int = tsac('Maybe[int]', 42)
        ",
            &[],
        );
    }

    #[test]
    fn quoted_type_var_constraints() {
        flakes(
            r"
        from typing import TypeVar, Optional

        T = TypeVar('T', 'str', 'Optional[int]', bytes)
        ",
            &[],
        );
    }

    #[test]
    fn quoted_type_var_bound() {
        flakes(
            r"
        from typing import TypeVar, Optional, List

        T = TypeVar('T', bound='Optional[int]')
        S = TypeVar('S', int, bound='List[int]')
        ",
            &[],
        );
    }

    #[test]
    fn literal_type_typing() {
        flakes(
            r"
        from typing import Literal

        def f(x: Literal['some string']) -> None:
            return None
        ",
            &[],
        );
    }

    #[test]
    fn literal_type_typing_extensions() {
        flakes(
            r"
        from typing_extensions import Literal

        def f(x: Literal['some string']) -> None:
            return None
        ",
            &[],
        );
    }

    #[test]
    fn annotated_type_typing_missing_forward_type_multiple_args() {
        flakes(
            r"
        from typing import Annotated

        def f(x: Annotated['integer', 1]) -> None:
            return None
        ",
            &[Rule::UndefinedName],
        );
    }

    #[test]
    fn annotated_type_typing_with_string_args() {
        flakes(
            r"
        from typing import Annotated

        def f(x: Annotated[int, '> 0']) -> None:
            return None
        ",
            &[],
        );
    }

    #[test]
    fn annotated_type_typing_with_string_args_in_union() {
        flakes(
            r"
        from typing import Annotated, Union

        def f(x: Union[Annotated['int', '>0'], 'integer']) -> None:
            return None
        ",
            &[Rule::UndefinedName],
        );
    }

    // We err on the side of assuming strings are forward references.
    #[ignore]
    #[test]
    fn literal_type_some_other_module() {
        // err on the side of false-negatives for types named Literal.
        flakes(
            r"
        from my_module import compat
        from my_module.compat import Literal

        def f(x: compat.Literal['some string']) -> None:
            return None
        def g(x: Literal['some string']) -> None:
            return None
        ",
            &[],
        );
    }

    #[test]
    fn literal_union_type_typing() {
        flakes(
            r"
        from typing import Literal

        def f(x: Literal['some string', 'foo bar']) -> None:
            return None
        ",
            &[],
        );
    }

    // TODO(charlie): Support nested deferred string annotations.
    #[ignore]
    #[test]
    fn deferred_twice_annotation() {
        flakes(
            r#"
            from queue import Queue
            from typing import Optional

            def f() -> "Optional['Queue[str]']":
                return None
        "#,
            &[],
        );
    }

    #[test]
    fn partial_string_annotations_with_future_annotations() {
        flakes(
            r"
            from __future__ import annotations

            from queue import Queue
            from typing import Optional

            def f() -> Optional['Queue[str]']:
                return None
        ",
            &[],
        );
    }

    #[test]
    fn forward_annotations_for_classes_in_scope() {
        flakes(
            r#"
        from typing import Optional

        def f():
            class C:
                a: "D"
                b: Optional["D"]
                c: "Optional[D]"

            class D: pass
        "#,
            &[],
        );
    }

    #[test]
    fn idiomiatic_typing_guards() {
        // typing.TYPE_CHECKING: python3.5.3+.
        flakes(
            r"
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                from t import T

            def f() -> T:
                pass
        ",
            &[],
        );
        // False: the old, more-compatible approach.
        flakes(
            r"
            if False:
                from t import T

            def f() -> T:
                pass
        ",
            &[],
        );
        // Some choose to assign a constant and do it that way.
        flakes(
            r"
            MYPY = False

            if MYPY:
                from t import T

            def f() -> T:
                pass
        ",
            &[],
        );
    }

    #[test]
    fn typing_guard_for_protocol() {
        flakes(
            r"
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                from typing import Protocol
            else:
                Protocol = object

            class C(Protocol):
                def f() -> int:
                    pass
        ",
            &[],
        );
    }

    #[test]
    fn typed_names_correct_forward_ref() {
        flakes(
            r#"
            from typing import TypedDict, List, NamedTuple

            List[TypedDict("x", {})]
            List[TypedDict("x", x=int)]
            List[NamedTuple("a", a=int)]
            List[NamedTuple("a", [("a", int)])]
        "#,
            &[],
        );
        flakes(
            r#"
            from typing import TypedDict, List, NamedTuple, TypeVar

            List[TypedDict("x", {"x": "Y"})]
            List[TypedDict("x", x="Y")]
            List[NamedTuple("a", [("a", "Y")])]
            List[NamedTuple("a", a="Y")]
            List[TypedDict("x", {"x": List["a"]})]
            List[TypeVar("A", bound="C")]
            List[TypeVar("A", List["C"])]
        "#,
            &[
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
                Rule::UndefinedName,
            ],
        );
        flakes(
            r#"
            from typing import NamedTuple, TypeVar, cast
            from t import A, B, C, D, E

            NamedTuple("A", [("a", A["C"])])
            TypeVar("A", bound=A["B"])
            TypeVar("A", A["D"])
            cast(A["E"], [])
        "#,
            &[],
        );
        flakes(
            r#"
            from typing import NewType

            def f():
                name = "x"
                NewType(name, int)
        "#,
            &[],
        );
    }

    #[test]
    fn named_types_classes() {
        flakes(
            r#"
            from typing import TypedDict, NamedTuple
            class X(TypedDict):
                y: TypedDict("z", {"zz":int})

            class Y(NamedTuple):
                y: NamedTuple("v", [("vv", int)])
        "#,
            &[],
        );
    }

    #[test]
    fn gh_issue_17196_regression_test() {
        flakes(
            r#"
            from typing import Annotated

            def type_annotations_from_tuple():
                annos = (str, "foo", "bar")
                return Annotated[annos]

            def type_annotations_from_filtered_tuple():
                annos = (str, None, "foo", None, "bar")
                return Annotated[tuple([a for a in annos if a is not None])]
        "#,
            &[],
        );
    }
}
