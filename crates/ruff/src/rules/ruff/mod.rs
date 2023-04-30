//! Ruff-specific rules.

pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::resolve_per_file_ignores;
    use crate::settings::types::PerFileIgnore;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::CollectionLiteralConcatenation, Path::new("RUF005.py"); "RUF005")]
    #[test_case(Rule::AsyncioDanglingTask, Path::new("RUF006.py"); "RUF006")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
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
    fn ruf100_0() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF100_0.py"),
            &settings::Settings::for_rules(vec![
                Rule::UnusedNOQA,
                Rule::LineTooLong,
                Rule::UnusedImport,
                Rule::UnusedVariable,
                Rule::TabIndentation,
            ]),
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

    #[test]
    fn ruff_pairwise_over_zipped() -> Result<()> {
        let diagnostics = test_path(
            Path::new("ruff/RUF007.py"),
            &settings::Settings::for_rules(vec![Rule::PairwiseOverZipped]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::MutableDataclassDefault, Path::new("RUF008.py"); "RUF008")]
    #[test_case(Rule::FunctionCallInDataclassDefaultArgument, Path::new("RUF009.py"); "RUF009")]
    fn mutable_defaults(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ruff").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
