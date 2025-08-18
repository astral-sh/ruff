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
    use ruff_db::diagnostic::Diagnostic;
    use ruff_python_parser::ParseOptions;
    use rustc_hash::FxHashMap;
    use test_case::test_case;

    use ruff_python_ast::PySourceType;
    use ruff_python_codegen::Stylist;
    use ruff_python_index::Indexer;
    use ruff_python_trivia::textwrap::dedent;

    use crate::linter::check_path;
    use crate::registry::{Linter, Rule};
    use crate::rules::isort;
    use crate::rules::pyflakes;
    use crate::settings::types::PreviewMode;
    use crate::settings::{LinterSettings, flags};
    use crate::source_kind::SourceKind;
    use crate::test::{test_contents, test_path, test_snippet};
    use crate::{Locator, assert_diagnostics, directives};

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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn f821_with_builtin_added_on_new_py_version_but_old_target_version_specified() {
        let diagnostics = test_snippet(
            "PythonFinalizationError",
            &LinterSettings {
                unresolved_target_version: ruff_python_ast::PythonVersion::PY312.into(),
                ..LinterSettings::for_rule(Rule::UndefinedName)
            },
        );
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn default_builtins() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/builtins.py"),
            &LinterSettings::for_rules(vec![Rule::UndefinedName]),
        )?;
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn default_typing_modules() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/typing_modules.py"),
            &LinterSettings::for_rules(vec![Rule::UndefinedName]),
        )?;
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/future_annotations.py"),
            &LinterSettings::for_rules(vec![Rule::UnusedImport, Rule::UndefinedName]),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn multi_statement_lines() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyflakes/multi_statement_lines.py"),
            &LinterSettings::for_rule(Rule::UnusedImport),
        )?;
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
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
        assert_diagnostics!(snapshot, diagnostics);
    }

    /// A re-implementation of the Pyflakes test runner.
    /// Note that all tests marked with `#[ignore]` should be considered TODOs.
    fn flakes(contents: &str) -> Vec<&str> {
        let contents = dedent(contents);
        let source_type = PySourceType::default();
        let source_kind = SourceKind::Python(contents.to_string());
        let settings = LinterSettings::for_rules(Linter::Pyflakes.rules());
        let target_version = settings.unresolved_target_version;
        let options =
            ParseOptions::from(source_type).with_target_version(target_version.parser_version());
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
            target_version,
        );
        messages.sort_by_key(|diagnostic| diagnostic.expect_range().start());
        messages
            .iter()
            .filter(|msg| !msg.is_invalid_syntax())
            .map(Diagnostic::name)
            .collect::<Vec<_>>()
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_undefined_names.py>
    #[test]
    fn undefined() {
        let actual = flakes("bar");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn defined_in_list_comp() {
        let actual = flakes("[a for a in range(10) if a]");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn undefined_in_list_comp() {
        let actual = flakes(
            r"
                [a for a in range(10)]
                a
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn undefined_exception_name() {
        // Exception names can't be used after the except: block.
        //
        // The exc variable is unused inside the exception handler.
        let actual = flakes(
            r"
                try:
        raise ValueError('ve')
                except ValueError as exc:
        pass
                exc
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn names_declared_in_except_blocks() {
        // Locals declared in except: blocks can be used after the block.
        //
        // This shows the example in test_undefinedExceptionName is
        // different.
        let actual = flakes(
            r"
                try:
        raise ValueError('ve')
                except ValueError as exc:
        e = exc
                e
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn undefined_exception_name_obscuring_local_variable2() {
        // Exception names are unbound after the `except:` block.
        //
        // Last line will raise UnboundLocalError.
        // The exc variable is unused inside the exception handler.
        let actual = flakes(
            r"
                try:
        raise ValueError('ve')
                except ValueError as exc:
        pass
                print(exc)
                exc = 'Original value'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_exception_in_except() {
        // The exception name can be deleted in the except: block.
        let actual = flakes(
            r"
                try:
        pass
                except Exception as exc:
        del exc
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn functions_need_global_scope() {
        let actual = flakes(
            r"
                class a:
        def b():
            fu
                fu = 1
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn builtins() {
        let actual = flakes("range(10)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn builtin_windows_error() {
        // C{WindowsError} is sometimes a builtin name, so no warning is emitted
        // for using it.
        let actual = flakes("WindowsError");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn module_annotations() {
        // Use of the C{__annotations__} in module scope should not emit
        // an undefined name warning when version is greater than or equal to 3.6.
        let actual = flakes("__annotations__");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn magic_globals_file() {
        // Use of the C{__file__} magic global should not emit an undefined name
        // warning.
        let actual = flakes("__file__");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn magic_globals_builtins() {
        // Use of the C{__builtins__} magic global should not emit an undefined
        // name warning.
        let actual = flakes("__builtins__");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn magic_globals_name() {
        // Use of the C{__name__} magic global should not emit an undefined name
        // warning.
        let actual = flakes("__name__");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn magic_module_in_class_scope() {
        // Use of the C{__module__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        let actual = flakes("__module__");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                class Foo:
        __module__
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class Foo:
        def bar(self):
            __module__
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn magic_qualname_in_class_scope() {
        // Use of the C{__qualname__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        let actual = flakes("__qualname__");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                class Foo:
        __qualname__
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class Foo:
        def bar(self):
            __qualname__
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        __qualname__ = 1

        class Foo:
            __qualname__
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn global_import_star() {
        // Can't find undefined names with import *.
        let actual = flakes("from fu import *; bar");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-local-with-import-star",
            "undefined-local-with-import-star-usage",
        ]
        "###);
    }

    #[test]
    fn defined_by_global() {
        // "global" can make an otherwise undefined name in another function
        // defined.
        let actual = flakes(
            r"
                def a(): global fu; fu = 1
                def b(): fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def c(): bar
                def b(): global bar; bar = 1
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        // TODO(charlie): Extract globals recursively (such that we don't raise F821).
        let actual = flakes(
            r"
                def c(): bar
                def d():
        def b():
            global bar; bar = 1
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_by_global_multiple_names() {
        // "global" can accept multiple names.
        let actual = flakes(
            r"
                def a(): global fu, bar; fu = 1; bar = 2
                def b(): fu; bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn global_in_global_scope() {
        // A global statement in the global scope is ignored.
        let actual = flakes(
            r"
                global x
                def foo():
        print(x)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn global_reset_name_only() {
        // A global statement does not prevent other names being undefined.
        let actual = flakes(
            r"
                def f1():
        s

                def f2():
        global m
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del() {
        // Del deletes bindings.
        let actual = flakes("a = 1; del a; a");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn del_global() {
        // Del a global binding from a function.
        let actual = flakes(
            r"
                a = 1
                def f():
        global a
        del a
                a
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_undefined() {
        // Del an undefined name.
        let actual = flakes("del a");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn del_conditional() {
        // Ignores conditional bindings deletion.
        let actual = flakes(
            r"
                context = None
                test = True
                if False:
        del(test)
                assert(test)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_conditional_nested() {
        // Ignored conditional bindings deletion even if they are nested in other
        // blocks.
        let actual = flakes(
            r"
                context = None
                test = True
                if False:
        with context():
            del(test)
                assert(test)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_while() {
        // Ignore bindings deletion if called inside the body of a while
        // statement.
        let actual = flakes(
            r"
                def test():
        foo = 'bar'
        while False:
            del foo
        assert(foo)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_while_test_usage() {
        // Ignore bindings deletion if called inside the body of a while
        // statement and name is used inside while's test part.
        let actual = flakes(
            r"
                def _worker():
        o = True
        while o is not True:
            del o
            o = False
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn del_while_nested() {
        // Ignore bindings deletions if node is part of while's test, even when
        // del is in a nested block.
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn global_from_nested_scope() {
        // Global names are available from nested scopes.
        let actual = flakes(
            r"
                a = 1
                def b():
        def c():
            a
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn later_redefined_global_from_nested_scope() {
        // Test that referencing a local name that shadows a global, before it is
        // defined, generates a warning.
        let actual = flakes(
            r"
                a = 1
                def fun():
        a
        a = 2
        return a
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn later_redefined_global_from_nested_scope2() {
        // Test that referencing a local name in a nested scope that shadows a
        // global declared in an enclosing scope, before it is defined, generates
        // a warning.
        let actual = flakes(
            r"
        a = 1
        def fun():
            global a
            def fun2():
                a
                a = 2
                return a
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-local",
        ]
        "###);
    }

    #[test]
    fn intermediate_class_scope_ignored() {
        // If a name defined in an enclosing scope is shadowed by a local variable
        // and the name is used locally before it is bound, an unbound local
        // warning is emitted, even if there is a class scope between the enclosing
        // scope and the local scope.
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn later_redefined_global_from_nested_scope3() {
        // Test that referencing a local name in a nested scope that shadows a
        // global, before it is defined, generates a warning.
        let actual = flakes(
            r"
        def fun():
            a = 1
            def fun2():
                a
                a = 1
                return a
            return a
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-local",
        ]
        "###);
    }

    #[test]
    fn undefined_augmented_assignment() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
            "undefined-name",
            "undefined-name",
            "unused-variable",
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn nested_class() {
        // Nested classes can access enclosing scope.
        let actual = flakes(
            r"
                def f(foo):
        class C:
            bar = foo
            def f(self):
                return foo
        return C()

                f(123).f()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn bad_nested_class() {
        // Free variables in nested classes must bind at class creation.
        let actual = flakes(
            r"
                def f():
        class C:
            bar = foo
        foo = 456
        return foo
                f()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_as_star_args() {
        // Star and double-star arg names are defined.
        let actual = flakes(
            r"
                def f(a, *b, **c):
        print(a, b, c)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_as_star_unpack() {
        // Star names in unpack are defined.
        let actual = flakes(
            r"
                a, *b = range(10)
                print(a, b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                *a, b = range(10)
                print(a, b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                a, *b, c = range(10)
                print(a, b, c)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_as_star_unpack() {
        // In stable, starred names in unpack are used if RHS is not a tuple/list literal.
        // In preview, these should be marked as unused.
        let actual = flakes(
            r"
                def f():
        a, *b = range(10)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        (*a, b) = range(10)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        [a, *b, c] = range(10)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn keyword_only_args() {
        // Keyword-only arg names are defined.
        let actual = flakes(
            r"
                def f(*, a, b=None):
        print(a, b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                import default_b
                def f(*, a, b=default_b):
        print(a, b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn keyword_only_args_undefined() {
        // Typo in kwonly name.
        let actual = flakes(
            r"
                def f(*, a, b=default_c):
        print(a, b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn annotation_undefined() {
        // Undefined annotations.
        let actual = flakes(
            r"
                from abc import note1, note2, note3, note4, note5
                def func(a: note1, *args: note2,
             b: note3=12, **kw: note4) -> note5: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                def func():
        d = e = 42
        def func(a: {1, d}) -> (lambda c: e): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn meta_class_undefined() {
        let actual = flakes(
            r"
                from abc import ABCMeta
                class A(metaclass=ABCMeta): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_in_gen_exp() {
        // Using the loop variable of a generator expression results in no
        // warnings.
        let actual = flakes("(a for a in [1, 2, 3] if a)");
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes("(b for b in (a for a in [1, 2, 3] if a) if b)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn undefined_in_gen_exp_nested() {
        // The loop variables of generator expressions nested together are
        // not defined in the other generator.
        let actual = flakes("(b for b in (a for a in [1, 2, 3] if b) if b)");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);

        let actual = flakes("(b for b in (a for a in [1, 2, 3] if a) if a)");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn undefined_with_error_handler() {
        // Some compatibility code checks explicitly for NameError.
        // It should not trigger warnings.
        let actual = flakes(
            r"
                try:
        socket_map
                except NameError:
        socket_map = {}
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r#"
                try:
        _memoryview.contiguous
                except (NameError, AttributeError):
        raise RuntimeError("Python >= 3.3 is required")
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        // If NameError is not explicitly handled, generate a warning.
        let actual = flakes(
            r"
                try:
        socket_map
                except:
        socket_map = {}
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                try:
        socket_map
                except Exception:
        socket_map = {}
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_in_class() {
        // Defined name for generator expressions and dict/set comprehension.
        let actual = flakes(
            r"
                class A:
        T = range(10)

        Z = (x for x in T)
        L = [x for x in T]
        B = dict((i, str(i)) for i in T)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                class A:
        T = range(10)

        X = {x for x in T}
        Y = {x:x for x in T}
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                class A:
        T = 1

        # In each case, `T` is undefined. Only the first `iter` uses the class scope.
        X = (T for x in range(10))
        Y = [x for x in range(10) if T]
        Z = [x for x in range(10) for y in T]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_in_class_nested() {
        // Defined name for nested generator expressions in a class.
        let actual = flakes(
            r"
                class A:
        T = range(10)

        Z = (x for x in (a for a in T))
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn undefined_in_loop() {
        // The loop variable is defined after the expression is computed.
        let actual = flakes(
            r"
                for i in range(i):
        print(i)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                [42 for i in range(i)]
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                (42 for i in range(i))
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn defined_from_lambda_in_dictionary_comprehension() {
        // Defined name referenced from a lambda function within a dict/set
        // comprehension.
        let actual = flakes(
            r"
                {lambda: id(x) for x in range(10)}
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn defined_from_lambda_in_generator() {
        // Defined name referenced from a lambda function within a generator
        // expression.
        let actual = flakes(
            r"
                any(lambda: id(x) for x in range(10))
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn undefined_from_lambda_in_dictionary_comprehension() {
        // Undefined name referenced from a lambda function within a dict/set
        // comprehension.
        let actual = flakes(
            r"
                {lambda: id(y) for x in range(10)}
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn undefined_from_lambda_in_comprehension() {
        // Undefined name referenced from a lambda function within a generator
        // expression.
        let actual = flakes(
            r"
                any(lambda: id(y) for x in range(10))
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn dunder_class() {
        let actual = flakes(
            r"
                class Test(object):
        def __init__(self):
            print(__class__.__name__)
            self.x = 1

                t = Test()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class Test(object):
        print(__class__.__name__)

        def __init__(self):
            self.x = 1

                t = Test()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class Test(object):
        X = [__class__ for _ in range(10)]

        def __init__(self):
            self.x = 1

                t = Test()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f(self):
        print(__class__.__name__)
        self.x = 1

                f()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_imports.py>
    #[test]
    fn unused_import() {
        let actual = flakes("import fu, bar");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "unused-import",
        ]
        "###);
        let actual = flakes("from baz import fu, bar");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn unused_import_relative() {
        let actual = flakes("from . import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from . import fu as baz");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from .. import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from ... import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from .. import fu as baz");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from .bar import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from ..bar import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from ...bar import fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes("from ...bar import fu as baz");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn aliased_import() {
        let actual = flakes("import fu as FU, bar as FU");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
            "unused-import",
        ]
        "###);
        let actual = flakes("from moo import fu as FU, bar as FU");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn aliased_import_shadow_module() {
        // Imported aliases can shadow the source of the import.
        let actual = flakes("from moo import fu as moo; moo");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu as fu; fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu.bar as fu; fu");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_import() {
        let actual = flakes("import fu; print(fu)");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("from baz import fu; print(fu)");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; del fu");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_import_relative() {
        let actual = flakes("from . import fu; assert fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("from .bar import fu; assert fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("from .. import fu; assert fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("from ..bar import fu as baz; assert baz");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_while_unused() {
        let actual = flakes("import fu; fu = 3");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
        let actual = flakes("import fu; fu, bar = 3");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
        let actual = flakes("import fu; [fu, bar] = 3");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn redefined_if() {
        // Test that importing a module twice within an if
        // block does raise a warning.
        let actual = flakes(
            r"
                i = 2
                if i==1:
        import os
        import os
                os.path
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_if_else() {
        // Test that importing a module twice in if
        // and else blocks does not raise a warning.
        let actual = flakes(
            r"
                i = 2
                if i==1:
        import os
                else:
        import os
                os.path
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try() {
        // Test that importing a module twice in a try block
        // does raise a warning.
        let actual = flakes(
            r"
                try:
        import os
        import os
                except:
        pass
                os.path
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_except() {
        // Test that importing a module twice in a try
        // and except block does not raise a warning.
        let actual = flakes(
            r"
                try:
        import os
                except:
        import os
                os.path
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_nested() {
        // Test that importing a module twice using a nested
        // try/except and if blocks does not issue a warning.
        let actual = flakes(
            r"
                try:
        if True:
            if True:
                import os
                except:
        import os
                os.path
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_except_multi() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_else() {
        let actual = flakes(
            r"
                try:
        from aa import mixer
                except ImportError:
        pass
                else:
        from bb import mixer
                mixer(123)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_except_else() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_except_finally() {
        let actual = flakes(
            r"
                try:
        from aa import a
                except ImportError:
        from bb import a
                finally:
        a = 42
                print(a)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_try_except_else_finally() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_function() {
        let actual = flakes(
            r"
                import fu
                def fu():
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_in_nested_function() {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        let actual = flakes(
            r"
                import fu
                def bar():
        def baz():
            def fu():
                pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_in_nested_function_twice() {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        let actual = flakes(
            r"
                import fu
                def bar():
        import fu
        def baz():
            def fu():
                pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_but_used_later() {
        // Test that a global import which is redefined locally,
        // but used later in another scope does not generate a warning.
        let actual = flakes(
            r"
                import unittest, transport

                class GetTransportTestCase(unittest.TestCase):
        def test_get_transport(self):
            transport = 'transport'
            self.assertIsNotNone(transport)

                class TestTransportMethodArgs(unittest.TestCase):
        def test_send_defaults(self):
            transport.Transport()",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_class() {
        let actual = flakes(
            r"
                import fu
                class fu:
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_subclass() {
        // If an imported name is redefined by a class statement which also uses
        // that name in the bases list, no warning is emitted.
        let actual = flakes(
            r"
                from fu import bar
                class bar(bar):
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_in_class() {
        // Test that shadowing a global with a class attribute does not produce a
        // warning.
        let actual = flakes(
            r"
                import fu
                class bar:
        fu = 1
                print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn import_in_class() {
        // Test that import within class is a locally scoped attribute.
        let actual = flakes(
            r"
                class bar:
        import fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                class bar:
        import fu

                fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_function() {
        let actual = flakes(
            r"
                import fu
                def fun():
        print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn shadowed_by_parameter() {
        let actual = flakes(
            r"
                import fu
                def fun(fu):
        print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import fu
                def fun(fu):
        print(fu)
                print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn new_assignment() {
        let actual = flakes("fu = None");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_getattr() {
        let actual = flakes("import fu; fu.bar.baz");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; \"bar\".fu.baz");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn used_in_slice() {
        let actual = flakes("import fu; print(fu.bar[1:])");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_if_body() {
        let actual = flakes(
            r"
                import fu
                if True: print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_if_conditional() {
        let actual = flakes(
            r"
                import fu
                if fu: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_elif_conditional() {
        let actual = flakes(
            r"
                import fu
                if False: pass
                elif fu: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_else() {
        let actual = flakes(
            r"
                import fu
                if False: pass
                else: print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_call() {
        let actual = flakes("import fu; fu.bar()");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_class() {
        let actual = flakes(
            r"
                import fu
                class bar:
        bar = fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_class_base() {
        let actual = flakes(
            r"
                import fu
                class bar(object, fu.baz):
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn not_used_in_nested_scope() {
        let actual = flakes(
            r"
                import fu
                def bleh():
        pass
                print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_for() {
        let actual = flakes(
            r"
                import fu
                for bar in range(9):
        print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_for_else() {
        let actual = flakes(
            r"
                import fu
                for bar in range(10):
        pass
                else:
        print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_for() {
        let actual = flakes(
            r"
                import fu
                for fu in range(2):
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn shadowed_by_for() {
        // Test that shadowing a global name with a for loop variable generates a
        // warning.
        let actual = flakes(
            r"
                import fu
                fu.bar()
                for fu in ():
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn shadowed_by_for_deep() {
        // Test that shadowing a global name with a for loop variable nested in a
        // tuple unpack generates a warning.
        let actual = flakes(
            r"
                import fu
                fu.bar()
                for (x, y, z, (a, b, c, (fu,))) in ():
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                import fu
                fu.bar()
                for [x, y, z, (a, b, c, (fu,))] in ():
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_return() {
        let actual = flakes(
            r"
                import fu
                def fun():
        return fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_operators() {
        let actual = flakes("import fu; 3 + fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 % fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 - fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 * fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 ** fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 / fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 3 // fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; -fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; ~fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 == fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 | fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 & fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 ^ fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 >> fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; 1 << fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_assert() {
        let actual = flakes("import fu; assert fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_subscript() {
        let actual = flakes("import fu; fu.bar[1]");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_logic() {
        let actual = flakes("import fu; fu and False");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; fu or False");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; not fu.bar");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_list() {
        let actual = flakes("import fu; [fu]");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_tuple() {
        let actual = flakes("import fu; (fu,)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_try() {
        let actual = flakes(
            r"
                import fu
                try: fu
                except: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_except() {
        let actual = flakes(
            r"
                import fu
                try: fu
                except: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_except() {
        let actual = flakes(
            r"
                import fu
                try: pass
                except Exception as fu: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-variable",
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn used_in_raise() {
        let actual = flakes(
            r"
                import fu
                raise fu.bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_yield() {
        let actual = flakes(
            r"
                import fu
                def gen():
        yield fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_dict() {
        let actual = flakes("import fu; {fu:None}");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; {1:fu}");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_parameter_default() {
        let actual = flakes(
            r"
                import fu
                def f(bar=fu):
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_attribute_assign() {
        let actual = flakes("import fu; fu.bar = 1");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_keyword_arg() {
        let actual = flakes("import fu; fu.bar(stuff=fu)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_assignment() {
        let actual = flakes("import fu; bar=fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; n=0; n+=fu");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_list_comp() {
        let actual = flakes("import fu; [fu for _ in range(1)]");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; [1 for _ in range(1) if fu]");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_try_finally() {
        let actual = flakes(
            r"
                import fu
                try: pass
                finally: fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import fu
                try: fu
                finally: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_while() {
        let actual = flakes(
            r"
                import fu
                while 0:
        fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import fu
                while fu: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_global() {
        // A 'global' statement shadowing an unused import should not prevent it
        // from being reported.
        let actual = flakes(
            r"
                import fu
                def f(): global fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn used_and_global() {
        // A 'global' statement shadowing a used import should not cause it to be
        // reported as unused.
        let actual = flakes(
            r"
        import foo
        def f(): global foo
        def g(): foo.is_used()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn assigned_to_global() {
        // Binding an import to a declared global should not cause it to be
        // reported as unused.
        let actual = flakes(
            r"
        def f(): global foo; import foo
        def g(): foo.is_used()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_exec() {
        let actual = flakes("import fu; exec('print(1)', fu.bar)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_lambda() {
        let actual = flakes(
            r"import fu;
        lambda: fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn shadowed_by_lambda() {
        let actual = flakes("import fu; lambda fu: fu");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "redefined-while-unused",
        ]
        "###);
        let actual = flakes("import fu; lambda fu: fu\nfu()");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_in_slice_obj() {
        let actual = flakes(
            r#"import fu;
        "meow"[::fu]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unused_in_nested_scope() {
        let actual = flakes(
            r"
                def bar():
        import fu
                fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn methods_dont_use_class_scope() {
        let actual = flakes(
            r"
                class bar:
        import fu
        def fun(self):
            fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn nested_functions_nest_scope() {
        let actual = flakes(
            r"
                def a():
        def b():
            fu
        import fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn nested_class_and_function_scope() {
        let actual = flakes(
            r"
                def a():
        import fu
        class b:
            def c(self):
                print(fu)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn package_import() {
        // If a dotted name is imported and used, no warning is reported.
        let actual = flakes(
            r"
                import fu.bar
                fu.bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unused_package_import() {
        // If a dotted name is imported and not used, an unused import warning is
        // reported.
        let actual = flakes("import fu.bar");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn duplicate_submodule_import() {
        // If a submodule of a package is imported twice, an unused import warning and a
        // redefined while unused warning are reported.
        let actual = flakes(
            r"
                import fu.bar, fu.bar
                fu.bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
        let actual = flakes(
            r"
                import fu.bar
                import fu.bar
                fu.bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn different_submodule_import() {
        // If two different submodules of a package are imported, no duplicate import
        // warning is reported for the package.
        let actual = flakes(
            r"
                import fu.bar, fu.baz
                fu.bar, fu.baz
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                import fu.bar
                import fu.baz
                fu.bar, fu.baz
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn aliased_submodule_import() {
        let actual = flakes(
            r"
                import fu.bar as baz
                import fu.bar as baz
                baz
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);

        let actual = flakes(
            r"
                import fu.bar as baz
                import baz
                baz
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);

        let actual = flakes(
            r"
                import fu.bar as baz
                import fu.bar as bop
                baz, bop
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import foo.baz
                import foo.baz as foo
                foo
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn used_package_with_submodule_import() {
        // Usage of package marks submodule imports as used.
        let actual = flakes(
            r"
                import fu
                import fu.bar
                fu.x
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import fu.bar
                import fu
                fu.x
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_package_with_submodule_import_of_alias() {
        // Usage of package by alias marks submodule imports as used.
        let actual = flakes(
            r"
                import foo as f
                import foo.bar
                f.bar.do_something()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                import foo as f
                import foo.bar.blah
                f.bar.blah.do_something()
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unused_package_with_submodule_import() {
        // When a package and its submodule are imported, only report once.
        let actual = flakes(
            r"
                import fu
                import fu.bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn assign_rhs_first() {
        let actual = flakes("import fu; fu = fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; fu, bar = fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; [fu, bar] = fu");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; fu += fu");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn trying_multiple_imports() {
        let actual = flakes(
            r"
                try:
        import fu
                except ImportError:
        import bar as fu
                fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn non_global_does_not_redefine() {
        let actual = flakes(
            r"
                import fu
                def a():
        fu = 3
        return fu
                fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn functions_run_later() {
        let actual = flakes(
            r"
                def a():
        fu
                import fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn function_names_are_bound_now() {
        let actual = flakes(
            r"
                import fu
                def fu():
        fu
                fu
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn ignore_non_import_redefinitions() {
        let actual = flakes("a = 1; a = 2");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn imported_in_class() {
        // Imports in class scope can be used through self.
        let actual = flakes(
            r"
                class C:
        import i
        def __init__(self):
            self.i
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn import_used_in_method_definition() {
        // Method named 'foo' with default args referring to module named 'foo'.
        let actual = flakes(
            r"
                import foo

                class Thing(object):
        def foo(self, parser=foo.parse_foo):
            pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn future_import() {
        // __future__ is special.
        let actual = flakes("from __future__ import division");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r#"
                "docstring is allowed before future import"
                from __future__ import division
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn future_import_first() {
        // __future__ imports must come before anything else.
        let actual = flakes(
            r"
                x = 5
                from __future__ import division
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "late-future-import",
        ]
        "###);
        let actual = flakes(
            r"
                from foo import bar
                from __future__ import division
                bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "late-future-import",
        ]
        "###);
    }

    #[test]
    fn future_import_used() {
        // __future__ is special, but names are injected in the namespace.
        let actual = flakes(
            r"
                from __future__ import division
                from __future__ import print_function

                assert print_function is not division
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn future_import_undefined() {
        // Importing undefined names from __future__ fails.
        let actual = flakes(
            r"
                from __future__ import print_statement
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "future-feature-not-defined",
        ]
        "###);
    }

    #[test]
    fn future_import_star() {
        // Importing '*' from __future__ fails.
        let actual = flakes(
            r"
                from __future__ import *
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "future-feature-not-defined",
        ]
        "###);
    }

    #[test]
    fn ignored_in_function() {
        // An C{__all__} definition does not suppress unused import warnings in a
        // function scope.
        let actual = flakes(
            r#"
                def foo():
        import bar
        __all__ = ["bar"]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn ignored_in_class() {
        // An C{__all__} definition in a class does not suppress unused import warnings.
        let actual = flakes(
            r#"
                import bar
                class foo:
        __all__ = ["bar"]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn ignored_when_not_directly_assigned() {
        let actual = flakes(
            r#"
                import bar
                (__all__,) = ("foo",)
                "#,
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn warning_suppressed() {
        // If a name is imported and unused but is named in C{__all__}, no warning
        // is reported.
        let actual = flakes(
            r#"
                import foo
                __all__ = ["foo"]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r#"
                import foo
                __all__ = ("foo",)
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn augmented_assignment() {
        // The C{__all__} variable is defined incrementally.
        let actual = flakes(
            r"
                import a
                import c
                __all__ = ['a']
                __all__ += ['b']
                if 1 < 3:
        __all__ += ['c', 'd']
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn list_concatenation_assignment() {
        // The C{__all__} variable is defined through list concatenation.
        let actual = flakes(
            r"
                import sys
                __all__ = ['a'] + ['b'] + ['c']
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "undefined-export",
            "undefined-export",
            "undefined-export",
        ]
        "###);
    }

    #[test]
    fn tuple_concatenation_assignment() {
        // The C{__all__} variable is defined through tuple concatenation.
        let actual = flakes(
            r"
                import sys
                __all__ = ('a',) + ('b',) + ('c',)
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "undefined-export",
            "undefined-export",
            "undefined-export",
        ]
        "###);
    }

    #[test]
    fn all_with_attributes() {
        let actual = flakes(
            r"
                from foo import bar
                __all__ = [bar.__name__]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn all_with_names() {
        let actual = flakes(
            r"
                from foo import bar
                __all__ = [bar]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn all_with_attributes_added() {
        let actual = flakes(
            r"
                from foo import bar
                from bar import baz
                __all__ = [bar.__name__] + [baz.__name__]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn all_mixed_attributes_and_strings() {
        let actual = flakes(
            r"
                from foo import bar
                from foo import baz
                __all__ = ['bar', baz.__name__]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unbound_exported() {
        // If C{__all__} includes a name which is not bound, a warning is emitted.
        let actual = flakes(
            r#"
                __all__ = ["foo"]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-export",
        ]
        "###);
    }

    #[test]
    fn import_star_exported() {
        // Report undefined if import * is used
        let actual = flakes(
            r"
                from math import *
                __all__ = ['sin', 'cos']
                csc(1)
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-local-with-import-star",
            "undefined-local-with-import-star-usage",
            "undefined-local-with-import-star-usage",
            "undefined-local-with-import-star-usage",
        ]
        "###);
    }

    #[ignore]
    #[test]
    fn import_star_not_exported() {
        // Report unused import when not needed to satisfy __all__.
        let actual = flakes(
            r"
                from foolib import *
                a = 1
                __all__ = ['a']
                ",
        );
        insta::assert_debug_snapshot!(actual,@r#""#);
    }

    #[test]
    fn used_in_gen_exp() {
        // Using a global in a generator expression results in no warnings.
        let actual = flakes("import fu; (fu for _ in range(1))");
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes("import fu; (1 for _ in range(1) if fu)");
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn redefined_by_gen_exp() {
        // Reusing a global name as the loop variable for a generator
        // expression results in a redefinition warning.
        let actual = flakes("import fu; (1 for fu in range(1))");
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn used_as_decorator() {
        // Using a global name in a decorator statement results in no warnings,
        // but using an undefined name in a decorator statement results in an
        // undefined name warning.
        let actual = flakes(
            r#"
                from interior import decorate
                @decorate
                def f():
        return "hello"
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r#"
                @decorate
                def f():
        return "hello"
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn used_as_class_decorator() {
        // Using an imported name as a class decorator results in no warnings,
        // but using an undefined name as a class decorator results in an
        // undefined name warning.
        let actual = flakes(
            r"
                from interior import decorate
                @decorate
                class foo:
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r#"
                from interior import decorate
                @decorate("foo")
                class bar:
        pass
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                @decorate
                class foo:
        pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_type_annotations.py>
    #[test]
    fn typing_overload() {
        // Allow intentional redefinitions via @typing.overload.
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn typing_extensions_overload() {
        // Allow intentional redefinitions via @typing_extensions.overload.
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn typing_overload_async() {
        // Allow intentional redefinitions via @typing.overload (async).
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn overload_with_multiple_decorators() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn overload_in_class() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn aliased_typing_import() {
        // Detect when typing is imported as another name.
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn not_a_typing_overload() {
        // regression test for @typing.overload detection bug in 2.1.0.
        let actual = flakes(
            r"
        def foo(x):
            return x

        @foo
        def bar():
            pass

        def bar():
            pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "redefined-while-unused",
        ]
        "###);
    }

    #[test]
    fn variable_annotations() {
        let actual = flakes(
            r"
                name: str
                age: int
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                name: str = 'Bob'
                age: int = 18
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class C:
        name: str
        age: int
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class C:
        name: str = 'Bob'
        age: int = 18
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        name: str
        age: int
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        name: str = 'Bob'
        age: int = 18
        foo: not_a_real_type = None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        name: str
        print(name)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing import Any
                def f():
        a: Any
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                foo: not_a_real_type
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                foo: not_a_real_type = None
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                class C:
        foo: not_a_real_type
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                class C:
        foo: not_a_real_type = None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        class C:
            foo: not_a_real_type
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        class C:
            foo: not_a_real_type = None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                bar: Bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                bar: 'Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                import foo
                bar: foo.Bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                import foo
                bar: 'foo.Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                def f(bar: Bar): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                def f(bar: 'Bar'): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                def f(bar) -> Bar: return bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from foo import Bar
                def f(bar) -> 'Bar': return bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                bar: 'Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                bar: 'foo.Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                from foo import Bar
                bar: str
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes(
            r"
                from foo import Bar
                def f(bar: str): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
        let actual = flakes(
            r"
                def f(a: A) -> A: pass
                class A: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                def f(a: 'A') -> 'A': return a
                class A: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                a: A
                class A: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                a: 'A'
                class A: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                T: object
                def f(t: T): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                T: object
                def g(t: 'T'): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r"
                T = object
                def f(t: T): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                T = object
                def g(t: 'T'): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                a: 'A B'
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "forward-annotation-syntax-error",
        ]
        "###);
        let actual = flakes(
            r"
                a: 'A; B'
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "forward-annotation-syntax-error",
        ]
        "###);
        let actual = flakes(
            r"
                a: '1 + 2'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r#"
                a: 'a: "A"'
                "#,
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "forward-annotation-syntax-error",
        ]
        "###);
    }

    #[test]
    fn variable_annotation_references_self_name_undefined() {
        let actual = flakes(
            r"
                x: int = x
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn use_pep695_type_aliass() {
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias
                from foo import Bar

                bar: TypeAlias = Bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias
                from foo import Bar

                bar: TypeAlias = 'Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias
                from foo import Bar

                class A:
        bar: TypeAlias = Bar
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias
                from foo import Bar

                class A:
        bar: TypeAlias = 'Bar'
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias

                bar: TypeAlias
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                from typing_extensions import TypeAlias
                from foo import Bar

                bar: TypeAlias
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "unused-import",
        ]
        "###);
    }

    #[test]
    fn annotating_an_import() {
        let actual = flakes(
            r"
        from a import b, c
        b: c
        print(b)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unused_annotation() {
        // Unused annotations are fine in module and class scope.
        let actual = flakes(
            r"
                x: int
                class Cls:
        y: int
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r"
                def f():
        x: int
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        // This should only print one UnusedVariable message.
        let actual = flakes(
            r"
                def f():
        x: int
        x = 3
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn unassigned_annotation_is_undefined() {
        let actual = flakes(
            r"
                name: str
                print(name)
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
        ]
        "###);
    }

    #[test]
    fn annotated_async_def() {
        let actual = flakes(
            r"
                class c: pass
                async def func(c: c) -> None: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn postponed_annotations() {
        let actual = flakes(
            r"
                from __future__ import annotations
                def f(a: A) -> A: pass
                class A:
        b: B
                class B: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                from __future__ import annotations
                def f(a: A) -> A: pass
                class A:
        b: Undefined
                class B: pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");

        let actual = flakes(
            r"
                from __future__ import annotations
                T: object
                def f(t: T): pass
                def g(t: 'T'): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
            "undefined-name",
        ]
        "###);

        let actual = flakes(
            r"
                from __future__ import annotations
                T = object
                def f(t: T): pass
                def g(t: 'T'): pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn type_annotation_clobbers_all() {
        let actual = flakes(
            r#"
                from typing import TYPE_CHECKING, List

                from y import z

                if not TYPE_CHECKING:
        __all__ = ("z",)
                else:
        __all__: List[str]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn return_annotation_is_class_scope_variable() {
        let actual = flakes(
            r"
                from typing import TypeVar
                class Test:
        Y = TypeVar('Y')

        def t(self, x: Y) -> Y:
            return x
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn return_annotation_is_function_body_variable() {
        let actual = flakes(
            r"
                class Test:
        def t(self) -> Y:
            Y = 2
            return Y
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn positional_only_argument_annotations() {
        let actual = flakes(
            r"
                from x import C

                def f(c: C, /): ...
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn partially_quoted_type_annotation() {
        let actual = flakes(
            r"
                from queue import Queue
                from typing import Optional

                def f() -> Optional['Queue[str]']:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn partially_quoted_type_assignment() {
        let actual = flakes(
            r"
                from queue import Queue
                from typing import Optional

                MaybeQueue = Optional['Queue[str]']
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn nested_partially_quoted_type_assignment() {
        let actual = flakes(
            r"
                from queue import Queue
                from typing import Callable

                Func = Callable[['Queue[str]'], None]
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn quoted_type_cast() {
        let actual = flakes(
            r"
                from typing import cast, Optional

                maybe_int = cast('Optional[int]', 42)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn type_cast_literal_str_to_str() {
        // Checks that our handling of quoted type annotations in the first
        // argument to `cast` doesn't cause issues when (only) the _second_
        // argument is a literal str which looks a bit like a type annotation.
        let actual = flakes(
            r"
                from typing import cast

                a_string = cast(str, 'Optional[int]')
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn quoted_type_cast_renamed_import() {
        let actual = flakes(
            r"
                from typing import cast as tsac, Optional as Maybe

                maybe_int = tsac('Maybe[int]', 42)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn quoted_type_var_constraints() {
        let actual = flakes(
            r"
                from typing import TypeVar, Optional

                T = TypeVar('T', 'str', 'Optional[int]', bytes)
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn quoted_type_var_bound() {
        let actual = flakes(
            r"
                from typing import TypeVar, Optional, List

                T = TypeVar('T', bound='Optional[int]')
                S = TypeVar('S', int, bound='List[int]')
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn literal_type_typing() {
        let actual = flakes(
            r"
                from typing import Literal

                def f(x: Literal['some string']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn literal_type_typing_extensions() {
        let actual = flakes(
            r"
                from typing_extensions import Literal

                def f(x: Literal['some string']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn annotated_type_typing_missing_forward_type_multiple_args() {
        let actual = flakes(
            r"
                from typing import Annotated

                def f(x: Annotated['integer', 1]) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn annotated_type_typing_with_string_args() {
        let actual = flakes(
            r"
                from typing import Annotated

                def f(x: Annotated[int, '> 0']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn annotated_type_typing_with_string_args_in_union() {
        let actual = flakes(
            r"
                from typing import Annotated, Union

                def f(x: Union[Annotated['int', '>0'], 'integer']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    // We err on the side of assuming strings are forward references.
    #[ignore]
    #[test]
    fn literal_type_some_other_module() {
        // err on the side of false-negatives for types named Literal.
        let actual = flakes(
            r"
                from my_module import compat
                from my_module.compat import Literal

                def f(x: compat.Literal['some string']) -> None:
        return None
                def g(x: Literal['some string']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@r#""#);
    }

    #[test]
    fn literal_union_type_typing() {
        let actual = flakes(
            r"
                from typing import Literal

                def f(x: Literal['some string', 'foo bar']) -> None:
        return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    // TODO(charlie): Support nested deferred string annotations.
    #[ignore]
    #[test]
    fn deferred_twice_annotation() {
        let actual = flakes(
            r#"
        from queue import Queue
        from typing import Optional

        def f() -> "Optional['Queue[str]']":
            return None
                "#,
        );
        insta::assert_debug_snapshot!(actual,@r#""#);
    }

    #[test]
    fn partial_string_annotations_with_future_annotations() {
        let actual = flakes(
            r"
        from __future__ import annotations

        from queue import Queue
        from typing import Optional

        def f() -> Optional['Queue[str]']:
            return None
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn forward_annotations_for_classes_in_scope() {
        let actual = flakes(
            r#"
                from typing import Optional

                def f():
        class C:
            a: "D"
            b: Optional["D"]
            c: "Optional[D]"

        class D: pass
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn idiomiatic_typing_guards() {
        // typing.TYPE_CHECKING: python3.5.3+.
        let actual = flakes(
            r"
        from typing import TYPE_CHECKING

        if TYPE_CHECKING:
            from t import T

        def f() -> T:
            pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        // False: the old, more-compatible approach.
        let actual = flakes(
            r"
        if False:
            from t import T

        def f() -> T:
            pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        // Some choose to assign a constant and do it that way.
        let actual = flakes(
            r"
        MYPY = False

        if MYPY:
            from t import T

        def f() -> T:
            pass
                ",
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn typing_guard_for_protocol() {
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn typed_names_correct_forward_ref() {
        let actual = flakes(
            r#"
        from typing import TypedDict, List, NamedTuple

        List[TypedDict("x", {})]
        List[TypedDict("x", x=int)]
        List[NamedTuple("a", a=int)]
        List[NamedTuple("a", [("a", int)])]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
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
        );
        insta::assert_debug_snapshot!(actual,@r###"
        [
            "undefined-name",
            "undefined-name",
            "undefined-name",
            "undefined-name",
            "undefined-name",
            "undefined-name",
            "undefined-name",
        ]
        "###);
        let actual = flakes(
            r#"
        from typing import NamedTuple, TypeVar, cast
        from t import A, B, C, D, E

        NamedTuple("A", [("a", A["C"])])
        TypeVar("A", bound=A["B"])
        TypeVar("A", A["D"])
        cast(A["E"], [])
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
        let actual = flakes(
            r#"
        from typing import NewType

        def f():
            name = "x"
            NewType(name, int)
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn named_types_classes() {
        let actual = flakes(
            r#"
        from typing import TypedDict, NamedTuple
        class X(TypedDict):
            y: TypedDict("z", {"zz":int})

        class Y(NamedTuple):
            y: NamedTuple("v", [("vv", int)])
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }

    #[test]
    fn gh_issue_17196_regression_test() {
        let actual = flakes(
            r#"
        from typing import Annotated

        def type_annotations_from_tuple():
            annos = (str, "foo", "bar")
            return Annotated[annos]

        def type_annotations_from_filtered_tuple():
            annos = (str, None, "foo", None, "bar")
            return Annotated[tuple([a for a in annos if a is not None])]
                "#,
        );
        insta::assert_debug_snapshot!(actual,@"[]");
    }
}
