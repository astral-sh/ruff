//! Module for Ruff-specific rules.

pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashSet;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;
    #[test_case(RuleCode::RUF004, Path::new("RUF004.py"); "RUF004")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn confusables() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/confusables.py"),
            &settings::Settings {
                allowed_confusables: FxHashSet::from_iter(['−', 'ρ', '∗']).into(),
                ..settings::Settings::for_rules(vec![
                    RuleCode::RUF001,
                    RuleCode::RUF002,
                    RuleCode::RUF003,
                ])
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_0() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/RUF100_0.py"),
            &settings::Settings::for_rules(vec![
                RuleCode::RUF100,
                RuleCode::E501,
                RuleCode::F401,
                RuleCode::F841,
            ]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruf100_1() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/RUF100_1.py"),
            &settings::Settings::for_rules(vec![RuleCode::RUF100, RuleCode::F401]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn flake8_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/flake8_noqa.py"),
            &settings::Settings::for_rules(vec![RuleCode::F401, RuleCode::F841]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn ruff_noqa() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/ruff_noqa.py"),
            &settings::Settings::for_rules(vec![RuleCode::F401, RuleCode::F841]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn redirects() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/ruff/redirects.py"),
            &settings::Settings::for_rules(vec![RuleCode::UP007]),
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
