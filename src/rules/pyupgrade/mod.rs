mod fixes;
pub(crate) mod rules;
pub mod settings;
pub(crate) mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;
    use crate::settings::types::PythonVersion;

    #[test_case(RuleCode::UP001, Path::new("UP001.py"); "UP001")]
    #[test_case(RuleCode::UP003, Path::new("UP003.py"); "UP003")]
    #[test_case(RuleCode::UP004, Path::new("UP004.py"); "UP004")]
    #[test_case(RuleCode::UP005, Path::new("UP005.py"); "UP005")]
    #[test_case(RuleCode::UP006, Path::new("UP006.py"); "UP006")]
    #[test_case(RuleCode::UP007, Path::new("UP007.py"); "UP007")]
    #[test_case(RuleCode::UP008, Path::new("UP008.py"); "UP008")]
    #[test_case(RuleCode::UP009, Path::new("UP009_0.py"); "UP009_0")]
    #[test_case(RuleCode::UP009, Path::new("UP009_1.py"); "UP009_1")]
    #[test_case(RuleCode::UP009, Path::new("UP009_2.py"); "UP009_2")]
    #[test_case(RuleCode::UP009, Path::new("UP009_3.py"); "UP009_3")]
    #[test_case(RuleCode::UP009, Path::new("UP009_4.py"); "UP009_4")]
    #[test_case(RuleCode::UP010, Path::new("UP010.py"); "UP010")]
    #[test_case(RuleCode::UP011, Path::new("UP011_0.py"); "UP011_0")]
    #[test_case(RuleCode::UP011, Path::new("UP011_1.py"); "UP011_1")]
    #[test_case(RuleCode::UP012, Path::new("UP012.py"); "UP012")]
    #[test_case(RuleCode::UP013, Path::new("UP013.py"); "UP013")]
    #[test_case(RuleCode::UP014, Path::new("UP014.py"); "UP014")]
    #[test_case(RuleCode::UP015, Path::new("UP015.py"); "UP015")]
    #[test_case(RuleCode::UP016, Path::new("UP016.py"); "UP016")]
    #[test_case(RuleCode::UP018, Path::new("UP018.py"); "UP018")]
    #[test_case(RuleCode::UP019, Path::new("UP019.py"); "UP019")]
    #[test_case(RuleCode::UP021, Path::new("UP021.py"); "UP021")]
    #[test_case(RuleCode::UP022, Path::new("UP022.py"); "UP022")]
    #[test_case(RuleCode::UP023, Path::new("UP023.py"); "UP023")]
    #[test_case(RuleCode::UP024, Path::new("UP024_0.py"); "UP024_0")]
    #[test_case(RuleCode::UP024, Path::new("UP024_1.py"); "UP024_1")]
    #[test_case(RuleCode::UP024, Path::new("UP024_2.py"); "UP024_2")]
    #[test_case(RuleCode::UP024, Path::new("UP024_3.py"); "UP024_3")]
    #[test_case(RuleCode::UP025, Path::new("UP025.py"); "UP025")]
    #[test_case(RuleCode::UP026, Path::new("UP026.py"); "UP026")]
    #[test_case(RuleCode::UP027, Path::new("UP027.py"); "UP027")]
    #[test_case(RuleCode::UP028, Path::new("UP028_0.py"); "UP028_0")]
    #[test_case(RuleCode::UP028, Path::new("UP028_1.py"); "UP028_1")]
    #[test_case(RuleCode::UP029, Path::new("UP029.py"); "UP029")]
    #[test_case(RuleCode::UP030, Path::new("UP030_0.py"); "UP030_0")]
    #[test_case(RuleCode::UP030, Path::new("UP030_1.py"); "UP030_1")]
    #[test_case(RuleCode::UP032, Path::new("UP032.py"); "UP032")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(RuleCode::UP006)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(RuleCode::UP006)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(RuleCode::UP007)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(RuleCode::UP007)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn datetime_utc_alias_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/UP017.py"),
            &settings::Settings {
                target_version: PythonVersion::Py311,
                ..settings::Settings::for_rule(RuleCode::UP017)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
