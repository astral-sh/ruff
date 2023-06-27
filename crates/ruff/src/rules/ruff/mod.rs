//! Ruff-specific rules.

pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use ruff_python_ast::source_code::SourceFileBuilder;

    use crate::pyproject_toml::lint_pyproject_toml;
    use crate::registry::Rule;
    use crate::settings::resolve_per_file_ignores;
    use crate::settings::types::{PerFileIgnore, PythonVersion};
    use crate::test::{test_path, test_resource_path};
    use crate::{assert_messages, settings};

    #[test_case(Rule::AsyncioDanglingTask, Path::new("RUF006.py"))]
    #[test_case(Rule::CollectionLiteralConcatenation, Path::new("RUF005.py"))]
    #[test_case(Rule::ExplicitFStringTypeConversion, Path::new("RUF010.py"))]
    #[test_case(Rule::FunctionCallInDataclassDefaultArgument, Path::new("RUF009.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_0.py"))]
    #[test_case(Rule::ImplicitOptional, Path::new("RUF013_1.py"))]
    #[test_case(Rule::MutableClassDefault, Path::new("RUF012.py"))]
    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008.py"))]
    #[test_case(Rule::PairwiseOverZipped, Path::new("RUF007.py"))]
    #[test_case(Rule::StaticKeyDictComprehension, Path::new("RUF011.py"))]
    #[test_case(Rule::UnreachableCode, Path::new("RUF014.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
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
            &settings::Settings {
                target_version: PythonVersion::Py39,
                ..settings::Settings::for_rule(Rule::ImplicitOptional)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn confusables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/confusables.py"),
            &settings::Settings {
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']),
                ..settings::Settings::for_rules(vec![
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
            &settings::Settings::for_rules(vec![Rule::UnusedVariable, Rule::AmbiguousVariableName]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_0() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_0.py"),
            &settings::Settings {
                external: FxHashSet::from_iter(vec!["V101".to_string()]),
                ..settings::Settings::for_rules(vec![
                    Rule::UnusedNOQA,
                    Rule::LineTooLong,
                    Rule::UnusedImport,
                    Rule::UnusedVariable,
                    Rule::TabIndentation,
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
            &settings::Settings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_2() -> Result<()> {
        let mut settings =
            settings::Settings::for_rules(vec![Rule::UnusedNOQA, Rule::UnusedImport]);

        settings.per_file_ignores = resolve_per_file_ignores(vec![PerFileIgnore::new(
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
            &settings::Settings::for_rules(vec![
                Rule::UnusedNOQA,
                Rule::LineTooLong,
                Rule::UndefinedName,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/flake8_noqa.py"),
            &settings::Settings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_noqa.py"),
            &settings::Settings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_targeted_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/ruff_targeted_noqa.py"),
            &settings::Settings::for_rules(vec![Rule::UnusedImport, Rule::UnusedVariable]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/redirects.py"),
            &settings::Settings::for_rules(vec![Rule::NonPEP604Annotation]),
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
        let messages = lint_pyproject_toml(source_file)?;
        assert_messages!(snapshot, messages);
        Ok(())
    }
}
